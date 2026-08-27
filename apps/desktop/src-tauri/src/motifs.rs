//! La bibliothèque de motifs PII, côté Rust (spec 002, R4.1).
//!
//! La source de vérité est `MOTIFS_PII` dans `packages/episode-spec`. Ce module
//! consomme le **miroir JSON** que `scripts/generer-motifs.mjs` en projette, et
//! `--verifier` échoue en CI si les deux ont divergé. Il n'existe donc jamais
//! deux listes à tenir à jour, seulement une liste et une projection contrôlée.
//!
//! `include_str!` plutôt qu'une lecture disque : si le miroir manque ou est
//! illisible, la compilation échoue. Un capteur qui démarre avec une
//! bibliothèque vide redacterait tout en silence — c'est-à-dire rien.

use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};

/// Le miroir, figé dans le binaire à la compilation.
const MIROIR: &str = include_str!("../../../../packages/episode-spec/motifs.json");

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MotifPii {
    #[serde(rename = "type")]
    pub type_pii: String,
    pub source: String,
    pub drapeaux: String,
    /// Qui l'emporte en cas de chevauchement. Le plus petit gagne.
    pub priorite: u32,
    pub note: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Miroir {
    version: u32,
    motifs: Vec<MotifPii>,
    /// Le filet du juge (v4). Voir `chercher_compact`.
    compact: Vec<MotifPii>,
}

/// Ramène les blancs exotiques à l'espace ASCII, avant toute recherche.
///
/// Miroir exact de `normaliserBlancs`. Les motifs sont compilés `unicode(false)`
/// pour que les deux moteurs lisent la même chaîne de la même façon ; le prix de
/// cette garantie, c'est qu'un `U+00A0` entre deux groupes de chiffres n'est pas
/// un séparateur reconnu. « 06<NBSP>12<NBSP>34… » traversait la redaction en
/// clair, et l'insécable est ce que produisent Word, les signatures de courriel
/// et beaucoup de champs de CRM.
///
/// Les caractères de largeur nulle deviennent une espace et non rien : sinon
/// `06<ZWSP>12` se lirait `0612` et on inventerait un numéro que personne n'a
/// écrit.
pub fn normaliser_blancs(texte: &str) -> String {
    texte
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c
            } else if c.is_whitespace()
                || matches!(c, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}')
            {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// La même chaîne, réduite à ses caractères signifiants.
///
/// Miroir exact de `compacter`. Tout ce qui n'est ni alphanumérique ASCII ni `+`
/// disparaît : une graphie que la bibliothèque ne connaît pas encore s'y réduit
/// à la même suite de chiffres que la graphie canonique.
pub fn compacter(texte: &str) -> String {
    texte
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+')
        .collect()
}

/// Une PII repérée : son type et ses bornes en octets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub type_pii: String,
    pub debut: usize,
    pub fin: usize,
}

impl Occurrence {
    pub fn longueur(&self) -> usize {
        self.fin.saturating_sub(self.debut)
    }
}

struct Compile {
    version: u32,
    motifs: Vec<(String, u32, Regex)>,
    compact: Vec<(String, u32, Regex)>,
}

fn compile() -> &'static Compile {
    static CACHE: OnceLock<Compile> = OnceLock::new();
    CACHE.get_or_init(|| {
        let miroir: Miroir =
            serde_json::from_str(MIROIR).expect("motifs.json illisible : la CI aurait du le voir");
        let compiler = |liste: &Vec<MotifPii>| -> Vec<(String, u32, Regex)> {
            liste
            .iter()
            .map(|m| {
                // `unicode(false)` : en JavaScript, `\d`, `\w` et `\b` sont
                // ASCII. En Rust ils sont Unicode par defaut, si bien que `\d`
                // matcherait les chiffres arabes-indiens et `\b` changerait de
                // sens autour des accents. Sans ce reglage, les deux
                // implementations liraient la MEME chaine differemment — la
                // divergence exacte que le miroir est cense empecher.
                //
                // Le seul drapeau qui a du sens ici est `g`. En JavaScript il
                // rend `exec` iteratif ; en Rust, `find_iter` est global par
                // nature. Un autre drapeau — `i`, `m`, `s` — changerait la
                // semantique d un cote sans que l autre le sache, et le miroir
                // cesserait de garantir quoi que ce soit. On refuse plutot que
                // d ignorer.
                assert_eq!(
                    m.drapeaux, "g",
                    "motif {} : drapeau « {} » non supporte par le miroir",
                    m.type_pii, m.drapeaux
                );
                let re = RegexBuilder::new(&m.source)
                    .unicode(false)
                    .build()
                    .unwrap_or_else(|e| {
                        panic!(
                            "motif {} ({}) refuse par le moteur Rust : {e}",
                            m.type_pii, m.note
                        )
                    });
                (m.type_pii.clone(), m.priorite, re)
            })
            .collect()
        };
        Compile {
            version: miroir.version,
            motifs: compiler(&miroir.motifs),
            compact: compiler(&miroir.compact),
        }
    })
}

