//! L'export et l'import chiffrés (spec 002, R6.1 et R6.2).
//!
//! **Ce que la perte d'un poste ne doit pas détruire.** Un corpus vit sur une
//! seule machine, sous une seule clé HMAC protégée par DPAPI — c'est-à-dire liée
//! au compte Windows courant. Sans export, un disque mort emporte tout ; et un
//! export qui oublierait la clé serait pire qu'inutile, parce qu'il rendrait un
//! corpus lisible mais **incapable de s'agrandir** : les captures suivantes du
//! même identifiant produiraient d'autres jetons, et toutes les jointures
//! casseraient au premier changement de machine.
//!
//! ## La forme de l'archive
//!
//! ```text
//! [4 octets] longueur du manifeste, gros-boutiste
//! [n octets] manifeste JSON, EN CLAIR
//! [le reste] corpus scellé (AES-256-GCM)
//! ```
//!
//! Le manifeste est en clair pour que `--verify` puisse dire de quoi il s'agit
//! avant qu'on donne le mot de passe, et pour qu'une archive dont le mot de
//! passe est perdu reste identifiable. Il ne porte donc **aucun contenu
//! utilisateur** : des versions, des compteurs, des sels, et la clé HMAC
//! enveloppée — qui est un secret, mais un secret déjà chiffré par le mot de
//! passe.
//!
//! Les identifiants d'épisode n'y figurent pas non plus. Ce sont des ULID :
//! leurs dix premiers caractères sont un horodatage, et les lister en clair
//! reviendrait à publier les heures de travail de l'opérateur.

use std::path::Path;

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::SecureRandom;

use crate::cle::{CleHmac, TAILLE_CLE};

pub const FORMAT: &str = "noe-export";
pub const VERSION_FORMAT: u32 = 1;

/// Itérations de PBKDF2-HMAC-SHA256.
///
/// Six cent mille, la recommandation OWASP pour cet algorithme. Le chiffre est
/// **écrit dans le manifeste** plutôt que codé en dur au déchiffrement : une
/// archive produite aujourd'hui doit rester lisible quand la recommandation
/// aura monté, et les bancs doivent pouvoir descendre sans que la production
/// descende avec eux.
pub const ITERATIONS: u32 = 600_000;

/// Un mot de passe court ne protège rien, et l'export est le seul endroit où le
/// corpus quitte la machine.
pub const LONGUEUR_MOT_DE_PASSE_MIN: usize = 12;

const SEL_OCTETS: usize = 16;

#[derive(Debug)]
pub enum ErreurArchive {
    MotDePasseTropCourt(usize),
    Disque(String),
    FormatInconnu(String),
    VersionInconnue(u32),
    MotDePasseRefuse,
    Corrompue(String),
    Alea,
}

impl std::fmt::Display for ErreurArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MotDePasseTropCourt(n) => write!(
                f,
                "mot de passe de {n} caracteres : il en faut au moins {LONGUEUR_MOT_DE_PASSE_MIN}"
            ),
            Self::Disque(e) => write!(f, "disque : {e}"),
            Self::FormatInconnu(s) => write!(f, "ce fichier n est pas une archive Noe ({s})"),
            Self::VersionInconnue(v) => write!(f, "archive de version {v}, non lisible ici"),
            // Le message ne distingue PAS « mauvais mot de passe » de « archive
            // alteree » : l'authentification AEAD echoue de la meme facon, et
            // pretendre les distinguer serait une devinette.
            Self::MotDePasseRefuse => {
                write!(f, "mot de passe refuse, ou archive alteree")
            }
            Self::Corrompue(e) => write!(f, "archive illisible : {e}"),
            Self::Alea => write!(f, "generateur aleatoire indisponible"),
        }
    }
}

impl std::error::Error for ErreurArchive {}

/// Ce que l'archive dit d'elle-même, sans mot de passe.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Manifeste {
    pub format: String,
    pub version_format: u32,
    /// La version du schéma d'épisode au moment de l'export.
    pub version_schema: u32,
    /// La version de la bibliothèque de motifs : un corpus jugé sous une
    /// version antérieure reste interprétable, encore faut-il savoir laquelle.
    pub version_motifs: u32,
    pub iterations: u32,
    pub sel_hex: String,
    pub nonce_corpus_hex: String,
    pub nonce_cle_hex: String,
    /// La clé HMAC scellée par le mot de passe d'export (R6.2).
    pub cle_enveloppee_hex: String,
    pub episodes: usize,
    pub quarantaine: usize,
    pub fichiers: usize,
    pub octets_clairs: u64,
}

/// Le corpus, une fois déchiffré : chemin relatif → contenu.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct Corpus {
    fichiers: std::collections::BTreeMap<String, String>,
}

