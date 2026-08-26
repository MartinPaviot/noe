//! Pseudonymisation déterministe (spec 002, R4.1, R4.2, R4.5).
//!
//! Chaque PII repérée devient un jeton `TYPE_hash8` — `EMAIL_7f3a9c21` — obtenu
//! par HMAC-SHA256 de la valeur **normalisée**, sous la clé du poste.
//!
//! La normalisation avant hachage n'est pas une coquetterie : c'est ce qui fait
//! vivre le graphe d'entités. « 06 12 34 56 78 », « 0612345678 » et
//! « +33 6 12 34 56 78 » désignent la même personne ; sans normalisation ils
//! donneraient trois jetons différents, donc trois entités là où il n'y en a
//! qu'une, et les jointures que la spec 003 doit exploiter n'existeraient pas.
//!
//! Un placeholder aléatoire aurait été plus simple et aurait détruit exactement
//! cette propriété. C'est la raison d'être du HMAC déterministe (R4.2).

use ring::hmac;

use crate::cle::CleHmac;
use crate::motifs::{chercher, resoudre_chevauchements};
use crate::source::{Cible, GenreEvenement};

/// Longueur du condensat dans le jeton, en caractères hexadécimaux.
///
/// Huit caractères = 32 bits. Le risque d'anniversaire devient sensible autour
/// de 2^16 ≈ 65 000 entités distinctes sur une même installation — très au-delà
/// d'un corpus de poste, mais le chiffre est écrit ici plutôt que découvert plus
/// tard. Si un corpus s'en approchait, c'est cette constante qu'il faudrait
/// remonter, et la version de la bibliothèque avec elle.
const LONGUEUR_CONDENSAT: usize = 8;

pub struct Redacteur {
    cle: hmac::Key,
}

/// Ramène les graphies d'une même valeur à une forme unique, avant hachage.
///
/// Les règles sont propres à chaque type et volontairement peu ambitieuses :
/// mieux vaut deux jetons pour une même entité qu'un jeton pour deux entités
/// différentes. Une normalisation trop agressive fusionnerait des personnes.
fn normaliser(type_pii: &str, brut: &str) -> String {
    let chiffres = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();

    match type_pii {
        // La casse d'une adresse n'est pas significative en pratique.
        "EMAIL" => brut.trim().to_lowercase(),

        // Toutes les graphies d'un numéro français convergent vers +33XXXXXXXXX.
        // Sans ça, « 06… » et « +33 6… » seraient deux personnes.
        "TEL_FR" => {
            let d = chiffres(brut);
            let national = d
                .strip_prefix("33")
                .map(str::to_string)
                .or_else(|| d.strip_prefix('0').map(str::to_string))
                .unwrap_or(d);
            format!("+33{national}")
        }

        "TEL_INTL" => format!("+{}", chiffres(brut)),

        // Un IBAN s'écrit avec ou sans espaces, en majuscules par convention.
        "IBAN" => brut
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_uppercase(),

        "CARTE" => chiffres(brut),

        // Type inconnu : on ne devine pas, on se contente du minimum sûr.
        _ => brut.trim().to_lowercase(),
    }
}

impl Redacteur {
    pub fn new(cle: &CleHmac) -> Self {
        Self {
            cle: hmac::Key::new(hmac::HMAC_SHA256, cle.octets()),
        }
    }

    /// Le jeton d'une valeur. Déterministe pour la vie de l'installation (R4.2).
    pub fn jeton(&self, type_pii: &str, valeur: &str) -> String {
        let normalise = normaliser(type_pii, valeur);
        let signature = hmac::sign(&self.cle, normalise.as_bytes());
        let hexa: String = signature
            .as_ref()
            .iter()
            .take(LONGUEUR_CONDENSAT / 2)
            .map(|o| format!("{o:02x}"))
            .collect();
        format!("{type_pii}_{hexa}")
    }

    /// Remplace toute PII d'un texte par son jeton.
    ///
    /// Les remplacements se font **de la fin vers le début** : remplacer par
    /// l'avant décalerait toutes les positions suivantes, et les occurrences
    /// restantes viseraient à côté.
    pub fn redacter(&self, texte: &str) -> String {
        let retenues = resoudre_chevauchements(&chercher(texte));
        if retenues.is_empty() {
            return texte.to_string();
        }

        let mut sortie = texte.to_string();
        for o in retenues.iter().rev() {
            let brut = &texte[o.debut..o.fin];
            sortie.replace_range(o.debut..o.fin, &self.jeton(&o.type_pii, brut));
        }
        sortie
    }

    /// R4.5 : la redaction s'applique AUSSI aux noms accessibles et aux régions.
    ///
    /// C'est le vecteur du monde réel — « Email de Jean Dupont — jean@… » comme
    /// titre de fenêtre. Le HMAC étant déterministe, le ciblage survit : même
    /// nom, même jeton, l'égalité de cible tient toujours.
    pub fn redacter_cible(&self, cible: &Cible) -> Cible {
        Cible {
            role: cible.role.clone(),
            nom: self.redacter(&cible.nom),
            region: cible.region.as_deref().map(|r| self.redacter(r)),
        }
    }

