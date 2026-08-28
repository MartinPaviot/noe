//! Le cinquième déclencheur : copier-coller apparié (spec 002, R2.3).
//!
//! La règle est stricte et vaut la peine d'être relue :
//!
//! > l'appariement est RESTREINT aux copies ET collages survenus pendant
//! > l'épisode sur des surfaces activées ; un collage dont la copie vient
//! > d'ailleurs est enregistré `paste{paired:false}` et LE SYSTÈME NE DOIT
//! > JAMAIS lire ni hasher le contenu du presse-papiers d'origine externe
//! > (un gestionnaire de mots de passe peut y vivre).
//!
//! L'implémentation naïve — hacher le presse-papiers à chaque collage puis
//! chercher une correspondance — **viole la règle avant de la vérifier** : au
//! moment où l'on hache, on a déjà lu un contenu dont on ignore l'origine.
//!
//! Windows tient un **numéro de séquence** du presse-papiers, incrémenté à
//! chaque écriture. On l'enregistre à chaque copie qu'on observe. Au collage, on
//! ne lit **que ce numéro** : s'il est des nôtres, on possède déjà le condensat ;
//! sinon le collage est non apparié et le contenu n'est jamais touché.
//!
//! La règle devient ainsi une propriété du code plutôt qu'une consigne — et un
//! test le vérifie en faisant échouer toute lecture de contenu pendant un
//! collage.

use std::collections::BTreeMap;

/// Ce que le système d'exploitation sait du presse-papiers.
///
/// Les deux méthodes n'ont pas le même statut : `sequence` est anodine,
/// `texte` est une lecture de contenu et n'est légitime que sur une copie
/// observée.
pub trait PressePapiers {
    /// Numéro de séquence, incrémenté à chaque écriture. Ne révèle aucun contenu.
    fn sequence(&self) -> u64;
    /// Le contenu texte. **À n'appeler que sur une copie observée.**
    fn texte(&self) -> Option<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Collage {
    /// La copie a eu lieu pendant l'épisode : on en possédait déjà le condensat.
    Apparie { empreinte: String },
    /// Origine externe. Rien n'a été lu, et c'est le point.
    NonApparie,
}

/// Combien de copies on garde en mémoire pour un épisode.
///
/// Un épisode borné à soixante minutes ne produit pas des milliers de copies ;
/// le plafond existe pour qu'un cas pathologique ne fasse pas enfler la mémoire,
/// pas pour arbitrer quoi que ce soit.
const COPIES_MAX: usize = 256;

#[derive(Default)]
pub struct Appariement {
    /// séquence observée → condensat du contenu copié.
    notres: BTreeMap<u64, String>,
}

impl Appariement {
    pub fn nouveau() -> Self {
        Self::default()
    }

    /// Une copie vient d'être observée sur une surface activée.
    ///
    /// C'est le SEUL endroit du programme qui lise le contenu du presse-papiers.
    pub fn copie_observee(&mut self, pp: &dyn PressePapiers) {
        let sequence = pp.sequence();
        let Some(texte) = pp.texte() else {
            return;
        };
        if texte.is_empty() {
            return;
        }
        if self.notres.len() >= COPIES_MAX {
            // On oublie la plus ancienne : une copie très ancienne dans un même
            // épisode n'a plus de valeur d'appariement.
            if let Some(&plus_vieille) = self.notres.keys().next() {
                self.notres.remove(&plus_vieille);
            }
        }
        self.notres.insert(sequence, empreinte(&texte));
    }

    /// Un collage vient d'être observé. **Ne lit jamais le contenu.**
    pub fn coller(&self, pp: &dyn PressePapiers) -> Collage {
        match self.notres.get(&pp.sequence()) {
            Some(e) => Collage::Apparie {
                empreinte: e.clone(),
            },
            None => Collage::NonApparie,
        }
    }

