//! La clé HMAC de pseudonymisation, protégée par DPAPI (spec 002, R4.1, R4.4).
//!
//! Une clé de 256 bits est tirée à l'installation et ne change plus : c'est elle
//! qui garantit R4.2 — « le MÊME input DOIT produire LE MÊME token pour toute la
//! vie de l'installation ». Si la clé changeait, les jetons changeraient, les
//! jointures du graphe d'entités casseraient, et le corpus deviendrait un tas
//! d'épisodes sans relations.
//!
//! R4.4 dit que la clé ne doit **jamais** apparaître en clair — ni dans un
//! fichier, ni dans un log, ni dans une sortie. Trois dispositions le rendent
//! difficile à violer par accident :
//!
//! 1. le fichier ne contient que le blob DPAPI, lié au compte Windows ;
//! 2. `Debug` est implémenté à la main et n'imprime aucun octet ;
//! 3. le type n'est ni `Serialize`, ni `Display`, ni `Clone` — il n'y a
//!    volontairement aucun chemin commode pour la faire sortir.

use std::path::Path;

use ring::rand::SecureRandom;
use windows::Win32::Foundation::LocalFree;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

/// Entropie applicative mêlée au chiffrement DPAPI.
///
/// Sans elle, n'importe quel programme tournant sous le même compte Windows
/// pourrait déchiffrer le blob en appelant simplement `CryptUnprotectData`. Avec
/// elle, il lui faut aussi connaître cette chaîne — ce n'est pas un secret, mais
/// c'est une barrière de plus contre le déchiffrement opportuniste.
const ENTROPIE: &[u8] = b"noe.capture.hmac.v1";

pub const TAILLE_CLE: usize = 32;

pub struct CleHmac([u8; TAILLE_CLE]);

/// R4.4 : `Debug` ne doit jamais faire fuir la clé.
///
/// Le dérivé automatique aurait imprimé les 32 octets au premier `{:?}` dans un
/// message d'erreur — et un message d'erreur finit dans un log.
impl std::fmt::Debug for CleHmac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CleHmac(<{TAILLE_CLE} octets, jamais affiches>)")
    }
}

#[derive(Debug)]
pub enum ErreurCle {
    Alea,
    Dpapi(String),
    Disque(String),
    Taille(usize),
}

impl std::fmt::Display for ErreurCle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alea => write!(f, "le generateur d alea du systeme a refuse"),
            Self::Dpapi(m) => write!(f, "DPAPI : {m}"),
            Self::Disque(m) => write!(f, "disque : {m}"),
            Self::Taille(n) => write!(f, "cle de taille inattendue : {n} octets"),
        }
    }
}

impl std::error::Error for ErreurCle {}

fn blob(donnees: &mut [u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: donnees.len() as u32,
        pbData: donnees.as_mut_ptr(),
    }
}

/// Copie la sortie DPAPI puis libère le tampon que Windows a alloué.
///
/// `CryptProtectData` alloue avec `LocalAlloc` ; ne pas libérer fuirait de la
/// mémoire à chaque appel. Le tampon contient de la matière sensible dans le
/// sens du déchiffrement, d'où la remise à zéro avant libération.
///
/// # Safety
/// `sortie` doit être un blob renseigné par un appel DPAPI réussi.
unsafe fn recuperer(sortie: &mut CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let vue = std::slice::from_raw_parts(sortie.pbData, sortie.cbData as usize);
    let copie = vue.to_vec();
    std::ptr::write_bytes(sortie.pbData, 0, sortie.cbData as usize);
    let _ = LocalFree(windows::Win32::Foundation::HLOCAL(
        sortie.pbData as *mut core::ffi::c_void,
    ));
    copie
}

impl CleHmac {
    /// Tire une clé neuve avec le générateur du système.
    pub fn generer() -> Result<Self, ErreurCle> {
        let mut octets = [0u8; TAILLE_CLE];
        ring::rand::SystemRandom::new()
            .fill(&mut octets)
            .map_err(|_| ErreurCle::Alea)?;
        Ok(Self(octets))
    }

    pub fn octets(&self) -> &[u8] {
        &self.0
    }

    /// Chiffre la clé pour le compte Windows courant.
    fn proteger(&self) -> Result<Vec<u8>, ErreurCle> {
        let mut clair = self.0;
        let mut entropie = ENTROPIE.to_vec();
        let entree = blob(&mut clair);
        let ent = blob(&mut entropie);
        let mut sortie = CRYPT_INTEGER_BLOB::default();

        // SAFETY : les trois blobs pointent sur des tampons vivants pendant tout
        // l appel, et `sortie` est recopie puis libere immediatement apres.
        unsafe {
            CryptProtectData(&entree, None, Some(&ent), None, None, 0, &mut sortie)
                .map_err(|e| ErreurCle::Dpapi(e.message()))?;
            Ok(recuperer(&mut sortie))
        }
    }

    fn deproteger(chiffre: &[u8]) -> Result<Self, ErreurCle> {
        let mut copie = chiffre.to_vec();
        let mut entropie = ENTROPIE.to_vec();
        let entree = blob(&mut copie);
        let ent = blob(&mut entropie);
        let mut sortie = CRYPT_INTEGER_BLOB::default();

        // SAFETY : idem `proteger`.
        let clair = unsafe {
            CryptUnprotectData(&entree, None, Some(&ent), None, None, 0, &mut sortie)
                .map_err(|e| ErreurCle::Dpapi(e.message()))?;
            recuperer(&mut sortie)
        };

        if clair.len() != TAILLE_CLE {
            return Err(ErreurCle::Taille(clair.len()));
        }
        let mut octets = [0u8; TAILLE_CLE];
        octets.copy_from_slice(&clair);
        Ok(Self(octets))
    }

