//! Les snapshots canonisés (spec 002, R2.3, R4.5).
//!
//! Quand l'un des cinq déclencheurs se produit, on persiste une photo du
//! conteneur actif : l'arbre sémantique, canonisé, redacté, plafonné à 50 Ko.
//!
//! **Canoniser n'est pas embellir.** Deux photos du même écran doivent donner
//! deux fichiers identiques, sinon un diff entre deux occurrences signale des
//! changements qui n'ont pas eu lieu. La canonisation efface donc tout ce qui
//! varie sans que rien n'ait changé — espaces, nœuds anonymes vides — et **rien
//! d'autre** : elle ne réordonne pas, parce que l'ordre d'une interface est de
//! l'information.
//!
//! **La redaction précède l'écriture, toujours** (R4.5). Un snapshot est le
//! vecteur de PII le plus large du produit : il ramasse tout le texte visible
//! d'un écran, y compris les champs que l'opérateur n'a pas touchés.

use crate::moteur::Declencheur;
use crate::redaction::Redacteur;

/// Budget de taille après canonisation (R2.3).
pub const BUDGET_OCTETS: usize = 50 * 1024;
/// Profondeur maximale de descente — valeur du spike (design §2 b).
pub const PROFONDEUR_MAX: usize = 12;
/// Plafond de nœuds — valeur du spike.
pub const NOEUDS_MAX: usize = 1500;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Noeud {
    pub role: String,
    pub nom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valeur: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub enfants: Vec<Noeud>,
}

impl Noeud {
    pub fn feuille(role: &str, nom: &str) -> Self {
        Self {
            role: role.to_string(),
            nom: nom.to_string(),
            valeur: None,
            enfants: Vec::new(),
        }
    }

    pub fn avec(mut self, enfants: Vec<Noeud>) -> Self {
        self.enfants = enfants;
        self
    }

    pub fn valant(mut self, valeur: &str) -> Self {
        self.valeur = Some(valeur.to_string());
        self
    }

    fn compter(&self) -> usize {
        1 + self.enfants.iter().map(Noeud::compter).sum::<usize>()
    }