    /// Combien de copies l'épisode a retenues. Sert aux tests des plafonds.
    #[cfg(test)]
    pub fn copies_connues(&self) -> usize {
        self.notres.len()
    }
}

/// Condensat tronqué du contenu copié.
///
/// Assez pour apparier une copie et un collage dans un même épisode, trop peu
/// pour reconstituer quoi que ce soit. Ce n'est PAS le HMAC de pseudonymisation :
/// on n'apparie ici que des occurrences internes à un épisode, et faire entrer
/// la clé du poste dans un usage aussi passager l'exposerait sans raison.
fn empreinte(texte: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for o in texte.as_bytes() {
        h ^= u64::from(*o);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("cp_{h:016x}")
}

/// Le presse-papiers de Windows.
///
/// `GetClipboardSequenceNumber` est un entier qui change à chaque écriture : il
/// ne révèle **aucun contenu** et ne demande aucune capacité particulière. C'est
/// tout ce qu'il faut pour savoir qu'une copie a eu lieu.
///
/// La détection du **collage**, elle, est en attente d'un arbitrage
/// (`docs/decisions.md`, D27) : Windows n'émet pas d'événement de collage, et la
/// seule voie fiable est un hook clavier système — une capacité que ce produit
/// ne s'octroie pas sans qu'on l'ait dit.
/// R2.3 — a-t-on le droit de lire le presse-papiers ?
///
/// La question est posée ici, en une fonction pure, parce que c'est le geste le
/// plus intrusif du capteur : une garde éparpillée dans une boucle de battement
/// n'est pas une garde, et surtout elle ne se teste pas.
///
/// Trois conditions, toutes nécessaires :
///
/// 1. **Une copie a été observée.** Sans frappe, pas de lecture — c'est ce qui
///    distingue Noe d'un gestionnaire de presse-papiers.
/// 2. **Elle a eu lieu sur une surface activée.** Le hook est posé sur tout le
///    bureau ; il dit qu'une combinaison a été pressée quelque part, pas qu'elle
///    l'a été là où on a le droit de regarder. Sans cette condition, un `Ctrl+C`
///    dans un gestionnaire de mots de passe était lu et haché — le cas que R2.3
///    nomme explicitement.
/// 3. **Le presse-papiers a vraiment changé.** Un `Ctrl+C` qui ne copie rien —
///    une console où il vaut interruption, une sélection vide — laissait
///    s'approprier ce qui traînait, c'est-à-dire ce qui avait été copié avant
///    l'épisode, ailleurs, et qu'on n'a jamais eu le droit de lire.
pub fn lecture_autorisee(
    copies: u64,
    surface: Option<&str>,
    liste: &crate::surfaces::ListeBlanche,
    sequence_avant: u64,
    sequence_maintenant: u64,
    en_pause: bool,
) -> bool {
    // **La pause compte ici.** Elle suspendait le journal et pas la lecture : le
    // presse-papiers etait ouvert et lu pendant que l'operateur avait demande
    // qu'on arrete de regarder. Rien n'etait ecrit, ce qui rendait la chose
    // invisible — mais « rien n'est ecrit » et « rien n'est lu » ne sont pas la
    // meme promesse, et c'est la seconde que la pause fait.
    !en_pause && copies > 0 && liste.autorise(surface) && sequence_maintenant != sequence_avant
}

pub struct PressePapiersWindows;

impl PressePapiers for PressePapiersWindows {
    fn sequence(&self) -> u64 {
        // SAFETY : lecture d'un compteur global, sans paramètre.
        u64::from(unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() })
    }

    /// Lit le texte du presse-papiers.
    ///
    /// **Appelée uniquement depuis `copie_observee`**, c'est-à-dire après qu'un
    /// `Ctrl+C` a été observé pendant un épisode ouvert. C'est ce qui rend la
    /// lecture légitime au sens de R2.3 : on ne lit pas « le presse-papiers »,
    /// on lit ce que l'opérateur vient de copier.
    fn texte(&self) -> Option<String> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, OpenClipboard,
        };
        use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
        use windows::Win32::System::Ole::CF_UNICODETEXT;

