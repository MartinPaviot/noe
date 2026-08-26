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

//! ## En attente de D27
//!
//! `allow(dead_code)` borné : la logique d'appariement est livrée et testée, mais
//! son consommateur de production dépend d'un arbitrage — comment le système
//! nous dit qu'un collage a eu lieu (`docs/decisions.md`, D27). **À retirer dès
//! que D27 est tranché**, quelle que soit la réponse.
#![allow(dead_code)]

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
#[cfg(not(test))]
pub struct PressePapiersWindows;

#[cfg(not(test))]
impl PressePapiers for PressePapiersWindows {
    fn sequence(&self) -> u64 {
        // SAFETY : lecture d'un compteur global, sans paramètre.
        u64::from(unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() })
    }

    fn texte(&self) -> Option<String> {
        // Volontairement non implémenté pour l'instant.
        //
        // Le seul appelant légitime est `copie_observee`, et il n'est pas encore
        // branché : tant qu'il ne l'est pas, ce programme n'a AUCUN chemin de
        // code qui lise le presse-papiers. Rendre `None` est plus sûr que de
        // préparer une lecture dont personne n'a besoin.
        None
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
}