fn hex(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

fn dehex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Dérive la clé de chiffrement depuis le mot de passe.
fn deriver(mot_de_passe: &str, sel: &[u8], iterations: u32) -> [u8; 32] {
    let mut cle = [0u8; 32];
    let n = std::num::NonZeroU32::new(iterations.max(1)).expect("iterations > 0");
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        n,
        sel,
        mot_de_passe.as_bytes(),
        &mut cle,
    );
    cle
}

fn sceller(
    cle: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    clair: &[u8],
) -> Result<Vec<u8>, ErreurArchive> {
    let unbound = UnboundKey::new(&AES_256_GCM, cle).map_err(|_| ErreurArchive::Alea)?;
    let mut tampon = clair.to_vec();
    LessSafeKey::new(unbound)
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(*nonce),
            Aad::empty(),
            &mut tampon,
        )
        .map_err(|_| ErreurArchive::Alea)?;
    Ok(tampon)
}

fn ouvrir(
    cle: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    scelle: &[u8],
) -> Result<Vec<u8>, ErreurArchive> {
    let unbound = UnboundKey::new(&AES_256_GCM, cle).map_err(|_| ErreurArchive::Alea)?;
    let mut tampon = scelle.to_vec();
    let clair = LessSafeKey::new(unbound)
        .open_in_place(
            Nonce::assume_unique_for_key(*nonce),
            Aad::empty(),
            &mut tampon,
        )
        .map_err(|_| ErreurArchive::MotDePasseRefuse)?;
    Ok(clair.to_vec())
}

fn alea<const N: usize>() -> Result<[u8; N], ErreurArchive> {
    let mut o = [0u8; N];
    ring::rand::SystemRandom::new()
        .fill(&mut o)
        .map_err(|_| ErreurArchive::Alea)?;
    Ok(o)
}

/// Ramasse tout le dossier d'épisodes, chemins relatifs en avant.
///
/// Les séparateurs sont normalisés en `/` : une archive faite sur Windows doit
/// se relire ailleurs, et un `\` dans une clé JSON n'a pas de sens portable.
fn ramasser(racine: &Path) -> std::io::Result<Corpus> {
    let mut corpus = Corpus::default();
    fn descendre(base: &Path, courant: &Path, corpus: &mut Corpus) -> std::io::Result<()> {
        for e in std::fs::read_dir(courant)?.flatten() {
            let chemin = e.path();
            if chemin.is_dir() {
                descendre(base, &chemin, corpus)?;
                continue;
            }
            let relatif = chemin
                .strip_prefix(base)
                .unwrap_or(&chemin)
                .to_string_lossy()
                .replace('\\', "/");
            match std::fs::read_to_string(&chemin) {
                Ok(contenu) => {
                    corpus.fichiers.insert(relatif, contenu);
                }
                // Un fichier binaire ou illisible n'est pas ramassé en silence :
                // l'export dirait sinon qu'il a tout pris alors qu'il manque
                // quelque chose. Aujourd'hui tout est du texte JSON ; le jour où
                // ça change, c'est ici qu'il faudra revenir.
                Err(err) => eprintln!("[noe] export : {relatif} ignore ({err})"),
            }
        }
        Ok(())
    }
    if racine.exists() {
        descendre(racine, racine, &mut corpus)?;
    }
    Ok(corpus)
}

fn compter(corpus: &Corpus) -> (usize, usize) {
    let episodes = corpus
        .fichiers
        .keys()
        .filter(|k| k.ends_with("/episode.json") && !k.starts_with("quarantaine/"))
        .count();
    let quarantaine = corpus
        .fichiers
        .keys()
        .filter(|k| k.starts_with("quarantaine/") && k.ends_with("/raison.txt"))
        .count();
    (episodes, quarantaine)
}

/// R6.1 — produit l'archive.
pub fn exporter(
    racine_episodes: &Path,
    cle_hmac: &CleHmac,
    mot_de_passe: &str,
    destination: &Path,
) -> Result<Manifeste, ErreurArchive> {
    exporter_avec(
        racine_episodes,
        cle_hmac,
        mot_de_passe,
        destination,
        ITERATIONS,
    )
}

