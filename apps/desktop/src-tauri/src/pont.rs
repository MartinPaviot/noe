//! Le pont entre l'extension navigateur et l'application (spec 002, tâche 6b).
//!
//! ## Pourquoi trois morceaux et pas deux
//!
//! Le native messaging de Chrome ne parle pas à un processus déjà lancé : il
//! **démarre** un exécutable, lui parle sur son entrée standard, et le tue quand
//! l'onglet ferme. L'application Noe, elle, tourne en permanence dans la barre
//! d'état. Il faut donc un relais :
//!
//! ```text
//! page → service worker → hôte de native messaging → tuyau nommé → app Noe
//! ```
//!
//! L'hôte est un exécutable minuscule (`noe-pont-dom.exe`) que Chrome lance et
//! qui ne fait qu'une chose : recopier ce qu'il reçoit dans un tuyau nommé local.
//!
//! ## Ce qui ne traverse jamais ce pont
//!
//! Le tuyau est **local et restreint au compte courant** — pas de socket, pas de
//! port TCP, rien qui écoute sur une interface réseau. Un port localhost serait
//! joignable par n'importe quel programme du poste, y compris par une page web
//! via `fetch`. C'est la différence entre « ça ne sort pas de la machine » et
//! « ça ne sort pas du compte ».
//!
//! Et le script de contenu ne lit jamais la valeur d'un champ : ce qui traverse,
//! ce sont des rôles, des noms accessibles et des chemins — la même matière que
//! ce qu'UIA remonte côté natif, et qui passera par le même rédacteur.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::source::{
    Abonnement, CaptureSource, Cible, ErreurSource, GenreEvenement, RawEvent, Source,
};

/// Le nom du tuyau. Un seul, fixe : l'hôte doit pouvoir le trouver sans
/// configuration, et deux instances de Noe sur un même compte n'ont pas de sens.
pub const TUYAU: &str = r"\\.\pipe\noe-dom";

/// Le nom de l'hôte de native messaging, tel que Chrome le cherche.
pub const NOM_HOTE: &str = "app.noe.pont";

/// La surface déclarée pour tout ce qui vient du navigateur.
///
/// Une seule, et c'est voulu : R2.1 partitionne par **classe de surface**, pas
/// par onglet. L'opérateur active « le navigateur » ou ne l'active pas.
pub const SURFACE: &str = "chrome.exe";

/// Ce que le service worker envoie, tel quel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Observation {
    pub genre: String,
    pub cible: CibleDom,
    pub seq: u64,
    /// L'identifiant de vie du service worker.
    ///
    /// MV3 arrête et relance le worker quand il veut, et la numérotation repart
    /// à zéro. Sans cet identifiant, deux moitiés d'épisode se recolleraient en
    /// silence — exactement le trou non déclaré que la règle 4 interdit.
    pub vie: String,
    #[serde(default)]
    pub onglet: Option<i64>,
    #[serde(default)]
    pub cadre: u64,
    #[serde(default)]
    pub etait_vide: Option<bool>,
    #[serde(default)]
    pub est_vide: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CibleDom {
    pub role: String,
    pub nom: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub chemin: String,
    #[serde(default)]
    pub data: std::collections::BTreeMap<String, String>,
}

/// Traduit une observation du navigateur en `RawEvent`.
///
/// Le pipeline aval — rédaction, journal, snapshot, assemblage — **ne bouge pas
/// d'une ligne** : c'est tout l'intérêt d'avoir mis un trait devant les sources.
/// Une observation DOM et une observation UIA se ressemblent à ce point-ci
/// précisément parce qu'elles décrivent la même chose.
pub fn en_evenement(o: &Observation, monotone_ms: u64) -> Option<RawEvent> {
    let mut cible = Cible::new(&o.cible.role, &o.cible.nom);
    if let Some(r) = &o.cible.region {
        cible = cible.dans(r);
    }
    let genre = match o.genre.as_str() {
        "focus" => GenreEvenement::Focus(cible),
        "invocation" => GenreEvenement::Invocation(cible),
        "changement_valeur" => GenreEvenement::ChangementValeur(cible),
        "soumission" => GenreEvenement::Soumission(cible),
        "saisie" => GenreEvenement::Saisie(cible),
        // Un genre inconnu ne devient PAS un événement générique : il vient
        // d'une version d'extension qu'on ne connaît pas, et l'interpréter de
        // travers serait pire que de l'ignorer. L'appelant le compte.
        _ => return None,
    };
    Some(RawEvent {
        source: crate::source::Source::Dom,
        monotone_ms,
        genre,
        surface: Some(SURFACE.to_string()),
    })
}