    /// R6.2 — installe une clé venue d'ailleurs, sous DPAPI.
    ///
    /// C'est le geste qui rend un import utile. Sans lui, un corpus restauré se
    /// lirait et se rejouerait, puis la première capture suivante produirait
    /// d'autres jetons pour les mêmes entités : les jointures casseraient en
    /// silence, à retardement, sans que rien ne dise pourquoi.
    ///
    /// **Écrase sans demander.** L'opérateur qui importe un corpus a déjà pris
    /// la décision ; refuser ici le laisserait avec un corpus et une clé qui ne
    /// vont pas ensemble, ce qui est le pire des trois états possibles.
    pub fn installer(chemin: &Path, octets: &[u8]) -> Result<Self, ErreurCle> {
        if octets.len() != TAILLE_CLE {
            return Err(ErreurCle::Taille(octets.len()));
        }
        let mut brut = [0u8; TAILLE_CLE];
        brut.copy_from_slice(octets);
        let cle = Self(brut);
        let chiffre = cle.proteger()?;
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ErreurCle::Disque(e.to_string()))?;
        }
        std::fs::write(chemin, &chiffre).map_err(|e| ErreurCle::Disque(e.to_string()))?;
        Ok(cle)
    }

    /// Charge la clé du poste, ou en crée une au premier lancement.
    ///
    /// **Ne régénère JAMAIS en silence.** Un blob illisible — profil Windows
    /// changé, fichier corrompu — fait échouer franchement : régénérer
    /// produirait de nouveaux jetons pour les mêmes entités, et le corpus
    /// existant deviendrait muet sans que rien ne le signale. C'est un cas où
    /// s'arrêter vaut mieux que continuer.
    pub fn charger_ou_creer(chemin: &Path) -> Result<Self, ErreurCle> {
        if chemin.exists() {
            let chiffre = std::fs::read(chemin).map_err(|e| ErreurCle::Disque(e.to_string()))?;
            return Self::deproteger(&chiffre);
        }

        let cle = Self::generer()?;
        let chiffre = cle.proteger()?;
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ErreurCle::Disque(e.to_string()))?;
        }
        std::fs::write(chemin, &chiffre).map_err(|e| ErreurCle::Disque(e.to_string()))?;
        Ok(cle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporaire(nom: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("noe-cle-{nom}-{}.bin", std::process::id()));
        p
    }

    #[test]
    fn la_cle_fait_bien_256_bits() {
        assert_eq!(CleHmac::generer().unwrap().octets().len(), 32);
    }

    #[test]
    fn deux_cles_tirees_ne_sont_pas_egales() {
        let a = CleHmac::generer().unwrap();
        let b = CleHmac::generer().unwrap();
        assert_ne!(a.octets(), b.octets(), "l alea du systeme est suspect");
    }

    #[test]
    fn debug_n_imprime_aucun_octet_de_la_cle() {
        // R4.4 : c'est le chemin de fuite le plus banal — une cle dans un
        // message d erreur, un message d erreur dans un log.
        let cle = CleHmac::generer().unwrap();
        let rendu = format!("{cle:?}");
        for octet in cle.octets() {
            assert!(
                !rendu.contains(&format!("{octet}"))
                    || rendu == "CleHmac(<32 octets, jamais affiches>)",
                "le Debug laisse filtrer de la matiere"
            );
        }
        assert_eq!(rendu, "CleHmac(<32 octets, jamais affiches>)");
    }

    #[test]
    fn le_fichier_sur_disque_ne_contient_pas_la_cle_en_clair() {
        let p = temporaire("clair");
        let _ = std::fs::remove_file(&p);
        let cle = CleHmac::charger_ou_creer(&p).unwrap();
        let sur_disque = std::fs::read(&p).unwrap();

        // R4.4, mecaniquement : la suite d octets de la cle ne doit apparaitre
        // nulle part dans le fichier.
        let attendu = cle.octets();
        let present = sur_disque
            .windows(attendu.len())
            .any(|fenetre| fenetre == attendu);
        assert!(!present, "la cle est lisible dans le fichier DPAPI");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn la_cle_survit_a_un_rechargement() {
        let p = temporaire("aller-retour");
        let _ = std::fs::remove_file(&p);

        let a = CleHmac::charger_ou_creer(&p).unwrap();
        let b = CleHmac::charger_ou_creer(&p).unwrap();
        assert_eq!(
            a.octets(),
            b.octets(),
            "R4.2 : la cle DOIT etre stable, sinon les jetons changent"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn un_blob_corrompu_echoue_au_lieu_de_regenerer() {
        let p = temporaire("corrompu");
        std::fs::write(&p, b"ceci n est pas un blob DPAPI").unwrap();

        let r = CleHmac::charger_ou_creer(&p);
        assert!(
            r.is_err(),
            "regenerer en silence donnerait de nouveaux jetons pour les memes \
             entites, et rendrait le corpus existant muet"
        );
        let _ = std::fs::remove_file(&p);
    }
}