    pub fn redacter_genre(&self, genre: &GenreEvenement) -> GenreEvenement {
        match genre {
            GenreEvenement::Focus(c) => GenreEvenement::Focus(self.redacter_cible(c)),
            GenreEvenement::Invocation(c) => GenreEvenement::Invocation(self.redacter_cible(c)),
            GenreEvenement::ChangementValeur(c) => {
                GenreEvenement::ChangementValeur(self.redacter_cible(c))
            }
            GenreEvenement::ChangementStructure(c) => {
                GenreEvenement::ChangementStructure(self.redacter_cible(c))
            }
            GenreEvenement::Saisie(c) => GenreEvenement::Saisie(self.redacter_cible(c)),
            GenreEvenement::Soumission(c) => GenreEvenement::Soumission(self.redacter_cible(c)),
            // Le titre d'une application peut porter le nom d'un client.
            GenreEvenement::BasculeApplication { vers } => GenreEvenement::BasculeApplication {
                vers: self.redacter(vers),
            },
            GenreEvenement::Veille => GenreEvenement::Veille,
            GenreEvenement::Reveil => GenreEvenement::Reveil,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motifs::chercher;

    fn redacteur() -> Redacteur {
        Redacteur::new(&CleHmac::generer().expect("alea"))
    }

    // -- Forme du jeton ----------------------------------------------------

    #[test]
    fn le_jeton_a_la_forme_type_hash8() {
        let r = redacteur();
        let j = r.jeton("EMAIL", "jean@exemple.fr");
        assert!(j.starts_with("EMAIL_"), "{j}");
        let hexa = j.trim_start_matches("EMAIL_");
        assert_eq!(hexa.len(), LONGUEUR_CONDENSAT);
        assert!(hexa.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn un_jeton_ne_ressemble_a_aucune_pii() {
        // Sinon la redaction produirait un texte que le validateur R4.6
        // rejetterait, et aucun episode ne pourrait jamais etre valide.
        let r = redacteur();
        let texte = format!(
            "{} {} {} {}",
            r.jeton("EMAIL", "a@b.fr"),
            r.jeton("TEL_FR", "0612345678"),
            r.jeton("IBAN", "FR7630006000011234567890189"),
            r.jeton("CARTE", "4970123456789012")
        );
        assert!(
            chercher(&texte).is_empty(),
            "un jeton est detecte comme PII : {texte}"
        );
    }

    // -- R4.2 : stabilite --------------------------------------------------

    #[test]
    fn le_meme_input_donne_toujours_le_meme_jeton() {
        let r = redacteur();
        let a = r.jeton("EMAIL", "jean@exemple.fr");
        let b = r.jeton("EMAIL", "jean@exemple.fr");
        assert_eq!(a, b);
    }

    #[test]
    fn deux_cles_differentes_donnent_deux_jetons_differents() {
        // Ce que la clé achète : deux postes ne peuvent pas rapprocher leurs
        // corpus par simple comparaison de jetons.
        let a = redacteur().jeton("EMAIL", "jean@exemple.fr");
        let b = redacteur().jeton("EMAIL", "jean@exemple.fr");
        assert_ne!(a, b);
    }

    // -- Normalisation : LA propriete qui fait vivre le graphe --------------

    #[test]
    fn toutes_les_graphies_d_un_numero_francais_donnent_un_seul_jeton() {
        let r = redacteur();
        let attendu = r.jeton("TEL_FR", "0612345678");
        for forme in [
            "06 12 34 56 78",
            "06.12.34.56.78",
            "06-12-34-56-78",
            "+33612345678",
            "+33 6 12 34 56 78",
            "+33-6-12-34-56-78",
        ] {
            assert_eq!(
                r.jeton("TEL_FR", forme),
                attendu,
                "« {forme} » designe la meme personne : un seul jeton"
            );
        }
    }

    #[test]
    fn la_casse_d_une_adresse_ne_cree_pas_deux_entites() {
        let r = redacteur();
        assert_eq!(
            r.jeton("EMAIL", "Jean.Dupont@Exemple.FR"),
            r.jeton("EMAIL", "jean.dupont@exemple.fr")
        );
    }

    #[test]
    fn un_iban_espace_ou_non_donne_le_meme_jeton() {
        let r = redacteur();
        assert_eq!(
            r.jeton("IBAN", "FR76 3000 6000 0112 3456 7890 189"),
            r.jeton("IBAN", "fr7630006000011234567890189")
        );
    }

    #[test]
    fn deux_personnes_differentes_ne_fusionnent_pas() {
        let r = redacteur();
        assert_ne!(
            r.jeton("TEL_FR", "0612345678"),
            r.jeton("TEL_FR", "0612345679"),
            "un chiffre de difference doit suffire"
        );
    }

    // -- R4.2 : non-collision ----------------------------------------------

    #[test]
    fn aucune_collision_sur_un_corpus_de_dix_mille_valeurs() {
        let r = redacteur();
        let mut vus = std::collections::HashSet::new();
        let mut collisions = Vec::new();
        for i in 0..10_000u32 {
            let jeton = r.jeton("EMAIL", &format!("personne{i}@exemple.fr"));
            if !vus.insert(jeton.clone()) {
                collisions.push(jeton);
            }
        }
        assert!(
            collisions.is_empty(),
            "{} collision(s) sur 10 000 : {collisions:?}",
            collisions.len()
        );
    }

    // -- Redaction d un texte ----------------------------------------------

    #[test]
    fn un_texte_redacte_ne_contient_plus_aucune_pii() {
        let r = redacteur();
        let sale = "Rappeler jean@exemple.fr au 06 12 34 56 78, RIB FR7630006000011234567890189";
        let propre = r.redacter(sale);

        assert!(
            chercher(&propre).is_empty(),
            "R4.6 : il reste une PII dans « {propre} »"
        );
        assert!(!propre.contains("jean@exemple.fr"));
        assert!(!propre.contains("06 12 34 56 78"));
        assert!(propre.contains("Rappeler"), "le texte utile doit survivre");
    }

    #[test]
    fn deux_pii_dans_un_meme_texte_sont_toutes_deux_remplacees() {
        let r = redacteur();
        let propre = r.redacter("de a@b.fr vers c@d.fr");
        assert_eq!(propre.matches("EMAIL_").count(), 2, "{propre}");
    }

    #[test]
    fn les_remplacements_ne_se_decalent_pas() {
        // Remplacer de l avant vers l arriere decalerait les positions
        // suivantes, et la derniere occurrence viserait a cote. Le jeton etant
        // plus long que « a@b.fr », le decalage serait visible.
        let r = redacteur();
        let propre = r.redacter("a@b.fr puis 06 12 34 56 78 puis c@d.fr");
        assert!(chercher(&propre).is_empty(), "{propre}");
        assert_eq!(propre.matches("EMAIL_").count(), 2, "{propre}");
        assert_eq!(propre.matches("TEL_FR_").count(), 1, "{propre}");
    }

    #[test]
    fn un_texte_sans_pii_ressort_intact() {
        let r = redacteur();
        let texte = "objection budget, rappeler apres arbitrage";
        assert_eq!(r.redacter(texte), texte);
    }

    #[test]
    fn un_iban_ne_devient_pas_un_jeton_telephonique() {
        // L arbitrage de chevauchement doit tenir jusque dans le rendu : sinon
        // le meme IBAN donnerait tantot IBAN_…, tantot TEL_FR_…, et le graphe
        // verrait deux entites.
        let r = redacteur();
        let propre = r.redacter("Virement sur FR7630006000011234567890189");
        assert!(propre.contains("IBAN_"), "{propre}");
        assert!(!propre.contains("TEL_FR_"), "{propre}");
    }

    // -- R4.5 : noms accessibles et regions ---------------------------------

    #[test]
    fn le_nom_accessible_est_redacte_et_la_cible_reste_ciblable() {
        let r = redacteur();
        let cible = Cible::new("link", "Email de jean@exemple.fr").dans("Boite de reception");
        let propre = r.redacter_cible(&cible);

        assert!(!propre.nom.contains("jean@exemple.fr"));
        assert!(propre.nom.contains("EMAIL_"));
        assert_eq!(propre.role, "link", "le role n est pas de la donnee");

        // R4.5 : « le HMAC deterministe preserve le ciblage ».
        assert_eq!(
            r.redacter_cible(&cible).nom,
            propre.nom,
            "meme nom, meme jeton : l egalite de cible doit tenir"
        );
    }

    #[test]
    fn la_region_est_redactee_elle_aussi() {
        let r = redacteur();
        let cible = Cible::new("textbox", "Note").dans("Dossier de 06 12 34 56 78");
        let propre = r.redacter_cible(&cible);
        assert!(propre.region.as_deref().unwrap_or("").contains("TEL_FR_"));
    }

    #[test]
    fn le_titre_d_application_est_redacte() {
        let r = redacteur();
        let genre = GenreEvenement::BasculeApplication {
            vers: "Outlook — jean@exemple.fr".into(),
        };
        match r.redacter_genre(&genre) {
            GenreEvenement::BasculeApplication { vers } => {
                assert!(!vers.contains("jean@exemple.fr"), "{vers}");
                assert!(vers.contains("EMAIL_"), "{vers}");
            }
            autre => panic!("genre change : {autre:?}"),
        }
    }

    #[test]
    fn les_evenements_sans_cible_traversent_sans_dommage() {
        let r = redacteur();
        assert_eq!(
            r.redacter_genre(&GenreEvenement::Veille),
            GenreEvenement::Veille
        );
        assert_eq!(
            r.redacter_genre(&GenreEvenement::Reveil),
            GenreEvenement::Reveil
        );
    }
}