    fn profondeur(&self) -> usize {
        1 + self
            .enfants
            .iter()
            .map(Noeud::profondeur)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub declencheur: Declencheur,
    pub monotone_ms: u64,
    pub racine: Noeud,
    /// Vrai si un budget a mordu — jamais silencieux (R2.3).
    pub tronque: bool,
    pub noeuds: usize,
    pub octets: usize,
}

/// Effondre les espaces. Un libellé n'est pas deux libellés parce qu'un rendu a
/// mis deux espaces là où il en mettait un.
fn normaliser(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Canonise un sous-arbre, en respectant profondeur et budget de nœuds.
///
/// `restants` est décrémenté en place : le plafond porte sur l'arbre ENTIER, pas
/// sur chaque branche. Un plafond par branche laisserait un arbre large exploser
/// le budget global sans qu'aucune branche ne le dépasse.
fn canoniser(noeud: &Noeud, profondeur: usize, restants: &mut usize) -> Option<Noeud> {
    if profondeur >= PROFONDEUR_MAX || *restants == 0 {
        return None;
    }
    *restants -= 1;

    let role = normaliser(&noeud.role);
    let nom = normaliser(&noeud.nom);
    let valeur = noeud
        .valeur
        .as_deref()
        .map(normaliser)
        .filter(|v| !v.is_empty());

    let mut enfants = Vec::new();
    for e in &noeud.enfants {
        if let Some(c) = canoniser(e, profondeur + 1, restants) {
            enfants.push(c);
        }
    }

    // Un nœud sans rôle utile, sans nom, sans valeur et sans enfant ne décrit
    // rien : il ne fait qu'ajouter du bruit à un diff. On l'écarte — mais
    // seulement s'il est vraiment vide, jamais s'il porte des enfants.
    if nom.is_empty()
        && valeur.is_none()
        && enfants.is_empty()
        && (role.is_empty() || role == "generic")
    {
        *restants += 1;
        return None;
    }

    Some(Noeud {
        role,
        nom,
        valeur,
        enfants,
    })
}

/// Retire le niveau le plus profond de l'arbre. Rend `false` s'il n'y a plus
/// rien à retirer.
fn elaguer_plus_profond(noeud: &mut Noeud, profondeur_cible: usize, courant: usize) -> bool {
    if courant + 1 >= profondeur_cible {
        let avait = !noeud.enfants.is_empty();
        noeud.enfants.clear();
        return avait;
    }
    let mut touche = false;
    for e in &mut noeud.enfants {
        touche |= elaguer_plus_profond(e, profondeur_cible, courant + 1);
    }
    touche
}

/// Construit le snapshot : canonisation, redaction, budget.
///
/// L'ordre n'est pas indifférent. La redaction s'applique **après** la
/// canonisation et **avant** la mesure : redacter d'abord ferait porter le
/// budget sur des jetons plus longs que les valeurs d'origine, et la troncature
/// dépendrait alors de la clé HMAC du poste — deux machines n'auraient pas le
/// même snapshot pour le même écran.
pub fn construire(
    declencheur: Declencheur,
    monotone_ms: u64,
    racine: &Noeud,
    redacteur: &Redacteur,
) -> Snapshot {
    let mut restants = NOEUDS_MAX;
    let mut tronque = false;

    let canonise = canoniser(racine, 0, &mut restants).unwrap_or_else(|| Noeud::feuille("", ""));
    if restants == 0 || racine.profondeur() > PROFONDEUR_MAX {
        tronque = true;
    }

    let mut redacte = redacter_arbre(&canonise, redacteur);

    // R2.3 : ≤ 50 Ko APRÈS canonisation. On élague par niveaux, du plus profond
    // au moins profond : perdre les feuilles coûte moins que perdre la
    // structure, qui est ce qui rend deux snapshots comparables.
    let mut octets = mesurer(&redacte);
    let mut cible = redacte.profondeur();
    while octets > BUDGET_OCTETS && cible > 1 {
        cible -= 1;
        elaguer_plus_profond(&mut redacte, cible, 0);
        tronque = true;
        octets = mesurer(&redacte);
    }

    Snapshot {
        declencheur,
        monotone_ms,
        noeuds: redacte.compter(),
        octets,
        racine: redacte,
        tronque,
    }
}

fn mesurer(noeud: &Noeud) -> usize {
    serde_json::to_string(noeud).map(|s| s.len()).unwrap_or(0)
}

/// R4.5 : rôle, nom et valeur passent tous par le rédacteur.
///
/// Le rôle aussi : il est théoriquement structurel, mais rien n'empêche une
/// application de nommer un type de contrôle d'après une donnée. Le coût est nul
/// — un rôle sans PII ressort intact.
fn redacter_arbre(noeud: &Noeud, r: &Redacteur) -> Noeud {
    Noeud {
        role: r.redacter(&noeud.role),
        nom: r.redacter(&noeud.nom),
        valeur: noeud.valeur.as_deref().map(|v| r.redacter(v)),
        enfants: noeud.enfants.iter().map(|e| redacter_arbre(e, r)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cle::CleHmac;
    use crate::motifs::chercher;

    fn redacteur() -> Redacteur {
        Redacteur::new(&CleHmac::generer().expect("alea"))
    }

    fn arbre_profond(n: usize) -> Noeud {
        let mut courant = Noeud::feuille("text", "feuille");
        for i in 0..n {
            courant = Noeud::feuille("generic", &format!("niveau{i}")).avec(vec![courant]);
        }
        courant
    }

    // -- Canonisation --------------------------------------------------------

    #[test]
    fn les_espaces_multiples_sont_effondres() {
        // Deux photos du meme ecran doivent donner deux fichiers identiques.
        let a = construire(
            Declencheur::Soumission,
            0,
            &Noeud::feuille("button", "Enregistrer   la   fiche"),
            &redacteur(),
        );
        assert_eq!(a.racine.nom, "Enregistrer la fiche");
    }

    #[test]
    fn un_noeud_vide_et_anonyme_disparait() {
        let racine = Noeud::feuille("generic", "Panneau").avec(vec![
            Noeud::feuille("button", "Enregistrer"),
            Noeud::feuille("generic", "   "),
            Noeud::feuille("", ""),
        ]);
        let s = construire(Declencheur::Soumission, 0, &racine, &redacteur());
        assert_eq!(s.racine.enfants.len(), 1, "{:?}", s.racine.enfants);
        assert_eq!(s.racine.enfants[0].nom, "Enregistrer");
    }

    #[test]
    fn un_noeud_anonyme_qui_porte_des_enfants_survit() {
        // L'ecarter couperait la branche entiere : le vide d'un conteneur ne
        // dit rien du contenu qu'il porte.
        let racine =
            Noeud::feuille("generic", "Racine")
                .avec(vec![Noeud::feuille("generic", "")
                    .avec(vec![Noeud::feuille("button", "Enregistrer")])]);
        let s = construire(Declencheur::Soumission, 0, &racine, &redacteur());
        assert_eq!(s.racine.enfants.len(), 1);
        assert_eq!(s.racine.enfants[0].enfants[0].nom, "Enregistrer");
    }

    #[test]
    fn l_ordre_des_enfants_est_preserve() {
        // Trier donnerait des diffs plus propres et des snapshots faux :
        // l'ordre d'une interface est de l'information.
        let racine = Noeud::feuille("list", "L").avec(vec![
            Noeud::feuille("listitem", "Zebre"),
            Noeud::feuille("listitem", "Alpha"),
            Noeud::feuille("listitem", "Milieu"),
        ]);
        let s = construire(Declencheur::Soumission, 0, &racine, &redacteur());
        let noms: Vec<&str> = s.racine.enfants.iter().map(|e| e.nom.as_str()).collect();
        assert_eq!(noms, vec!["Zebre", "Alpha", "Milieu"]);
    }

    #[test]
    fn deux_canonisations_du_meme_arbre_sont_identiques() {
        let r = redacteur();
        let racine = arbre_profond(5);
        let a = construire(Declencheur::Soumission, 0, &racine, &r);
        let b = construire(Declencheur::Soumission, 0, &racine, &r);
        assert_eq!(a.racine, b.racine, "la canonisation doit etre deterministe");
    }

    // -- Budgets (R2.3) ------------------------------------------------------

    #[test]
    fn la_profondeur_est_plafonnee_et_le_dit() {
        let s = construire(
            Declencheur::Soumission,
            0,
            &arbre_profond(PROFONDEUR_MAX + 8),
            &redacteur(),
        );
        assert!(
            s.tronque,
            "R2.3 : une troncature ne doit jamais etre muette"
        );
        assert!(
            s.racine.profondeur() <= PROFONDEUR_MAX,
            "profondeur {}",
            s.racine.profondeur()
        );
    }

    #[test]
    fn un_arbre_dans_les_clous_n_est_pas_marque_tronque() {
        // Le controle temoin : si celui-ci criait a la troncature, le drapeau
        // ne voudrait plus rien dire et on cesserait de le regarder.
        let s = construire(Declencheur::Soumission, 0, &arbre_profond(4), &redacteur());
        assert!(!s.tronque);
    }

    #[test]
    fn le_nombre_de_noeuds_est_plafonne() {
        let large = Noeud::feuille("list", "L").avec(
            (0..NOEUDS_MAX + 500)
                .map(|i| Noeud::feuille("listitem", &format!("item {i}")))
                .collect(),
        );
        let s = construire(Declencheur::Soumission, 0, &large, &redacteur());
        assert!(s.noeuds <= NOEUDS_MAX, "noeuds : {}", s.noeuds);
        assert!(s.tronque);
    }

    #[test]
    fn le_budget_de_cinquante_kilo_octets_est_tenu() {
        // Des noms longs : le plafond de noeuds ne suffit pas a tenir la taille.
        let gros = Noeud::feuille("list", "L").avec(
            (0..NOEUDS_MAX - 1)
                .map(|i| Noeud::feuille("listitem", &format!("{i} {}", "x".repeat(200))))
                .collect(),
        );
        let s = construire(Declencheur::Soumission, 0, &gros, &redacteur());
        assert!(s.octets <= BUDGET_OCTETS, "octets : {}", s.octets);
        assert!(s.tronque);
    }

    #[test]
    fn l_elagage_garde_la_structure_et_lache_les_feuilles() {
        // Perdre les feuilles coute moins que perdre la structure : c'est elle
        // qui rend deux snapshots comparables.
        let gros = Noeud::feuille("generic", "Racine").avec(
            (0..40)
                .map(|i| {
                    Noeud::feuille("generic", &format!("section {i}")).avec(
                        (0..40)
                            .map(|j| {
                                Noeud::feuille("text", &"y".repeat(120)).valant(&format!("{j}"))
                            })
                            .collect(),
                    )
                })
                .collect(),
        );
        let s = construire(Declencheur::Soumission, 0, &gros, &redacteur());
        assert!(s.octets <= BUDGET_OCTETS);
        assert!(!s.racine.enfants.is_empty(), "la premiere strate survit");
    }

    // -- R4.5 : la redaction precede l'ecriture ------------------------------

    #[test]
    fn aucune_pii_ne_survit_dans_un_snapshot() {
        // Le report explicite de la tache 3 : c'est ici que les snapshots
        // apparaissent, donc ici que R4.5 se prouve sur eux.
        let racine = Noeud::feuille("document", "Fiche de jean.dupont@exemple.fr").avec(vec![
            Noeud::feuille("textbox", "Telephone").valant("06 12 34 56 78"),
            Noeud::feuille("textbox", "RIB").valant("FR7630006000011234567890189"),
            Noeud::feuille("text", "Carte 4970 1234 5678 9012"),
        ]);
        let s = construire(Declencheur::SaisiePuisInactivite, 42, &racine, &redacteur());

        let serialise = serde_json::to_string(&s).expect("serialisable");
        assert!(
            chercher(&serialise).is_empty(),
            "R4.5 : une PII a survecu — {serialise}"
        );
        assert!(serialise.contains("EMAIL_"), "et les jetons sont bien la");
        assert!(serialise.contains("TEL_FR_"));
        assert!(serialise.contains("IBAN_"));
        assert!(serialise.contains("CARTE_"));
    }

    #[test]
    fn le_meme_ecran_donne_les_memes_jetons() {
        // Sans ca, deux snapshots du meme ecran ne seraient pas comparables et
        // le diff signalerait des changements qui n'ont pas eu lieu.
        let r = redacteur();
        let racine = Noeud::feuille("textbox", "Contact").valant("jean@exemple.fr");
        let a = construire(Declencheur::Soumission, 0, &racine, &r);
        let b = construire(Declencheur::Soumission, 999, &racine, &r);
        assert_eq!(a.racine, b.racine);
    }

    #[test]
    fn la_troncature_ne_depend_pas_de_la_cle_du_poste() {
        // La redaction s'applique APRES la canonisation : si le budget portait
        // sur des jetons, deux machines n'auraient pas le meme snapshot du meme
        // ecran, et les corpus ne seraient plus comparables entre postes.
        let racine = Noeud::feuille("list", "L").avec(
            (0..300)
                .map(|i| Noeud::feuille("listitem", &format!("client{i}@exemple.fr")))
                .collect(),
        );
        let a = construire(Declencheur::Soumission, 0, &racine, &redacteur());
        let b = construire(Declencheur::Soumission, 0, &racine, &redacteur());
        assert_eq!(a.noeuds, b.noeuds, "meme nombre de noeuds retenus");
        assert_eq!(a.tronque, b.tronque);
    }

    #[test]
    fn le_declencheur_et_l_instant_voyagent_avec_le_snapshot() {
        let s = construire(
            Declencheur::BasculeAvecRetour,
            12_345,
            &Noeud::feuille("button", "X"),
            &redacteur(),
        );
        assert_eq!(s.declencheur, Declencheur::BasculeAvecRetour);
        assert_eq!(s.monotone_ms, 12_345);
    }
}