pub fn version() -> u32 {
    compile().version
}

pub fn types() -> Vec<String> {
    compile().motifs.iter().map(|(t, _, _)| t.clone()).collect()
}

/// Toutes les occurrences, triées comme le fait `chercherPii` côté TypeScript :
/// par position, puis par type à position égale.
///
/// **Les bornes portent sur `normaliser_blancs(texte)`, pas sur `texte`.** Qui
/// remplace doit donc normaliser d'abord — c'est ce que fait `Redacteur`.
pub fn chercher(texte: &str) -> Vec<Occurrence> {
    let texte = normaliser_blancs(texte);
    let texte = texte.as_str();
    let mut trouvees: Vec<Occurrence> = Vec::new();
    for (type_pii, _, re) in &compile().motifs {
        for m in re.find_iter(texte) {
            trouvees.push(Occurrence {
                type_pii: type_pii.clone(),
                debut: m.start(),
                fin: m.end(),
            });
        }
    }
    trouvees.sort_by(|a, b| a.debut.cmp(&b.debut).then(a.type_pii.cmp(&b.type_pii)));
    trouvees
}

/// Le filet du juge : les motifs appliqués au texte **compacté** (v4).
///
/// R4.6 valide la redaction avec la bibliothèque même qui a servi à redacter. Un
/// juge adossé à ce qu'il contrôle est aveugle par construction : tout trou de
/// motif passe deux fois, à l'écriture puis à la validation, et l'épisode
/// ressort gradé « redaction validée ». C'est ce qui s'est produit trois fois.
///
/// Ce filet ne partage pas les motifs et ne redacte jamais. Il refuse de
/// valider, ce qui est le bon sens de l'erreur : le jour où il parle seul, c'est
/// la bibliothèque qu'il faut corriger.
pub fn chercher_compact(texte: &str) -> Vec<Occurrence> {
    let texte = compacter(&normaliser_blancs(texte));
    let mut trouvees: Vec<Occurrence> = Vec::new();
    for (type_pii, _, re) in &compile().compact {
        for m in re.find_iter(&texte) {
            trouvees.push(Occurrence {
                type_pii: type_pii.clone(),
                debut: m.start(),
                fin: m.end(),
            });
        }
    }
    trouvees.sort_by(|a, b| a.debut.cmp(&b.debut).then(a.type_pii.cmp(&b.type_pii)));
    trouvees
}