/// Le même export, à nombre d'itérations choisi.
///
/// Il n'existe que pour les bancs : six cent mille itérations coûtent une
/// seconde par appel en profil de debug, quinze tests y passeraient un quart de
/// minute, et un banc lent finit par ne plus être lancé. Le chiffre de
/// production est gardé par son propre test.
///
/// Le nombre voyage dans le manifeste, donc l'import suit sans rien savoir.
fn exporter_avec(
    racine_episodes: &Path,
    cle_hmac: &CleHmac,
    mot_de_passe: &str,
    destination: &Path,
    iterations: u32,
) -> Result<Manifeste, ErreurArchive> {
    if mot_de_passe.chars().count() < LONGUEUR_MOT_DE_PASSE_MIN {
        return Err(ErreurArchive::MotDePasseTropCourt(
            mot_de_passe.chars().count(),
        ));
    }

    let corpus = ramasser(racine_episodes).map_err(|e| ErreurArchive::Disque(e.to_string()))?;
    let clair = serde_json::to_vec(&corpus).map_err(|e| ErreurArchive::Corrompue(e.to_string()))?;
    let (episodes, quarantaine) = compter(&corpus);

    let sel: [u8; SEL_OCTETS] = alea()?;
    let nonce_corpus: [u8; NONCE_LEN] = alea()?;
    // Un nonce PAR message, jamais le même deux fois avec la même clé : c'est la
    // seule règle que GCM ne pardonne pas. Le corpus et la clé HMAC sont deux
    // messages sous la même clé dérivée, donc deux nonces.
    let nonce_cle: [u8; NONCE_LEN] = alea()?;

    let derivee = deriver(mot_de_passe, &sel, iterations);
    let corpus_scelle = sceller(&derivee, &nonce_corpus, &clair)?;
    let cle_enveloppee = sceller(&derivee, &nonce_cle, cle_hmac.octets())?;

    let manifeste = Manifeste {
        format: FORMAT.to_string(),
        version_format: VERSION_FORMAT,
        version_schema: crate::assemblage::SCHEMA_V,
        version_motifs: crate::motifs::version(),
        iterations,
        sel_hex: hex(&sel),
        nonce_corpus_hex: hex(&nonce_corpus),
        nonce_cle_hex: hex(&nonce_cle),
        cle_enveloppee_hex: hex(&cle_enveloppee),
        episodes,
        quarantaine,
        fichiers: corpus.fichiers.len(),
        octets_clairs: clair.len() as u64,
    };

    ecrire(destination, &manifeste, &corpus_scelle)?;
    Ok(manifeste)
}

fn ecrire(
    destination: &Path,
    manifeste: &Manifeste,
    corpus_scelle: &[u8],
) -> Result<(), ErreurArchive> {
    let entete =
        serde_json::to_vec(manifeste).map_err(|e| ErreurArchive::Corrompue(e.to_string()))?;
    let mut fichier = Vec::with_capacity(4 + entete.len() + corpus_scelle.len());
    fichier.extend_from_slice(
        &u32::try_from(entete.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    fichier.extend_from_slice(&entete);
    fichier.extend_from_slice(corpus_scelle);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ErreurArchive::Disque(e.to_string()))?;
    }
    std::fs::write(destination, &fichier).map_err(|e| ErreurArchive::Disque(e.to_string()))
}

/// Lit le manifeste **sans** mot de passe.
///
/// C'est ce qui permet à `--verify` de dire de quoi il s'agit avant qu'on donne
/// le secret, et à une archive dont le mot de passe est perdu de rester
/// identifiable au lieu d'être un tas d'octets.
pub fn lire_manifeste(source: &Path) -> Result<(Manifeste, Vec<u8>), ErreurArchive> {
    let brut = std::fs::read(source).map_err(|e| ErreurArchive::Disque(e.to_string()))?;
    if brut.len() < 4 {
        return Err(ErreurArchive::FormatInconnu("fichier trop court".into()));
    }
    let taille = u32::from_be_bytes([brut[0], brut[1], brut[2], brut[3]]) as usize;
    let fin = 4usize
        .checked_add(taille)
        .filter(|f| *f <= brut.len())
        .ok_or_else(|| ErreurArchive::FormatInconnu("entete incoherente".into()))?;

    let manifeste: Manifeste = serde_json::from_slice(&brut[4..fin])
        .map_err(|e| ErreurArchive::FormatInconnu(e.to_string()))?;
    if manifeste.format != FORMAT {
        return Err(ErreurArchive::FormatInconnu(manifeste.format));
    }
    if manifeste.version_format != VERSION_FORMAT {
        return Err(ErreurArchive::VersionInconnue(manifeste.version_format));
    }
    Ok((manifeste, brut[fin..].to_vec()))
}

/// Ce qu'un `--verify` a constaté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub manifeste: Manifeste,
    pub episodes_relus: usize,
    /// Les épisodes que le schéma refuse. Nommés, pas comptés : une archive
    /// qu'on restaure doit dire lesquels sont douteux.
    pub illisibles: Vec<String>,
    /// Les compteurs du manifeste correspondent-ils au contenu réel ?
    pub compteurs_coherents: bool,
}

