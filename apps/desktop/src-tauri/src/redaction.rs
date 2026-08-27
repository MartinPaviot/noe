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

/// Longueur du condensat dans le jeton, en caractères base32.
///
/// **Treize caractères = 65 bits**, et non plus huit hexadécimaux = 32 bits.
///
/// L'ancien commentaire annonçait un risque « sensible autour de 2^16 ≈ 65 000
/// entités ». Le calcul était faux d'un ordre de grandeur utile : le paradoxe
/// des anniversaires donne déjà **1,2 % de chance de collision sur 10 000
/// valeurs**, et 29 % sur 50 000. Le banc de non-collision l'a prouvé en
/// rougissant — il tire une clé neuve à chaque exécution, et il a fini par
/// tomber sur le cas.
///
/// Une collision ici ne fait pas perdre une jointure : elle en **invente** une.
/// Deux personnes différentes reçoivent le même pseudonyme et fusionnent dans le
/// graphe d'entités. C'est exactement l'erreur que la normalisation refuse de
/// commettre — « mieux vaut deux jetons pour une entité qu'un jeton pour deux
/// entités » — commise à l'autre bout de la chaîne.
///
/// **Base32 et non hexadécimal**, et c'est le second point. Un condensat plus
/// long en hexadécimal a une chance sur cent soixante environ de contenir une
/// suite de dix chiffres — donc de ressembler à un numéro de téléphone pour les
/// motifs et pour le filet du juge. L'alphabet RFC 4648 minuscule (`a-z2-7`) ne
/// contient ni `0` ni `1` : aucun jeton ne peut commencer une graphie
/// téléphonique française, qui exige `0`, `+33` ou `0033`.
const LONGUEUR_CONDENSAT: usize = 13;

/// L'alphabet base32 de la RFC 4648, en minuscules.
///
/// Sans `0` ni `1` : c'est ce qui garantit qu'un jeton ne sera jamais pris pour
/// un numéro par les motifs qu'on vient de durcir.
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Encode les huit premiers octets d'un condensat en treize caractères base32.
///
/// 8 octets = 64 bits ; 13 caractères en portent 65. Le dernier ne code donc
/// qu'un bit utile, et c'est sans importance : ce qui compte est que deux
/// condensats différents donnent deux chaînes différentes.
fn base32(octets: &[u8]) -> String {
    let mut sortie = String::with_capacity(LONGUEUR_CONDENSAT);
    let mut accumulateur: u64 = 0;
    let mut bits = 0u32;
    let mut restants = LONGUEUR_CONDENSAT;
    for o in octets {
        accumulateur = (accumulateur << 8) | u64::from(*o);
        bits += 8;
        while bits >= 5 && restants > 0 {
            bits -= 5;
            let indice = ((accumulateur >> bits) & 0x1f) as usize;
            sortie.push(BASE32[indice] as char);
            restants -= 1;
        }
    }
    // Les bits qui restent, complétés par des zéros — le cas du dernier
    // caractère quand 64 n'est pas un multiple de 5.
    while restants > 0 {
        let indice = ((accumulateur << (5 - bits)) & 0x1f) as usize;
        sortie.push(BASE32[indice] as char);
        restants -= 1;
        bits = 0;
    }
    sortie
}

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
            // L'indicatif d'abord, le zero de conduite ensuite — dans cet
            // ordre, et les deux. « +33 (0)6 12 34 56 78 » donne les chiffres
            // « 330612345678 » : oter le seul « 33 » laisserait « 0612345678 »,
            // donc un jeton different de « +33 6 12 34 56 78 » pour le meme
            // numero. Deux jetons pour une entite, c'est une jointure perdue.
            let sans_indicatif = d
                .strip_prefix("0033")
                .or_else(|| d.strip_prefix("33"))
                .unwrap_or(&d);
            let national = sans_indicatif.strip_prefix('0').unwrap_or(sans_indicatif);
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
        let condensat = base32(&signature.as_ref()[..8]);
        format!("{type_pii}_{condensat}")
    }

    /// Remplace toute PII d'un texte par son jeton.
    ///
    /// Les remplacements se font **de la fin vers le début** : remplacer par
    /// l'avant décalerait toutes les positions suivantes, et les occurrences
    /// restantes viseraient à côté.
    pub fn redacter(&self, texte: &str) -> String {
        // Les bornes rendues par `chercher` portent sur le texte NORMALISE :
        // c'est la seule facon qu'un insecable entre deux groupes de chiffres
        // ne fasse pas passer un numero en clair. On remplace donc dans la
        // meme chaine que celle qui a ete fouillee — sinon les positions
        // viseraient a cote des la premiere PII precedee d'un blanc exotique.
        let texte = crate::motifs::normaliser_blancs(texte);
        let texte = texte.as_str();
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
            // Copie et collage ne portent aucun texte : il n'y a rien a
            // redacter, et c'est voulu — le contenu d'un presse-papiers ne
            // traverse jamais le pipeline (R2.3).
            GenreEvenement::Copie => GenreEvenement::Copie,
            GenreEvenement::Collage { apparie } => GenreEvenement::Collage { apparie: *apparie },
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
    fn le_jeton_a_la_forme_type_base32() {
        let r = redacteur();
        let j = r.jeton("EMAIL", "jean@exemple.fr");
        assert!(j.starts_with("EMAIL_"), "{j}");
        let condensat = j.trim_start_matches("EMAIL_");
        assert_eq!(condensat.len(), LONGUEUR_CONDENSAT);
        assert!(
            condensat.bytes().all(|c| BASE32.contains(&c)),
            "hors alphabet : {condensat}"
        );
    }

    #[test]
    fn aucun_jeton_ne_peut_ressembler_a_un_numero() {
        // La garantie structurelle du choix de l'alphabet : ni `0` ni `1`. Un
        // condensat hexadecimal plus long aurait eu environ une chance sur cent
        // soixante de contenir une suite de dix chiffres, donc de se faire
        // prendre pour un telephone par le filet du juge — et de declasser un
        // episode honnete sans recours.
        assert!(!BASE32.contains(&b'0'));
        assert!(!BASE32.contains(&b'1'));
        let r = redacteur();
        for i in 0..500u32 {
            let j = r.jeton("TEL_FR", &format!("06123456{i:02}"));
            assert!(
                crate::motifs::chercher(&j).is_empty(),
                "la bibliotheque mord sur son propre jeton : {j}"
            );
            assert!(
                crate::motifs::chercher_compact(&j).is_empty(),
                "le filet mord sur un jeton : {j}"
            );
        }
    }

    #[test]
    fn base32_encode_sans_perdre_d_information() {
        // Deux condensats differents doivent donner deux chaines differentes,
        // sinon la collision qu'on vient de fermer reviendrait par la porte de
        // l'encodage.
        let mut vus = std::collections::HashSet::new();
        for i in 0..2_000u64 {
            let octets = i.to_be_bytes();
            assert!(vus.insert(base32(&octets)), "collision d encodage sur {i}");
        }
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
        // Dix mille valeurs : c'est le corpus ou 32 bits collisionnaient une
        // fois sur quatre-vingts. En 65 bits, la probabilite tombe sous
        // 3 x 10^-15, et ce banc cesse d'etre un tirage au sort.
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