/// Arbitre les chevauchements — miroir exact de `resoudreChevauchements`.
///
/// Glouton : priorité croissante, puis longueur décroissante, puis position.
/// Deux occurrences qui se chevauchent ne peuvent pas être remplacées toutes les
/// deux sans produire un jeton tronqué au milieu d'un autre ; et laisser l'ordre
/// d'évaluation décider donnerait deux jetons différents pour la même entité
/// selon le moteur, donc une jointure perdue.
pub fn resoudre_chevauchements(occurrences: &[Occurrence]) -> Vec<Occurrence> {
    let priorites: std::collections::BTreeMap<&str, u32> = compile()
        .motifs
        .iter()
        .map(|(t, p, _)| (t.as_str(), *p))
        .collect();

    let mut candidats: Vec<&Occurrence> = occurrences.iter().collect();
    candidats.sort_by(|a, b| {
        let pa = priorites
            .get(a.type_pii.as_str())
            .copied()
            .unwrap_or(u32::MAX);
        let pb = priorites
            .get(b.type_pii.as_str())
            .copied()
            .unwrap_or(u32::MAX);
        pa.cmp(&pb)
            .then(b.longueur().cmp(&a.longueur()))
            .then(a.debut.cmp(&b.debut))
    });

    let mut retenues: Vec<Occurrence> = Vec::new();
    for c in candidats {
        let chevauche = retenues.iter().any(|r| c.debut < r.fin && r.debut < c.fin);
        if !chevauche {
            retenues.push(c.clone());
        }
    }
    retenues.sort_by_key(|o| o.debut);
    retenues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct OccurrenceAttendue {
        #[serde(rename = "type")]
        type_pii: String,
        index: usize,
    }

    #[derive(serde::Deserialize)]
    struct Cas {
        entree: String,
        occurrences: Vec<OccurrenceAttendue>,
        retenues: Vec<OccurrenceAttendue>,
    }

    #[derive(serde::Deserialize)]
    struct Vecteurs {
        version: u32,
        cas: Vec<Cas>,
    }

    const VECTEURS: &str =
        include_str!("../../../../packages/episode-spec/vecteurs-redaction.json");

    #[test]
    fn l_iban_l_emporte_sur_le_telephone_qu_il_contient() {
        let brutes = chercher("Virement sur FR7630006000011234567890189");
        assert!(
            brutes.len() >= 2,
            "le chevauchement doit exister : {brutes:?}"
        );
        let retenues = resoudre_chevauchements(&brutes);
        assert_eq!(
            retenues
                .iter()
                .map(|o| o.type_pii.as_str())
                .collect::<Vec<_>>(),
            vec!["IBAN"]
        );
    }

    #[test]
    fn un_numero_francais_rend_toujours_tel_fr() {
        for forme in ["+33 6 12 34 56 78", "+33612345678", "06 12 34 56 78"] {
            let retenues = resoudre_chevauchements(&chercher(&format!("tel {forme}")));
            assert_eq!(
                retenues
                    .iter()
                    .map(|o| o.type_pii.as_str())
                    .collect::<Vec<_>>(),
                vec!["TEL_FR"],
                "graphie « {forme} » : un meme numero doit rendre un meme jeton"
            );
        }
    }

    #[test]
    fn les_retenues_ne_se_chevauchent_jamais() {
        let retenues = resoudre_chevauchements(&chercher(
            "FR7630006000011234567890189 puis a@b.fr puis 4970 1234 5678 9012",
        ));
        for paire in retenues.windows(2) {
            assert!(
                paire[1].debut >= paire[0].fin,
                "chevauchement residuel : {paire:?}"
            );
        }
    }

    #[test]
    fn tous_les_motifs_compilent_dans_le_moteur_rust() {
        // Le moteur Rust n'a PAS d'anticipation (`(?!…)`) ni de rétrospection.
        // Si un motif en introduit une, ce test tombe ici plutôt qu'en
        // production, et la bibliotheque doit revenir au sous-ensemble commun
        // aux trois moteurs. Le chargement lui-meme panique dans ce cas : il
        // suffit donc de le declencher.
        let types = types();
        assert!(!types.is_empty(), "aucun motif charge");
        assert!(types.contains(&"EMAIL".to_string()));
        assert!(version() >= 3, "version du miroir : {}", version());
    }

    /// LE test qui justifie tout le dispositif du miroir.
    ///
    /// Il ne compare pas des chaînes de motifs — deux moteurs peuvent lire la
    /// même chaîne différemment — mais les SORTIES sur les mêmes entrées.
    #[test]
    fn rust_et_typescript_voient_exactement_la_meme_chose() {
        let v: Vecteurs = serde_json::from_str(VECTEURS).expect("vecteurs illisibles");
        assert_eq!(
            v.version,
            version(),
            "les vecteurs et le miroir ne sont pas de la meme version"
        );

        let mut desaccords = Vec::new();
        for cas in &v.cas {
            // L'index TypeScript compte en unites UTF-16, l'index Rust en
            // octets. Sur de l'ASCII, les deux coincident ; ailleurs, ils ne
            // seraient pas comparables.
            //
            // v4 : c'est la forme NORMALISEE qui doit etre ASCII, pas l'entree.
            // Exiger l'ASCII sur le brut interdisait les vecteurs a insecable,
            // donc interdisait de tester la classe de fuite qui a le plus
            // longtemps traverse la redaction. On protegeait la comparabilite
            // en supprimant le cas a comparer.
            assert!(
                normaliser_blancs(&cas.entree).is_ascii(),
                "vecteur non-ASCII apres normalisation : {:?}",
                cas.entree
            );

            let obtenu: Vec<(String, usize)> = chercher(&cas.entree)
                .into_iter()
                .map(|o| (o.type_pii, o.debut))
                .collect();
            let attendu: Vec<(String, usize)> = cas
                .occurrences
                .iter()
                .map(|o| (o.type_pii.clone(), o.index))
                .collect();

            if obtenu != attendu {
                desaccords.push(format!(
                    "  detection {:?}\n    TypeScript : {attendu:?}\n    Rust       : {obtenu:?}",
                    cas.entree
                ));
            }

            // L'arbitrage compte AUTANT que la détection : c'est lui qui décide
            // quel jeton un texte produira, donc si les jointures du graphe
            // d'entités tiendront. Deux moteurs qui détectent pareil mais
            // arbitrent différemment produiraient deux jetons pour une même
            // entité — exactement la panne que le miroir doit exclure.
            let retenu: Vec<(String, usize)> = resoudre_chevauchements(&chercher(&cas.entree))
                .into_iter()
                .map(|o| (o.type_pii, o.debut))
                .collect();
            let retenu_attendu: Vec<(String, usize)> = cas
                .retenues
                .iter()
                .map(|o| (o.type_pii.clone(), o.index))
                .collect();

            if retenu != retenu_attendu {
                desaccords.push(format!(
                    "  arbitrage {:?}\n    TypeScript : {retenu_attendu:?}\n    Rust       : {retenu:?}",
                    cas.entree
                ));
            }
        }

        assert!(
            desaccords.is_empty(),
            "{} desaccord(s) entre les deux implementations :\n{}",
            desaccords.len(),
            desaccords.join("\n")
        );
    }

    #[test]
    fn les_chiffres_non_ascii_ne_comptent_pas_comme_des_chiffres() {
        // En Rust, `\d` est Unicode par defaut : sans `unicode(false)`, la carte
        // en chiffres arabes-indiens serait detectee cote Rust et pas cote JS.
        let arabe = "١٢٣٤ ١٢٣٤ ١٢٣٤ ١٢٣٤";
        assert!(
            chercher(arabe).is_empty(),
            "le moteur Rust doit se comporter comme JavaScript"
        );
    }

    #[test]
    fn le_numero_qui_fuyait_en_v1_est_bien_vu() {
        let trouvees = chercher("Ligne directe +33 6 12 34 56 78 merci");
        assert!(
            trouvees.iter().any(|o| o.type_pii == "TEL_FR"),
            "D24 : cette graphie traversait la redaction, obtenu {trouvees:?}"
        );
    }

    // -- v4 : les graphies qui traversaient la redaction (D29) --------------

    #[test]
    fn les_graphies_trouvees_par_revue_adverse_sont_detectees() {
        // Chacune est une forme d'affichage courante d'un numero francais, pas
        // une curiosite de laboratoire : en-tetes de courriel, signatures,
        // champs de CRM.
        for texte in [
            "Mobile +33 (0)6 12 34 56 78",
            "Standard +33 (0)1 42 68 53 00",
            "Direct +33 (0)612345678",
            "Depuis l etranger 0033 6 12 34 56 78",
            "Ligne 06\u{a0}12\u{a0}34\u{a0}56\u{a0}78",
            "Ligne 06\u{202f}12\u{202f}34\u{202f}56\u{202f}78",
            "Ligne 06\u{200b}12\u{200b}34\u{200b}56\u{200b}78",
        ] {
            let types: Vec<String> = chercher(texte).into_iter().map(|o| o.type_pii).collect();
            assert!(
                types.iter().any(|t| t == "TEL_FR"),
                "non detecte : {texte:?} -> {types:?}"
            );
        }
    }

    #[test]
    fn la_largeur_nulle_devient_une_espace_pas_rien() {
        // Sinon `06<ZWSP>12` se lirait `0612` et on inventerait une graphie que
        // personne n'a ecrite.
        assert_eq!(normaliser_blancs("06\u{200b}12"), "06 12");
    }

    #[test]
    fn le_filet_voit_ce_que_la_bibliotheque_raterait() {
        // Le tiret cadratin comme separateur : aucun motif ne le connait. Le
        // filet, si — et c'est exactement sa raison d'etre.
        let exotique = "06\u{2014}12\u{2014}34\u{2014}56\u{2014}78";
        let vus: Vec<String> = chercher(exotique).into_iter().map(|o| o.type_pii).collect();
        assert!(!vus.iter().any(|t| t == "TEL_FR"), "{vus:?}");
        let filet: Vec<String> = chercher_compact(exotique)
            .into_iter()
            .map(|o| o.type_pii)
            .collect();
        assert_eq!(filet, vec!["TEL_FR_COMPACT".to_string()]);
    }

    #[test]
    fn le_filet_ne_mord_pas_sur_ce_qui_n_est_pas_un_numero() {
        // Un faux positif ici declasse un episode honnete sans recours.
        for texte in [
            "1767225600000",
            "2026-01-14T09:12:03.000Z",
            "Montant 1 234,56 EUR",
            "Code postal 75011 Paris",
            "SIRET 12345678900011",
            "01JQA1B2C3D4E5F6G7H8J9K0M1",
            "TEL_FR_1a2b3c4d",
        ] {
            assert!(
                chercher_compact(texte).is_empty(),
                "faux positif du filet sur {texte:?}"
            );
        }
    }

    #[test]
    fn le_filet_est_le_meme_des_deux_cotes() {
        // Meme dispositif que les motifs : le miroir porte les deux listes, et
        // une divergence ferait diverger les grades sans que personne le voie.
        let miroir: Miroir = serde_json::from_str(MIROIR).unwrap();
        assert_eq!(
            miroir.compact.len(),
            1,
            "le filet doit rester minuscule : chaque motif y est un risque de faux positif"
        );
        assert_eq!(miroir.compact[0].type_pii, "TEL_FR_COMPACT");
    }

}