        // SAFETY : chaque ouverture est refermée, y compris sur les chemins
        // d'échec — un presse-papiers laissé ouvert bloque tout le poste.
        unsafe {
            if OpenClipboard(None).is_err() {
                return None;
            }
            let handle: HANDLE = match GetClipboardData(CF_UNICODETEXT.0.into()) {
                Ok(h) => HANDLE(h.0),
                Err(_) => {
                    let _ = CloseClipboard();
                    return None;
                }
            };
            let pointeur = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0));
            if pointeur.is_null() {
                let _ = CloseClipboard();
                return None;
            }

            // Longueur bornée : un presse-papiers peut contenir un document
            // entier, et on n'en veut qu'une empreinte.
            const MAX: usize = 8 * 1024;
            let large = pointeur as *const u16;
            let mut longueur = 0usize;
            while longueur < MAX && *large.add(longueur) != 0 {
                longueur += 1;
            }
            let tranche = std::slice::from_raw_parts(large, longueur);
            let texte = String::from_utf16_lossy(tranche);

            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
            let _ = CloseClipboard();
            Some(texte)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Un presse-papiers de test qui **crie** si on lit son contenu.
    ///
    /// C'est ce compteur qui transforme R2.3 d'une consigne en une propriété
    /// vérifiée : le test du collage externe échoue si une seule lecture a lieu.
    struct PressePapiersFaux {
        sequence: Cell<u64>,
        contenu: Cell<Option<&'static str>>,
        lectures: Cell<usize>,
    }

    impl PressePapiersFaux {
        fn new() -> Self {
            Self {
                sequence: Cell::new(1),
                contenu: Cell::new(Some("")),
                lectures: Cell::new(0),
            }
        }

        /// Quelqu'un écrit dans le presse-papiers : la séquence avance.
        fn ecrire(&self, texte: &'static str) {
            self.sequence.set(self.sequence.get() + 1);
            self.contenu.set(Some(texte));
        }
    }

    impl PressePapiers for PressePapiersFaux {
        fn sequence(&self) -> u64 {
            self.sequence.get()
        }
        fn texte(&self) -> Option<String> {
            self.lectures.set(self.lectures.get() + 1);
            self.contenu.get().map(str::to_string)
        }
    }

    #[test]
    fn en_pause_on_ne_lit_rien_du_tout() {
        // La pause suspendait le journal et pas la lecture : le presse-papiers
        // etait ouvert et lu pendant que l'operateur avait demande qu'on arrete
        // de regarder. Rien n'etait ecrit, ce qui rendait la chose invisible —
        // mais « rien n'est ecrit » et « rien n'est lu » ne sont pas la meme
        // promesse, et c'est la seconde que la pause fait.
        let liste = crate::surfaces::ListeBlanche::depuis(["chrome.exe"]);
        // Toutes les autres conditions sont reunies : seule la pause tranche.
        assert!(lecture_autorisee(
            1,
            Some("chrome.exe"),
            &liste,
            1,
            2,
            false
        ));
        assert!(!lecture_autorisee(
            1,
            Some("chrome.exe"),
            &liste,
            1,
            2,
            true
        ));
    }

    #[test]
    fn une_copie_puis_un_collage_s_apparient() {
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();

        pp.ecrire("Reference du dossier 4412");
        a.copie_observee(&pp);

        assert!(matches!(a.coller(&pp), Collage::Apparie { .. }));
    }

    #[test]
    fn un_collage_dont_la_copie_vient_d_ailleurs_n_est_pas_apparie() {
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();

        pp.ecrire("copie interne");
        a.copie_observee(&pp);

        // Une AUTRE application ecrit dans le presse-papiers : un gestionnaire
        // de mots de passe, par exemple.
        pp.ecrire("mot-de-passe-du-coffre");

        assert_eq!(a.coller(&pp), Collage::NonApparie);
    }

    #[test]
    fn un_collage_externe_ne_lit_jamais_le_contenu() {
        // LE test de R2.3. L'implementation naive — hacher a chaque collage puis
        // chercher une correspondance — violerait la regle avant de la
        // verifier : au moment ou l'on hache, on a deja lu.
        let pp = PressePapiersFaux::new();
        let a = Appariement::nouveau();

        pp.ecrire("mot-de-passe-du-coffre");
        let lectures_avant = pp.lectures.get();

        assert_eq!(a.coller(&pp), Collage::NonApparie);
        assert_eq!(
            pp.lectures.get(),
            lectures_avant,
            "R2.3 : le contenu d un presse-papiers externe ne doit JAMAIS etre lu"
        );
    }

    #[test]
    fn meme_un_collage_apparie_ne_relit_pas_le_contenu() {
        // Le condensat est deja en main : relire n'apporterait rien et
        // multiplierait les occasions de fuite.
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();
        pp.ecrire("interne");
        a.copie_observee(&pp);

        let apres_copie = pp.lectures.get();
        let _ = a.coller(&pp);
        assert_eq!(pp.lectures.get(), apres_copie);
    }

    #[test]
    fn deux_copies_du_meme_texte_donnent_la_meme_empreinte() {
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();

        pp.ecrire("Dossier 4412");
        a.copie_observee(&pp);
        let premiere = a.coller(&pp);

        pp.ecrire("autre chose");
        a.copie_observee(&pp);
        pp.ecrire("Dossier 4412");
        a.copie_observee(&pp);
        let seconde = a.coller(&pp);

        assert_eq!(premiere, seconde, "meme contenu, meme empreinte");
    }

    #[test]
    fn deux_textes_differents_ne_s_apparient_pas_entre_eux() {
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();
        pp.ecrire("alpha");
        a.copie_observee(&pp);
        let alpha = a.coller(&pp);

        pp.ecrire("beta");
        a.copie_observee(&pp);
        let beta = a.coller(&pp);

        assert_ne!(alpha, beta);
    }

    #[test]
    fn une_copie_vide_n_est_pas_retenue() {
        // Apparier sur du vide apparierait tout avec tout.
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();
        pp.ecrire("");
        a.copie_observee(&pp);
        assert_eq!(a.copies_connues(), 0);
        assert_eq!(a.coller(&pp), Collage::NonApparie);
    }

    #[test]
    fn le_nombre_de_copies_retenues_est_plafonne() {
        let pp = PressePapiersFaux::new();
        let mut a = Appariement::nouveau();
        for _ in 0..COPIES_MAX + 50 {
            pp.ecrire("quelque chose");
            a.copie_observee(&pp);
        }
        assert!(a.copies_connues() <= COPIES_MAX, "{}", a.copies_connues());
    }

    #[test]
    fn l_empreinte_ne_laisse_pas_deviner_le_contenu() {
        // Elle est courte et prefixee : elle ne ressemble a rien qu'un lecteur
        // pourrait prendre pour de la donnee.
        let e = empreinte("jean.dupont@exemple.fr");
        assert!(e.starts_with("cp_"));
        assert!(!e.contains("jean"));
        assert!(!e.contains("exemple"));
        assert_eq!(e.len(), 19);
    }

    // -- R2.3 : les trois conditions de la lecture -------------------------

    fn autorisees(surfaces: &[&str]) -> crate::surfaces::ListeBlanche {
        crate::surfaces::ListeBlanche::depuis(surfaces.iter().copied())
    }

    #[test]
    fn sans_copie_observee_on_ne_lit_pas() {
        // C'est ce qui distingue Noe d'un gestionnaire de presse-papiers : sans
        // frappe de l'operateur, le presse-papiers n'est jamais ouvert.
        assert!(!lecture_autorisee(
            0,
            Some("chrome.exe"),
            &autorisees(&["chrome.exe"]),
            1,
            2,
            false
        ));
    }

    #[test]
    fn un_ctrl_c_hors_surface_activee_ne_declenche_aucune_lecture() {
        // Le cas que R2.3 nomme : le gestionnaire de mots de passe. Le hook est
        // pose sur tout le bureau ; il dit qu'une combinaison a ete pressee
        // quelque part, pas qu'elle l'a ete la ou on a le droit de regarder.
        let liste = autorisees(&["chrome.exe"]);
        assert!(!lecture_autorisee(
            1,
            Some("keepass.exe"),
            &liste,
            1,
            2,
            false
        ));
        assert!(!lecture_autorisee(
            1,
            Some("bitwarden.exe"),
            &liste,
            1,
            2,
            false
        ));
    }

    #[test]
    fn une_surface_non_nommee_ne_declenche_aucune_lecture() {
        // Un processus protege ou eleve ne se laisse pas nommer. On n'autorise
        // pas ce qu'on n'a pas su identifier.
        assert!(!lecture_autorisee(
            1,
            None,
            &autorisees(&["chrome.exe"]),
            1,
            2,
            false
        ));
    }

    #[test]
    fn une_liste_vide_ne_declenche_jamais_de_lecture() {
        // Au premier lancement, le presse-papiers n'est jamais ouvert.
        for surface in [Some("chrome.exe"), Some("outlook.exe"), None] {
            assert!(!lecture_autorisee(
                3,
                surface,
                &crate::surfaces::ListeBlanche::vide(),
                1,
                2,
                false
            ));
        }
    }

    #[test]
    fn un_ctrl_c_qui_n_a_rien_copie_ne_declenche_aucune_lecture() {
        // Une console ou Ctrl+C vaut interruption, une selection vide. Sans
        // cette condition, on s'approprie ce qui trainait — le mot de passe
        // copie trente secondes plus tot, ailleurs, avant l'episode.
        let liste = autorisees(&["chrome.exe"]);
        assert!(!lecture_autorisee(
            1,
            Some("chrome.exe"),
            &liste,
            7,
            7,
            false
        ));
    }

    #[test]
    fn une_copie_reelle_sur_surface_activee_est_lue() {
        // Le seul cas qui passe, et il faut qu'il passe : sinon l'appariement
        // copier-coller ne mesure rien.
        let liste = autorisees(&["chrome.exe"]);
        assert!(lecture_autorisee(
            1,
            Some("chrome.exe"),
            &liste,
            7,
            8,
            false
        ));
        assert!(lecture_autorisee(
            1,
            Some("CHROME.EXE"),
            &liste,
            7,
            8,
            false
        ));
    }
}