/// Ce que le serveur a constaté depuis le dernier relevé.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bilan {
    pub recues: u64,
    /// Les observations dont le genre est inconnu de cette version.
    pub genres_inconnus: u64,
    /// Les ruptures de numérotation, y compris les redémarrages de worker.
    pub ruptures: u64,
}

/// Suit la numérotation d'un service worker et voit les trous.
#[derive(Debug, Default)]
pub struct Suiveur {
    vie: Option<String>,
    dernier_seq: u64,
}

impl Suiveur {
    /// Combien d'observations manquent avant celle-ci.
    ///
    /// Un redémarrage du worker rend `0` et non un trou géant : la numérotation
    /// repart à zéro, et compter la différence produirait un trou imaginaire.
    /// Le changement de vie est *lui-même* la rupture, et il est signalé comme
    /// telle.
    pub fn constater(&mut self, o: &Observation) -> Rupture {
        match &self.vie {
            Some(v) if *v == o.vie => {
                let attendu = self.dernier_seq + 1;
                self.dernier_seq = o.seq;
                if o.seq > attendu {
                    Rupture::Manquantes(o.seq - attendu)
                } else {
                    Rupture::Aucune
                }
            }
            Some(_) => {
                self.vie = Some(o.vie.clone());
                self.dernier_seq = o.seq;
                Rupture::WorkerRedemarre
            }
            None => {
                self.vie = Some(o.vie.clone());
                self.dernier_seq = o.seq;
                Rupture::Aucune
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rupture {
    Aucune,
    Manquantes(u64),
    WorkerRedemarre,
}

/// La source navigateur.
///
/// Elle sert le tuyau nommé pendant tout l'épisode et pousse les `RawEvent` dans
/// le même canal que la source native. Le moteur ne sait pas laquelle a parlé —
/// c'est le champ `source` de l'événement qui le dit, pour le diagnostic.
pub struct DomSource {
    actif: Arc<std::sync::atomic::AtomicBool>,
    /// L'horloge du processus, la MEME que partout ailleurs.
    ///
    /// Une source qui daterait depuis sa propre origine ferait exactement ce que
    /// D30 vient de corriger cote UIA : deux echelles dans un meme journal, un
    /// delai d'inactivite qui part au mauvais moment, et des trous ecrases a t1.
    ///
    /// En test il n'y a pas de tuyau, donc pas de boucle qui la lise — d'ou
    /// l'annotation plutot qu'un champ retire, qu'il faudrait remettre.
    #[cfg_attr(test, allow(dead_code))]
    horloge: Arc<dyn crate::horloge::Horloge>,
    bilan: Arc<Mutex<Bilan>>,
}

impl DomSource {
    pub fn new(horloge: Arc<dyn crate::horloge::Horloge>) -> Self {
        Self {
            actif: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            horloge,
            bilan: Arc::new(Mutex::new(Bilan::default())),
        }
    }

    pub fn bilan(&self) -> Bilan {
        self.bilan.lock().expect("bilan empoisonne").clone()
    }
}

/// L'événement qui déclare une perte d'observations.
///
/// La surface est celle du pont : c'est bien du navigateur que le flux vient, et
/// une liste blanche qui ne l'autoriserait pas doit pouvoir refuser ce trou
/// comme elle refuse le reste — un trou attribué à une surface qu'on n'observe
/// pas serait un trou inventé.
fn rupture(monotone_ms: u64, manquantes: u64) -> RawEvent {
    RawEvent {
        source: Source::Dom,
        monotone_ms,
        genre: GenreEvenement::RuptureFlux { manquantes },
        surface: Some(SURFACE.to_string()),
    }
}

/// Consomme une ligne JSON venue du pont et pousse ce qu'il faut.
///
/// Extrait de la boucle réseau pour être testable sans tuyau : c'est ici que
/// vivent toutes les décisions, la boucle ne fait que lire des octets.
pub fn traiter_ligne(
    ligne: &str,
    suiveur: &mut Suiveur,
    bilan: &mut Bilan,
    monotone_ms: u64,
) -> Vec<RawEvent> {
    let Ok(o) = serde_json::from_str::<Observation>(ligne) else {
        // Une ligne illisible n'est pas silencieuse : elle compte comme une
        // rupture, parce qu'on ne sait pas ce qu'elle contenait.
        bilan.ruptures += 1;
        return vec![rupture(monotone_ms, 0)];
    };
    bilan.recues += 1;

    let mut evenements = Vec::new();
    // **Le bilan ne suffit pas.** Il comptait les ruptures et personne ne le
    // lisait : `bilan()` n'avait aucun appelant hors des bancs. Un trou compté et
    // jamais déclaré est un trou rebouché en silence, ce que la règle 4 interdit
    // — et le compteur rendait la chose pire, en donnant l'impression que
    // quelqu'un s'en occupait.
    match suiveur.constater(&o) {
        Rupture::Aucune => {}
        Rupture::Manquantes(n) => {
            bilan.ruptures += n;
            evenements.push(rupture(monotone_ms, n));
        }
        Rupture::WorkerRedemarre => {
            bilan.ruptures += 1;
            evenements.push(rupture(monotone_ms, 0));
        }
    }

    match en_evenement(&o, monotone_ms) {
        Some(e) => evenements.push(e),
        None => bilan.genres_inconnus += 1,
    }
    evenements
}

#[cfg(not(test))]
impl CaptureSource for DomSource {
    fn abonner(&mut self, puits: Sender<RawEvent>) -> Result<Abonnement, ErreurSource> {
        use std::sync::atomic::Ordering;
        if self.actif.load(Ordering::SeqCst) {
            return Err(ErreurSource::DejaAbonne);
        }
        let actif = self.actif.clone();
        let abonnement = Abonnement::nouveau(actif.clone());
        let horloge = self.horloge.clone();
        let bilan = self.bilan.clone();

        std::thread::Builder::new()
            .name("noe-dom".into())
            .spawn(move || servir(&puits, &actif, &horloge, &bilan))
            .map_err(|_| ErreurSource::DejaAbonne)?;

        Ok(abonnement)
    }
}

/// Sert le tuyau nommé tant que l'épisode dure.
///
/// **Une instance en écoute EN PERMANENCE**, et un fil par connexion. C'est le
/// point qui a coûté le plus cher à comprendre : la première version servait une
/// connexion à la fois, et rien n'écoutait pendant qu'elle lisait.
///
/// Le cas se produit tout seul. Chrome relance le service worker quand il veut,
/// et à chaque relance il **redémarre l'hôte**. Le nouvel hôte se présente
/// pendant que l'ancien tient encore la connexion — il ne trouve personne, et la
/// capture navigateur s'arrête pour le reste de l'épisode, en silence. Mesuré
/// sur le banc : rechargement de l'extension, et plus une seule observation.
///
/// Le compteur d'instances est celui de `CreateNamedPipeW` : quatre suffisent
/// largement pour un hôte à la fois plus un qui se présente.
#[cfg(not(test))]
fn servir(
    puits: &Sender<RawEvent>,
    actif: &Arc<std::sync::atomic::AtomicBool>,
    horloge: &Arc<dyn crate::horloge::Horloge>,
    bilan: &Arc<Mutex<Bilan>>,
) {
    use std::sync::atomic::Ordering;

    // Le suiveur est PARTAGÉ entre les connexions : la numérotation appartient
    // au service worker, pas au tuyau. Un suiveur par connexion prendrait chaque
    // reconnexion pour un premier message et raterait les trous.
    let suiveur = Arc::new(Mutex::new(Suiveur::default()));

    while actif.load(Ordering::SeqCst) {
        let tuyau = match creer_tuyau() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[noe] tuyau DOM indisponible : {e}");
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }
        };
        if !attendre_connexion(&tuyau) {
            continue;
        }
        // La lecture part sur son propre fil, et la boucle recrée aussitôt une
        // instance en écoute. C'est tout le correctif.
        let puits = puits.clone();
        let actif = actif.clone();
        let horloge = horloge.clone();
        let bilan = bilan.clone();
        let suiveur = suiveur.clone();
        let _ = std::thread::Builder::new()
            .name("noe-dom-lecture".into())
            .spawn(move || lire(tuyau, &puits, &actif, &horloge, &bilan, &suiveur));
    }
}

/// Lit une connexion jusqu'à sa fin.
#[cfg(not(test))]
fn lire(
    tuyau: std::fs::File,
    puits: &Sender<RawEvent>,
    actif: &Arc<std::sync::atomic::AtomicBool>,
    horloge: &Arc<dyn crate::horloge::Horloge>,
    bilan: &Arc<Mutex<Bilan>>,
    suiveur: &Arc<Mutex<Suiveur>>,
) {
    use std::io::{BufRead, BufReader};
    use std::sync::atomic::Ordering;

    for ligne in BufReader::new(tuyau).lines().map_while(Result::ok) {
        if !actif.load(Ordering::SeqCst) {
            return;
        }
        let evenements = {
            let mut s = suiveur.lock().expect("suiveur empoisonne");
            let mut b = bilan.lock().expect("bilan empoisonne");
            traiter_ligne(&ligne, &mut s, &mut b, horloge.monotone_ms())
        };
        for e in evenements {
            if puits.send(e).is_err() {
                return;
            }
        }
    }
}

/// Crée une instance du tuyau, **restreinte au compte courant**.
///
/// Le descripteur de sécurité est explicite : sans lui, un tuyau nommé est
/// joignable par n'importe quel processus de la machine, et le pont deviendrait
/// une porte d'entrée pour écrire de faux épisodes.
///
/// `D:P(A;;GA;;;OW)(A;;GA;;;SY)` — DACL protégée, tous droits au propriétaire du
/// tuyau et au système, personne d'autre.
#[cfg(not(test))]
fn creer_tuyau() -> std::io::Result<std::fs::File> {
    use std::os::windows::io::FromRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
    use windows::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    let nom: Vec<u16> = TUYAU.encode_utf16().chain(std::iter::once(0)).collect();
    let sddl: Vec<u16> = "D:P(A;;GA;;;OW)(A;;GA;;;SY)"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY : le descripteur est construit par Windows, rendu dans `sd`, et
    // reste vivant jusqu'après `CreateNamedPipeW`. Il fuit d'un descripteur par
    // connexion — borné par le nombre de connexions d'un épisode, et le
    // processus le rend à sa sortie. Le fermer proprement demanderait
    // `LocalFree`, qui n'est pas exposé par les features retenues.
    unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )?;
        let attributs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        let poignee = CreateNamedPipeW(
            PCWSTR(nom.as_ptr()),
            PIPE_ACCESS_INBOUND,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            4,
            0,
            64 * 1024,
            0,
            Some(&attributs),
        );
        if poignee.is_invalid() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(std::fs::File::from_raw_handle(poignee.0))
    }
}