impl Verdict {
    pub fn valide(&self) -> bool {
        self.illisibles.is_empty() && self.compteurs_coherents
    }
}

fn ouvrir_corpus(
    manifeste: &Manifeste,
    scelle: &[u8],
    mot_de_passe: &str,
) -> Result<Corpus, ErreurArchive> {
    let sel = dehex(&manifeste.sel_hex)
        .ok_or_else(|| ErreurArchive::Corrompue("sel illisible".into()))?;
    let nonce: [u8; NONCE_LEN] = dehex(&manifeste.nonce_corpus_hex)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| ErreurArchive::Corrompue("nonce illisible".into()))?;
    let derivee = deriver(mot_de_passe, &sel, manifeste.iterations);
    let clair = ouvrir(&derivee, &nonce, scelle)?;
    serde_json::from_slice(&clair).map_err(|e| ErreurArchive::Corrompue(e.to_string()))
}

/// R6.1 — `noe import --verify` : on relit tout, on n'installe rien.
pub fn verifier(source: &Path, mot_de_passe: &str) -> Result<Verdict, ErreurArchive> {
    let (manifeste, scelle) = lire_manifeste(source)?;
    let corpus = ouvrir_corpus(&manifeste, &scelle, mot_de_passe)?;

    let mut illisibles = Vec::new();
    let mut episodes_relus = 0usize;
    for (chemin, contenu) in &corpus.fichiers {
        if !chemin.ends_with("/episode.json") {
            continue;
        }
        // Le schéma, pas seulement le JSON : un épisode syntaxiquement valide
        // mais structurellement faux ne se rejouerait pas, et une archive qui
        // le dirait bon mentirait au moment où on en a le plus besoin.
        match serde_json::from_str::<crate::assemblage::Episode>(contenu) {
            Ok(_) => episodes_relus += 1,
            Err(_) => illisibles.push(chemin.clone()),
        }
    }

    let (episodes, quarantaine) = compter(&corpus);
    Ok(Verdict {
        compteurs_coherents: episodes == manifeste.episodes
            && quarantaine == manifeste.quarantaine
            && corpus.fichiers.len() == manifeste.fichiers,
        manifeste,
        episodes_relus,
        illisibles,
    })
}

/// R6.2 — restaure le corpus ET la clé HMAC sur la machine cible.
///
/// **La clé d'abord, et si elle échoue on n'écrit rien.** Un corpus restauré
/// sans sa clé est un piège : il se lit, il se rejoue, et la première capture
/// suivante produit des jetons différents pour les mêmes entités. Les jointures
/// cassent en silence, à retardement, et rien ne dit pourquoi.
pub fn importer(
    source: &Path,
    mot_de_passe: &str,
    racine_episodes: &Path,
    chemin_cle: &Path,
) -> Result<Verdict, ErreurArchive> {
    let verdict = verifier(source, mot_de_passe)?;
    let (manifeste, scelle) = lire_manifeste(source)?;
    let corpus = ouvrir_corpus(&manifeste, &scelle, mot_de_passe)?;

    // 1. La clé.
    let sel = dehex(&manifeste.sel_hex)
        .ok_or_else(|| ErreurArchive::Corrompue("sel illisible".into()))?;
    let nonce_cle: [u8; NONCE_LEN] = dehex(&manifeste.nonce_cle_hex)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| ErreurArchive::Corrompue("nonce de cle illisible".into()))?;
    let enveloppee = dehex(&manifeste.cle_enveloppee_hex)
        .ok_or_else(|| ErreurArchive::Corrompue("cle enveloppee illisible".into()))?;
    let derivee = deriver(mot_de_passe, &sel, manifeste.iterations);
    let octets = ouvrir(&derivee, &nonce_cle, &enveloppee)?;
    if octets.len() != TAILLE_CLE {
        return Err(ErreurArchive::Corrompue(format!(
            "cle de {} octets au lieu de {TAILLE_CLE}",
            octets.len()
        )));
    }
    CleHmac::installer(chemin_cle, &octets).map_err(|e| ErreurArchive::Disque(e.to_string()))?;

    // 2. Le corpus.
    for (relatif, contenu) in &corpus.fichiers {
        // Une archive vient peut-être d'ailleurs ; on ne lui laisse pas écrire
        // où elle veut. Le contrôle **nomme ce qu'il accepte** — des composants
        // ordinaires, et rien d'autre.
        //
        // La première écriture listait ce qu'elle refusait : `..` et un chemin
        // commençant par `/`. Sur Windows, qui est la seule plateforme visée,
        // ça laissait passer **quatre** évasions : `C:illeurs`, `\serveur        // partage`, `illeurs` et `C:relatif`. Aucune ne contient `..` ni ne
        // commence par `/`, et `Path::join` avec un chemin absolu **remplace**
        // la base au lieu de s'y ajouter.
        if !chemin_confine(relatif) {
            return Err(ErreurArchive::Corrompue(format!(
                "chemin refuse dans l archive : {relatif}"
            )));
        }
        let cible = racine_episodes.join(relatif);
        if let Some(parent) = cible.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ErreurArchive::Disque(e.to_string()))?;
        }
        std::fs::write(&cible, contenu).map_err(|e| ErreurArchive::Disque(e.to_string()))?;
    }
    Ok(verdict)
}

/// Un chemin d'archive reste-t-il sous la racine ?
///
/// **Une liste blanche de composants**, et pas une liste noire de formes. Un
/// composant `Normal` est un simple nom de fichier ou de dossier ; tout le reste
/// — remontée, racine, préfixe de lecteur, point courant — sort du dossier ou
/// laisse `join` faire quelque chose d'inattendu.
///
/// C'est la même doctrine qu'ailleurs dans ce dépôt : on ne peut pas énumérer
/// toutes les façons de sortir d'un dossier, on peut énumérer les façons d'y
/// rester.
fn chemin_confine(relatif: &str) -> bool {
    use std::path::Component;
    if relatif.is_empty() {
        return false;
    }
    // `Path` interprète `/` et `\` sur Windows ; sur un autre système, `\` est
    // un caractère de nom ordinaire. On refuse donc l'antislash explicitement,
    // pour que le verdict ne dépende pas de la plateforme qui l'évalue.
    if relatif.contains('\\') {
        return false;
    }
    std::path::Path::new(relatif)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un banc à faible coût de dérivation.
    ///
    /// Six cent mille itérations prennent une seconde par appel en profil de
    /// debug : douze tests y passeraient une demi-minute, et un banc lent finit
    /// par ne plus être lancé. Le chiffre de production est vérifié à part.
    const ITERATIONS_BANC: u32 = 32;

    const MOT_DE_PASSE: &str = "un-mot-de-passe-assez-long";

    fn racine(nom: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("noe-archive-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Un export de banc : le MÊME chemin, à itérations basses.
    ///
    /// Le même code exactement, pas une réimplémentation : un banc qui
    /// reconstruirait l'archive à la main testerait le banc et pas l'export.
    fn exporter_banc(
        source: &Path,
        cle: &CleHmac,
        mot_de_passe: &str,
        destination: &Path,
    ) -> Manifeste {
        exporter_avec(source, cle, mot_de_passe, destination, ITERATIONS_BANC).expect("export")
    }

    fn corpus_de_banc(r: &Path) {
        let ep = r.join("01JQA1B2C3D4E5F6G7H8J9K0M1");
        std::fs::create_dir_all(&ep).unwrap();
        std::fs::write(
            ep.join("episode.json"),
            serde_json::to_string(&episode_valide()).unwrap(),
        )
        .unwrap();
        std::fs::write(ep.join("journal.jsonl"), "{\"kind\":\"cloture_auto\"}\n").unwrap();
        let q = r.join("quarantaine").join("01JQA1B2C3D4E5F6G7H8J9K0M2");
        std::fs::create_dir_all(&q).unwrap();
        std::fs::write(q.join("raison.txt"), "aucune action").unwrap();
    }

    fn episode_valide() -> crate::assemblage::Episode {
        crate::assemblage::Episode {
            schema_v: crate::assemblage::SCHEMA_V,
            id: "01JQA1B2C3D4E5F6G7H8J9K0M1".into(),
            task_slug: "maj-crm-post-echange".into(),
            t0: "2026-01-14T09:12:00.000Z".into(),
            t1: "2026-01-14T09:16:42.000Z".into(),
            events: Vec::new(),
            entities: Vec::new(),
            grade: "C".into(),
            grade_reason: "banc".into(),
            scope_fields: Vec::new(),
            completeness: crate::assemblage::Completude {
                explained: 0,
                out_of_scope: 0,
                gaps: 0,
            },
            supersedes: None,
        }
    }

    #[test]
    fn l_hexadecimal_fait_l_aller_retour() {
        for v in [
            vec![],
            vec![0u8],
            vec![255, 0, 16, 128],
            (0..=255u8).collect(),
        ] {
            assert_eq!(dehex(&hex(&v)), Some(v));
        }
        assert_eq!(dehex("abc"), None, "longueur impaire");
        assert_eq!(dehex("zz"), None, "hors alphabet");
    }

    #[test]
    fn le_manifeste_ne_porte_aucun_contenu_utilisateur() {
        // R6.1 : le manifeste est en clair pour que `--verify` puisse parler
        // avant qu'on donne le secret. Il ne doit donc rien porter qu'on ne
        // voudrait pas voir en clair — y compris les identifiants d'episode, qui
        // sont des ULID et publient donc les heures de travail.
        let r = racine("manifeste-clair");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        let m = exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);

        let brut = std::fs::read(&dest).unwrap();
        let taille = u32::from_be_bytes([brut[0], brut[1], brut[2], brut[3]]) as usize;
        let entete = String::from_utf8_lossy(&brut[4..4 + taille]).to_string();
        for interdit in [
            "01JQA1B2C3D4E5F6G7H8J9K0M1",
            "maj-crm-post-echange",
            "2026-01-14",
            "aucune action",
        ] {
            assert!(
                !entete.contains(interdit),
                "« {interdit} » en clair :\n{entete}"
            );
        }
        assert_eq!(m.episodes, 1);
        assert_eq!(m.quarantaine, 1);
    }

    #[test]
    fn le_corpus_n_apparait_jamais_en_clair_dans_le_fichier() {
        let r = racine("corpus-chiffre");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);

        let brut = std::fs::read(&dest).unwrap();
        let texte = String::from_utf8_lossy(&brut);
        for interdit in ["maj-crm-post-echange", "cloture_auto", "aucune action"] {
            assert!(
                !texte.contains(interdit),
                "« {interdit} » en clair dans l archive"
            );
        }
    }

    #[test]
    fn un_aller_retour_rend_exactement_le_meme_corpus() {
        let r = racine("aller-retour");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        let cle = CleHmac::generer().unwrap();
        exporter_banc(&r, &cle, MOT_DE_PASSE, &dest);

        let cible = racine("aller-retour-cible");
        let chemin_cle = cible.join("cle.bin");
        let v = importer(&dest, MOT_DE_PASSE, &cible, &chemin_cle).expect("import");
        assert!(v.valide(), "{v:?}");
        assert_eq!(v.episodes_relus, 1);

        let attendu =
            std::fs::read_to_string(r.join("01JQA1B2C3D4E5F6G7H8J9K0M1").join("episode.json"))
                .unwrap();
        let obtenu = std::fs::read_to_string(
            cible
                .join("01JQA1B2C3D4E5F6G7H8J9K0M1")
                .join("episode.json"),
        )
        .unwrap();
        assert_eq!(attendu, obtenu);
        assert!(cible
            .join("quarantaine")
            .join("01JQA1B2C3D4E5F6G7H8J9K0M2")
            .join("raison.txt")
            .exists());
    }

    #[test]
    fn la_cle_hmac_traverse_la_machine_et_rend_les_memes_jetons() {
        // R6.2, et c'est TOUT l'enjeu. Un corpus restaure sans sa cle se lit et
        // se rejoue — puis la premiere capture suivante produit d'autres jetons
        // pour les memes entites, et les jointures cassent en silence, a
        // retardement, sans que rien ne dise pourquoi.
        let r = racine("migration");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        let cle = CleHmac::generer().unwrap();
        let avant = crate::redaction::Redacteur::new(&cle).jeton("EMAIL", "jean@exemple.fr");
        exporter_banc(&r, &cle, MOT_DE_PASSE, &dest);

        // Une machine vierge : rien, pas meme un fichier de cle.
        let cible = racine("migration-cible");
        let chemin_cle = cible.join("cle.bin");
        importer(&dest, MOT_DE_PASSE, &cible, &chemin_cle).expect("import");

        // La cle est installee sous DPAPI : on la recharge comme le ferait
        // l'application au demarrage suivant.
        let rechargee = CleHmac::charger_ou_creer(&chemin_cle).expect("cle rechargee");
        let apres = crate::redaction::Redacteur::new(&rechargee).jeton("EMAIL", "jean@exemple.fr");
        assert_eq!(avant, apres, "meme entite, meme jeton, autre machine");
    }

    #[test]
    fn un_mauvais_mot_de_passe_ne_rend_rien() {
        let r = racine("mauvais-mdp");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);

        let e = verifier(&dest, "un-autre-mot-de-passe").unwrap_err();
        assert!(matches!(e, ErreurArchive::MotDePasseRefuse), "{e:?}");
    }

    #[test]
    fn une_archive_alteree_est_refusee() {
        // AES-GCM authentifie : un octet change dans le corps ne donne pas un
        // corpus legerement faux, il ne donne rien du tout. C'est la difference
        // entre chiffrer et proteger.
        let r = racine("alteree");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);

        let mut brut = std::fs::read(&dest).unwrap();
        let dernier = brut.len() - 1;
        brut[dernier] ^= 0xff;
        std::fs::write(&dest, &brut).unwrap();

        assert!(matches!(
            verifier(&dest, MOT_DE_PASSE).unwrap_err(),
            ErreurArchive::MotDePasseRefuse
        ));
    }

    #[test]
    fn un_import_qui_echoue_n_installe_pas_de_cle() {
        // Sinon la machine cible se retrouverait avec la cle d'un corpus qu'elle
        // n'a pas recu — et ecraserait la sienne.
        let r = racine("echec-cle");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);

        let cible = racine("echec-cle-cible");
        let chemin_cle = cible.join("cle.bin");
        assert!(importer(&dest, "mauvais-mot-de-passe", &cible, &chemin_cle).is_err());
        assert!(!chemin_cle.exists(), "aucune cle ne doit avoir ete ecrite");
    }

    #[test]
    fn un_mot_de_passe_court_est_refuse_a_l_export() {
        // L'export est le seul endroit ou le corpus quitte la machine. Un mot de
        // passe de six caracteres rendrait tout le reste decoratif.
        let r = racine("mdp-court");
        corpus_de_banc(&r);
        let e = exporter(&r, &CleHmac::generer().unwrap(), "court", &r.join("x.noe")).unwrap_err();
        assert!(matches!(e, ErreurArchive::MotDePasseTropCourt(5)), "{e:?}");
    }

    #[test]
    fn le_manifeste_se_lit_sans_mot_de_passe() {
        // C'est ce qui permet a une archive dont le secret est perdu de rester
        // identifiable au lieu d'etre un tas d'octets.
        let r = racine("manifeste-sans-mdp");
        corpus_de_banc(&r);
        let dest = r.join("corpus.noe");
        exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);

        let (m, _) = lire_manifeste(&dest).expect("manifeste lisible");
        assert_eq!(m.format, FORMAT);
        assert_eq!(m.version_schema, crate::assemblage::SCHEMA_V);
        assert_eq!(m.version_motifs, crate::motifs::version());
    }

    #[test]
    fn un_fichier_qui_n_est_pas_une_archive_est_refuse_franchement() {
        let r = racine("pas-une-archive");
        let f = r.join("bruit.bin");
        std::fs::write(&f, b"ceci n est pas une archive").unwrap();
        assert!(matches!(
            lire_manifeste(&f).unwrap_err(),
            ErreurArchive::FormatInconnu(_)
        ));
    }

    #[test]
    fn un_episode_illisible_est_nomme_pas_seulement_compte() {
        // Une archive qu'on restaure doit dire LESQUELS sont douteux : c'est le
        // moment ou l'operateur a le moins de moyens d'aller voir lui-meme.
        let r = racine("episode-illisible");
        corpus_de_banc(&r);
        let abime = r.join("01JQA1B2C3D4E5F6G7H8J9K0M9");
        std::fs::create_dir_all(&abime).unwrap();
        std::fs::write(abime.join("episode.json"), "{\"pas\":\"un episode\"}").unwrap();

        let dest = r.join("corpus.noe");
        exporter_banc(&r, &CleHmac::generer().unwrap(), MOT_DE_PASSE, &dest);
        let v = verifier(&dest, MOT_DE_PASSE).unwrap();
        assert!(!v.valide());
        assert_eq!(v.illisibles.len(), 1);
        assert!(v.illisibles[0].contains("01JQA1B2C3D4E5F6G7H8J9K0M9"));
    }

    #[test]
    fn un_chemin_qui_remonte_est_refuse_a_l_import() {
        // Une archive vient peut-etre d'ailleurs. On ne lui laisse pas ecrire ou
        // elle veut.
        let r = racine("chemin-remontant");
        let dest = r.join("mechante.noe");
        let mut corpus = Corpus::default();
        corpus
            .fichiers
            .insert("../../evade.txt".into(), "non".into());
        let clair = serde_json::to_vec(&corpus).unwrap();
        let sel = [7u8; SEL_OCTETS];
        let nonce = [9u8; NONCE_LEN];
        let derivee = deriver(MOT_DE_PASSE, &sel, ITERATIONS_BANC);
        let scelle = sceller(&derivee, &nonce, &clair).unwrap();
        let cle_enveloppee = sceller(&derivee, &[3u8; NONCE_LEN], &[0u8; TAILLE_CLE]).unwrap();
        let m = Manifeste {
            format: FORMAT.into(),
            version_format: VERSION_FORMAT,
            version_schema: crate::assemblage::SCHEMA_V,
            version_motifs: crate::motifs::version(),
            iterations: ITERATIONS_BANC,
            sel_hex: hex(&sel),
            nonce_corpus_hex: hex(&nonce),
            nonce_cle_hex: hex(&[3u8; NONCE_LEN]),
            cle_enveloppee_hex: hex(&cle_enveloppee),
            episodes: 0,
            quarantaine: 0,
            fichiers: 1,
            octets_clairs: clair.len() as u64,
        };
        ecrire(&dest, &m, &scelle).unwrap();

        let cible = racine("chemin-remontant-cible");
        let e = importer(&dest, MOT_DE_PASSE, &cible, &cible.join("cle.bin")).unwrap_err();
        assert!(matches!(e, ErreurArchive::Corrompue(_)), "{e:?}");
        assert!(!r.parent().unwrap().join("evade.txt").exists());
    }

    #[test]
    fn le_chiffre_d_iterations_de_production_tient_la_recommandation() {
        // Le banc tourne a 32 iterations pour ne pas durer une demi-minute. Ce
        // test-la garde le chiffre reel, qui est le seul qui protege quelque
        // chose.
        // `assert_eq!` et non `assert!(>=)` : clippy refuse une comparaison
        // dont il sait deja l'issue, et il a raison de la refuser. Ce test garde
        // un CHIFFRE, pas un calcul — le faire baisser doit rougir, pas passer
        // inapercu. Le monter est une decision, et elle passera par ici.
        assert_eq!(
            ITERATIONS, 600_000,
            "PBKDF2-HMAC-SHA256 : la recommandation OWASP est 600 000"
        );
    }

    #[test]
    fn deux_exports_du_meme_corpus_ne_donnent_pas_le_meme_fichier() {
        // Sel et nonces tires a chaque export : deux archives identiques
        // laisseraient deduire que le corpus n'a pas bouge, ce qui est deja une
        // information.
        let r = racine("deux-exports");
        corpus_de_banc(&r);
        let cle = CleHmac::generer().unwrap();
        let a = r.join("a.noe");
        let b = r.join("b.noe");
        exporter_banc(&r, &cle, MOT_DE_PASSE, &a);
        exporter_banc(&r, &cle, MOT_DE_PASSE, &b);
        assert_ne!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    }
}