#[cfg(not(test))]
fn attendre_connexion(tuyau: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE};
    use windows::Win32::System::Pipes::ConnectNamedPipe;

    // SAFETY : la poignée appartient au `File` passé en argument, vivant pendant
    // tout l'appel.
    unsafe {
        let h = HANDLE(tuyau.as_raw_handle());
        match ConnectNamedPipe(h, None) {
            Ok(()) => true,
            // Un client déjà connecté avant l'appel n'est pas une erreur.
            Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult(),
        }
    }
}

/// Le service du tuyau, pour le banc de la tâche 6b. **Hors production.**
///
/// Le banc emprunte le MÊME service que la production — même descripteur de
/// sécurité, mêmes instances multiples, même suiveur partagé — sinon il
/// vérifierait le banc et pas le pont. C'est d'ailleurs comme ça que le défaut
/// d'instance unique s'est vu.
#[cfg(not(test))]
pub fn banc_servir(
    puits: &Sender<RawEvent>,
    actif: &Arc<std::sync::atomic::AtomicBool>,
    horloge: &Arc<dyn crate::horloge::Horloge>,
    bilan: &Arc<Mutex<Bilan>>,
) {
    servir(puits, actif, horloge, bilan);
}

/// Une instance de tuyau prête à recevoir, pour un banc qui veut lire lui-même
/// les lignes brutes. **Hors production.**
#[cfg(not(test))]
pub fn banc_tuyau() -> Option<std::fs::File> {
    let tuyau = creer_tuyau().ok()?;
    if attendre_connexion(&tuyau) {
        Some(tuyau)
    } else {
        None
    }
}