#[cfg(test)]
mod tests_confinement {
    use super::chemin_confine;

    #[test]
    fn un_chemin_d_episode_ordinaire_est_accepte() {
        assert!(chemin_confine("01JQA1B2C3D4E5F6G7H8J9K0M1/episode.json"));
        assert!(chemin_confine("episode.json"));
        assert!(chemin_confine("a/b/c/d.json"));
    }

    #[test]
    fn une_remontee_est_refusee() {
        assert!(!chemin_confine("../ailleurs.json"));
        assert!(!chemin_confine("a/../../ailleurs.json"));
        // Celui-ci ne « remonte » qu'apres normalisation : `a/../b` reste sous la
        // racine, mais l'accepter demanderait de normaliser, et normaliser un
        // chemin qu'on n'a pas ecrit soi-meme est le debut des ennuis.
        assert!(!chemin_confine("a/../b.json"));
    }

    #[test]
    fn les_quatre_evasions_windows_sont_refusees() {
        // Aucune ne contient `..`, aucune ne commence par `/`. Le garde d'origine
        // les laissait toutes passer, et `Path::join` avec un chemin absolu
        // REMPLACE la base au lieu de s'y ajouter : l'archive ecrivait ou elle
        // voulait, sur la seule plateforme que ce programme vise.
        assert!(!chemin_confine("C:/ailleurs/x.json"), "lecteur absolu");
        assert!(!chemin_confine("C:ailleurs/x.json"), "relatif au lecteur");
        assert!(!chemin_confine("/ailleurs/x.json"), "racine");
        assert!(!chemin_confine("//serveur/partage/x.json"), "UNC");
    }

    #[test]
    fn l_antislash_est_refuse_quelle_que_soit_la_plateforme() {
        // Windows le lit comme un separateur, un autre systeme comme un
        // caractere de nom. Un verdict qui changerait selon la machine qui
        // l'evalue ne serait pas un verdict.
        let contre_oblique = char::from(92);
        assert!(!chemin_confine(&format!(
            "C:{contre_oblique}ailleurs{contre_oblique}x.json"
        )));
        assert!(!chemin_confine(&format!(
            "{contre_oblique}{contre_oblique}serveur{contre_oblique}partage"
        )));
        assert!(!chemin_confine(&format!("a{contre_oblique}b.json")));
    }

    #[test]
    fn un_chemin_vide_ou_degenere_est_refuse() {
        assert!(!chemin_confine(""));
        assert!(!chemin_confine("."));
        assert!(!chemin_confine("./x.json"));
        assert!(!chemin_confine("/"));
    }
}