#[cfg(test)]
impl CaptureSource for DomSource {
    /// Sans tuyau, la source s'abonne et ne pousse rien : c'est la logique de
    /// `traiter_ligne` qui est vérifiée, et elle l'est sans réseau.
    fn abonner(&mut self, _puits: Sender<RawEvent>) -> Result<Abonnement, ErreurSource> {
        use std::sync::atomic::Ordering;
        if self.actif.load(Ordering::SeqCst) {
            return Err(ErreurSource::DejaAbonne);
        }
        Ok(Abonnement::nouveau(self.actif.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(genre: &str, seq: u64, vie: &str) -> Observation {
        Observation {
            genre: genre.to_string(),
            cible: CibleDom {
                role: "button".into(),
                nom: "Enregistrer".into(),
                region: Some("Details".into()),
                chemin: "div[0]/button[2]".into(),
                data: std::collections::BTreeMap::new(),
            },
            seq,
            vie: vie.to_string(),
            onglet: Some(7),
            cadre: 0,
            etait_vide: None,
            est_vide: None,
        }
    }

    fn ligne(o: &Observation) -> String {
        serde_json::to_string(o).unwrap()
    }

    #[test]
    fn une_observation_devient_un_evenement_de_source_dom() {
        // R2.1 : un episode mixte doit dire quelle source a produit quel
        // evenement, sinon un defaut de capture devient indiagnosticable.
        let e = en_evenement(&observation("invocation", 1, "v1"), 1_000).expect("evenement");
        assert_eq!(e.source, crate::source::Source::Dom);
        assert_eq!(e.monotone_ms, 1_000);
        assert_eq!(e.surface.as_deref(), Some(SURFACE));
        match e.genre {
            GenreEvenement::Invocation(c) => {
                assert_eq!(c.role, "button");
                assert_eq!(c.nom, "Enregistrer");
                assert_eq!(c.region.as_deref(), Some("Details"));
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn les_quatre_genres_du_capteur_sont_traduits() {
        for (genre, attendu) in [
            ("focus", "focus"),
            ("invocation", "invocation"),
            ("changement_valeur", "changement_valeur"),
            ("soumission", "soumission"),
        ] {
            let e = en_evenement(&observation(genre, 1, "v1"), 0);
            assert!(e.is_some(), "{attendu} non traduit");
        }
    }

    #[test]
    fn un_genre_inconnu_est_compte_et_pas_interprete() {
        // Il vient d'une version d'extension qu'on ne connait pas. L'interpreter
        // de travers serait pire que de l'ignorer — mais l'ignorer en silence
        // serait pire encore.
        let mut s = Suiveur::default();
        let mut b = Bilan::default();
        let e = traiter_ligne(
            &ligne(&observation("teleportation", 1, "v1")),
            &mut s,
            &mut b,
            0,
        );
        assert!(e.is_empty());
        assert_eq!(b.genres_inconnus, 1);
        assert_eq!(b.recues, 1);
    }

    #[test]
    fn une_ligne_illisible_compte_comme_une_rupture() {
        // On ne sait pas ce qu'elle contenait : la traiter comme un non-evenement
        // ferait disparaitre une observation sans laisser de trace.
        let mut s = Suiveur::default();
        let mut b = Bilan::default();
        // Elle produit desormais un evenement : une ligne illisible est une
        // observation perdue, et une observation perdue est un TROU. Le compteur
        // seul ne suffisait pas — personne ne le lisait.
        let sortie = traiter_ligne("{ pas du json", &mut s, &mut b, 0);
        assert_eq!(sortie.len(), 1);
        assert!(matches!(
            sortie[0].genre,
            GenreEvenement::RuptureFlux { .. }
        ));
        assert_eq!(b.ruptures, 1);
        assert_eq!(b.recues, 0);
    }

    #[test]
    fn un_saut_de_numerotation_est_compte() {
        let mut s = Suiveur::default();
        let mut b = Bilan::default();
        traiter_ligne(&ligne(&observation("focus", 1, "v1")), &mut s, &mut b, 0);
        traiter_ligne(&ligne(&observation("focus", 5, "v1")), &mut s, &mut b, 0);
        assert_eq!(b.ruptures, 3, "les observations 2, 3 et 4 manquent");
    }

    #[test]
    fn un_redemarrage_de_worker_n_invente_pas_un_trou_geant() {
        // MV3 arrete et relance le service worker quand il veut, et la
        // numerotation repart a zero. Compter la difference produirait un trou
        // imaginaire de la taille de tout ce qui a precede.
        let mut s = Suiveur::default();
        let mut b = Bilan::default();
        for seq in 1..=50 {
            traiter_ligne(&ligne(&observation("focus", seq, "v1")), &mut s, &mut b, 0);
        }
        assert_eq!(b.ruptures, 0);
        traiter_ligne(&ligne(&observation("focus", 1, "v2")), &mut s, &mut b, 0);
        assert_eq!(b.ruptures, 1, "une rupture, pas cinquante");
    }

    #[test]
    fn la_coupure_du_worker_ne_passe_pas_inapercue() {
        // Sans identifiant de vie, deux moities d'episode se recolleraient en
        // silence — le trou non declare que la regle 4 interdit.
        let mut s = Suiveur::default();
        assert_eq!(s.constater(&observation("focus", 1, "v1")), Rupture::Aucune);
        assert_eq!(s.constater(&observation("focus", 2, "v1")), Rupture::Aucune);
        assert_eq!(
            s.constater(&observation("focus", 1, "v2")),
            Rupture::WorkerRedemarre
        );
    }

    #[test]
    fn tout_ce_qui_vient_du_navigateur_porte_la_meme_surface() {
        // R2.1 partitionne par CLASSE de surface, pas par onglet : l'operateur
        // active « le navigateur » ou ne l'active pas. Une surface par onglet
        // rendrait la liste blanche inutilisable.
        for genre in ["focus", "invocation", "soumission"] {
            let e = en_evenement(&observation(genre, 1, "v1"), 0).unwrap();
            assert_eq!(e.surface.as_deref(), Some("chrome.exe"));
        }
    }

    #[test]
    fn le_changement_de_valeur_ne_porte_aucune_valeur() {
        // Le capteur envoie deux booleens — « c'etait vide », « c'est vide » —
        // et jamais ce que l'operateur a tape. Le RawEvent ne doit pas en porter
        // plus : ce qui n'existe pas ne peut pas fuir.
        let mut o = observation("changement_valeur", 1, "v1");
        o.etait_vide = Some(true);
        o.est_vide = Some(false);
        let e = en_evenement(&o, 0).unwrap();
        let serialise = serde_json::to_string(&e.genre).unwrap();
        assert!(!serialise.contains("vide"), "{serialise}");
    }

    #[test]
    fn une_seconde_souscription_est_refusee() {
        let mut s = DomSource::new(Arc::new(crate::horloge::HorlogeSimulee::new()));
        let (tx, _rx) = std::sync::mpsc::channel();
        let _a = s.abonner(tx).unwrap();
        let (tx2, _rx2) = std::sync::mpsc::channel();
        assert!(matches!(s.abonner(tx2), Err(ErreurSource::DejaAbonne)));
    }

    #[test]
    fn le_tuyau_est_local_et_nomme() {
        // Pas un port TCP : un port localhost est joignable par n'importe quel
        // programme du poste, y compris par une page web via `fetch`. C'est la
        // difference entre « ca ne sort pas de la machine » et « ca ne sort pas
        // du compte ».
        assert!(TUYAU.starts_with(r"\\.\pipe\"), "{TUYAU}");
        assert!(!TUYAU.contains("127.0.0.1") && !TUYAU.contains("localhost"));
    }

    // -- Tache 6d : la frontiere entre les deux sources ---------------------

    /// Un episode mixte, tel qu'il se produit : l'operateur lit un courriel dans
    /// Outlook, bascule vers le CRM dans le navigateur, revient.
    fn episode_mixte() -> Vec<RawEvent> {
        use crate::source::{Cible, Source};
        let natif = |ms: u64, nom: &str| RawEvent {
            source: Source::Uia,
            monotone_ms: ms,
            genre: GenreEvenement::Invocation(Cible::new("button", nom)),
            surface: Some("outlook.exe".into()),
        };
        let bascule = |ms: u64, vers: &str| RawEvent {
            source: Source::Uia,
            monotone_ms: ms,
            genre: GenreEvenement::BasculeApplication {
                vers: vers.to_string(),
            },
            surface: Some(vers.to_string()),
        };
        let navigateur = |ms: u64, genre: &str, nom: &str| {
            let mut o = Observation {
                genre: genre.to_string(),
                cible: CibleDom {
                    role: "button".into(),
                    nom: nom.into(),
                    region: None,
                    chemin: String::new(),
                    data: std::collections::BTreeMap::new(),
                },
                seq: ms,
                vie: "v1".into(),
                onglet: Some(1),
                cadre: 0,
                etait_vide: None,
                est_vide: None,
            };
            o.seq = ms;
            en_evenement(&o, ms).expect("genre connu")
        };

        vec![
            natif(0, "Repondre"),
            bascule(1_000, "chrome.exe"),
            navigateur(2_000, "invocation", "Modifier"),
            navigateur(3_000, "soumission", "Enregistrer"),
            bascule(4_000, "outlook.exe"),
            natif(5_000, "Archiver"),
        ]
    }

    #[test]
    fn un_episode_mixte_garde_la_source_de_chaque_evenement() {
        // R2.1 : sans ce champ, un defaut de capture dans un episode mixte
        // devient indiagnosticable — on ne sait meme pas quelle source a parle.
        let e = episode_mixte();
        let sources: Vec<crate::source::Source> = e.iter().map(|x| x.source).collect();
        assert_eq!(
            sources,
            vec![
                crate::source::Source::Uia,
                crate::source::Source::Uia,
                crate::source::Source::Dom,
                crate::source::Source::Dom,
                crate::source::Source::Uia,
                crate::source::Source::Uia,
            ]
        );
    }

    #[test]
    fn la_frontiere_n_introduit_ni_doublon_ni_trou() {
        // La partition est par CLASSE de surface, jamais par bascule dynamique
        // sur une meme surface : chaque evenement appartient a une source et une
        // seule, donc la frontiere ne peut ni dupliquer ni perdre.
        use crate::horloge::HorlogeSimulee;
        use crate::moteur::{EntreeJournal, Moteur};

        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let redacteur = std::sync::Arc::new(crate::redaction::Redacteur::new(
            &crate::cle::CleHmac::generer().unwrap(),
        ));
        let mut m = Moteur::ouvrir(h, redacteur, "outlook.exe").avec_liste_blanche(
            crate::surfaces::ListeBlanche::depuis(["outlook.exe", "chrome.exe"]),
        );

        let evenements = episode_mixte();
        let attendues = evenements
            .iter()
            .filter(|e| !matches!(e.genre, GenreEvenement::BasculeApplication { .. }))
            .count();
        for e in evenements {
            m.traiter(e);
        }

        let actions: Vec<&EntreeJournal> = m
            .journal()
            .iter()
            .filter(|e| matches!(e, EntreeJournal::UiAction { .. }))
            .collect();
        assert_eq!(
            actions.len(),
            attendues,
            "ni doublon ni perte : {actions:?}"
        );
        assert!(
            m.gaps().is_empty(),
            "la frontiere ne doit produire aucun trou : {:?}",
            m.gaps()
        );
        assert_eq!(m.hors_perimetre(), 0, "les deux surfaces sont activees");
    }

    #[test]
    fn le_navigateur_hors_liste_blanche_ne_capture_rien() {
        // La liste blanche gouverne les DEUX sources de la meme facon. Activer
        // les applications natives sans activer le navigateur doit vraiment
        // laisser le navigateur dehors.
        use crate::horloge::HorlogeSimulee;
        use crate::moteur::Moteur;

        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let redacteur = std::sync::Arc::new(crate::redaction::Redacteur::new(
            &crate::cle::CleHmac::generer().unwrap(),
        ));
        let mut m = Moteur::ouvrir(h, redacteur, "outlook.exe")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["outlook.exe"]));

        for e in episode_mixte() {
            m.traiter(e);
        }
        let texte = m
            .journal()
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !texte.contains("Modifier"),
            "le DOM ne devait pas entrer :\n{texte}"
        );
        assert!(
            texte.contains("Repondre"),
            "le natif devait entrer :\n{texte}"
        );
        assert_eq!(m.hors_perimetre(), 2, "les deux observations DOM refusees");
    }
}
