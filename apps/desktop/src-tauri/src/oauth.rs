//! OAuth PKCE et le coffre à jetons (spec 003, R1.2 et R1.3).
//!
//! ## Pourquoi PKCE, et pourquoi une boucle locale
//!
//! Une application de bureau ne peut pas garder un secret client : il est dans le
//! binaire, donc il appartient à quiconque le télécharge. PKCE remplace ce secret
//! par une preuve **fabriquée à chaque échange** — on tire un vérificateur au
//! hasard, on n'envoie que son condensat, et on ne révèle le vérificateur qu'au
//! moment de convertir le code. Un attaquant qui intercepte le code ne peut rien
//! en faire sans le vérificateur, qui n'a jamais transité.
//!
//! Le retour se fait sur `http://127.0.0.1:<port>/cb`, sur un écouteur **éphémère**
//! ouvert le temps de l'échange. `127.0.0.1` et non `localhost` : le second peut
//! résoudre en IPv6 ou, sur certains postes, ailleurs. Et surtout pas `0.0.0.0`,
//! qui exposerait le callback au réseau local — un code d'autorisation qui passe
//! par le Wi-Fi d'un café n'est plus un secret.
//!
//! ## Ce que ce module ne fait pas
//!
//! Il ne demande **jamais** les identifiants de l'opérateur. Le navigateur
//! système s'en charge, sur la page du fournisseur, où l'opérateur reconnaît
//! l'adresse. C'est l'un des trois irréductibles de la doctrine : les secrets se
//! tapent, ils ne se transmettent pas.

//! ## Ce qui est branché, et ce qui attend
//!
//! Le **coffre à jetons** et l'**état du connecteur** ont un appelant : le
//! démarrage les lit, le tray les montre, la panique les efface.
//!
//! Le **flux d'autorisation** — PKCE, écouteur de retour, URL — n'en a pas
//! encore : il lui faut une application connectée chez le fournisseur, et cette
//! application se crée dans l'org de démo, dont les identifiants ont été perdus
//! (incident du 2026-08-27). Le code est écrit et testé ; il attend un compte.
//!
//! L'annotation ci-dessous porte le numéro de la tâche qui devra la retirer.
//! Un `allow` sans échéance devient permanent, et finit par masquer du vrai code
//! mort.
#![allow(dead_code)] // retiré quand la tâche 0 rend l'org accessible

use crate::cle::ErreurCle;

/// R1.3 — on rafraîchit **avant** l'expiration, pas au premier échec.
///
/// Cinq minutes de marge. Attendre le 401 marcherait aussi, mais coûterait un
/// aller-retour raté au pire moment : celui où une lecture d'état est en cours et
/// où l'épisode se ferme dans quelques secondes.
pub const MARGE_RAFRAICHISSEMENT_MS: u64 = 5 * 60 * 1000;

/// Longueur du vérificateur PKCE, en octets tirés au sort.
///
/// La RFC 7636 exige entre 43 et 128 **caractères** après encodage. Trente-deux
/// octets donnent quarante-trois caractères en base64url sans remplissage : le
/// minimum de la norme, et déjà 256 bits d'entropie.
const OCTETS_VERIFICATEUR: usize = 32;

/// L'alphabet base64url de la RFC 4648 §5 — sans remplissage.
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode en base64url **sans remplissage**, comme PKCE l'exige.
///
/// Le remplissage `=` est explicitement interdit par la RFC 7636 : un serveur
/// strict rejette le condensat, et le message d'erreur ne dit jamais pourquoi.
pub fn base64url(octets: &[u8]) -> String {
    let mut sortie = String::with_capacity(octets.len().div_ceil(3) * 4);
    for morceau in octets.chunks(3) {
        let a = u32::from(morceau[0]);
        let b = morceau.get(1).copied().map_or(0, u32::from);
        let c = morceau.get(2).copied().map_or(0, u32::from);
        let bloc = (a << 16) | (b << 8) | c;
        let indices = [
            (bloc >> 18) & 0x3f,
            (bloc >> 12) & 0x3f,
            (bloc >> 6) & 0x3f,
            bloc & 0x3f,
        ];
        // Trois octets donnent quatre caractères ; deux en donnent trois ; un, deux.
        let utiles = morceau.len() + 1;
        for i in indices.iter().take(utiles) {
            sortie.push(BASE64URL[*i as usize] as char);
        }
    }
    sortie
}

/// Le couple PKCE d'un échange.
///
/// `Debug` est écrit à la main : le vérificateur est un secret de courte vie, et
/// un `{:?}` dans un message d'erreur suffirait à le poser dans un journal.
pub struct Pkce {
    verificateur: String,
    pub challenge: String,
    /// R1.2 — la valeur anti-rejeu du paramètre `state`.
    pub etat: String,
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkce")
            .field("verificateur", &"(masque)")
            .field("challenge", &self.challenge)
            .field("etat", &self.etat)
            .finish()
    }
}

impl Pkce {
    /// Tire un couple neuf.
    ///
    /// **Un par échange, jamais réutilisé.** Réutiliser un vérificateur revient à
    /// avoir un secret client — c'est-à-dire à annuler tout l'intérêt de PKCE.
    pub fn tirer() -> Result<Self, ErreurCle> {
        use ring::rand::SecureRandom;

        let alea = ring::rand::SystemRandom::new();
        let mut brut = [0u8; OCTETS_VERIFICATEUR];
        alea.fill(&mut brut).map_err(|_| ErreurCle::Alea)?;
        let verificateur = base64url(&brut);

        let mut etat_brut = [0u8; 16];
        alea.fill(&mut etat_brut).map_err(|_| ErreurCle::Alea)?;

        let condensat = ring::digest::digest(&ring::digest::SHA256, verificateur.as_bytes());
        Ok(Self {
            challenge: base64url(condensat.as_ref()),
            verificateur,
            etat: base64url(&etat_brut),
        })
    }

    /// Le vérificateur, pour l'échange du code. **Consomme** le couple.
    ///
    /// Consommer plutôt que prêter : après l'échange, le vérificateur n'a plus
    /// aucun usage légitime, et le type interdit de le redemander.
    pub fn consommer(self) -> String {
        self.verificateur
    }

    /// La méthode de challenge. `S256` seulement — `plain` transmet le
    /// vérificateur en clair dans l'URL, ce qui vide PKCE de son sens.
    pub fn methode() -> &'static str {
        "S256"
    }
}

/// Ce que le fournisseur a rendu.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Jetons {
    pub acces: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rafraichissement: Option<String>,
    /// L'instant mural d'expiration, en millisecondes.
    pub expire_a_ms: u64,
    /// L'instance du fournisseur — Salesforce en donne une par org.
    #[serde(default)]
    pub instance: String,
}

impl Jetons {
    /// R1.3 — faut-il rafraîchir maintenant ?
    ///
    /// La marge évite l'aller-retour raté au pire moment. Sans jeton de
    /// rafraîchissement, la réponse est **non** : il n'y a rien à tenter, et le
    /// dire évite une boucle d'échecs.
    pub fn a_rafraichir(&self, maintenant_ms: u64) -> bool {
        self.rafraichissement.is_some()
            && maintenant_ms + MARGE_RAFRAICHISSEMENT_MS >= self.expire_a_ms
    }

    pub fn expire(&self, maintenant_ms: u64) -> bool {
        maintenant_ms >= self.expire_a_ms
    }
}

/// L'état d'un connecteur, tel que la barre d'état le montre (R1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EtatConnecteur {
    /// Aucun jeton : le connecteur n'a jamais été branché.
    #[default]
    Absent,
    /// Tout va bien.
    Connecte,
    /// **Le rafraîchissement a échoué définitivement.**
    ///
    /// Pas un crash, pas une perte d'épisode : les lectures manquées deviennent
    /// des trous classés, et l'opérateur voit la question en attente au tray.
    /// R1.3 est explicite là-dessus, et c'est la même doctrine que partout —
    /// mieux vaut un trou déclaré qu'un arrêt.
    ReauthRequise,
}

impl EtatConnecteur {
    pub fn infobulle(self) -> &'static str {
        match self {
            Self::Absent => "aucun systeme branche",
            Self::Connecte => "systemes branches",
            Self::ReauthRequise => "reconnexion demandee",
        }
    }
}

/// Le coffre à jetons : DPAPI, comme la clé HMAC.
///
/// R1.2 : « les tokens DOIVENT être stockés via DPAPI et NE DOIVENT JAMAIS
/// apparaître dans un fichier suivi, un log ou un export non enveloppé ».
///
/// Même mécanique que `cle.rs`, et **même entropie applicative distincte** : un
/// blob de jetons ne doit pas se déchiffrer avec la mécanique prévue pour la clé
/// HMAC, sinon une confusion de chemin livrerait l'un pour l'autre.
pub struct CoffreJetons;

/// L'entropie applicative du coffre à jetons. Différente de celle de la clé HMAC.
const ENTROPIE: &[u8] = b"noe.federation.tokens.v1";

impl CoffreJetons {
    /// Écrit les jetons, chiffrés pour le compte Windows courant.
    pub fn enregistrer(chemin: &std::path::Path, jetons: &Jetons) -> Result<(), ErreurCle> {
        let clair = serde_json::to_vec(jetons)
            .map_err(|e| ErreurCle::Disque(format!("serialisation : {e}")))?;
        let chiffre = crate::cle::proteger_octets(&clair, ENTROPIE)?;
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ErreurCle::Disque(e.to_string()))?;
        }
        std::fs::write(chemin, &chiffre).map_err(|e| ErreurCle::Disque(e.to_string()))
    }

    /// Relit les jetons.
    ///
    /// **Un blob illisible n'est pas une erreur fatale** — contrairement à la clé
    /// HMAC. Perdre la clé rend tout le corpus muet ; perdre un jeton coûte une
    /// reconnexion. On rend donc `ReauthRequise` et l'application continue, ce que
    /// R1.3 exige : « sans crash ni perte d'épisode ».
    pub fn charger(chemin: &std::path::Path) -> Resultat {
        if !chemin.exists() {
            return Resultat::Absent;
        }
        let Ok(chiffre) = std::fs::read(chemin) else {
            return Resultat::Illisible("fichier de jetons illisible".into());
        };
        let Ok(clair) = crate::cle::deproteger_octets(&chiffre, ENTROPIE) else {
            // Profil Windows changé, fichier tronqué, blob d'un autre usage :
            // dans tous les cas, la reconnexion est le remède.
            return Resultat::Illisible("blob de jetons indechiffrable".into());
        };
        match serde_json::from_slice::<Jetons>(&clair) {
            Ok(j) => Resultat::Jetons(Box::new(j)),
            Err(e) => Resultat::Illisible(format!("jetons corrompus : {e}")),
        }
    }

    /// Efface les jetons. À la déconnexion, et **à la panique**.
    pub fn effacer(chemin: &std::path::Path) -> std::io::Result<()> {
        if chemin.exists() {
            std::fs::remove_file(chemin)?;
        }
        Ok(())
    }
}

/// Ce que le chargement a donné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resultat {
    Jetons(Box<Jetons>),
    Absent,
    /// Avec sa raison : « illisible » tout court ne se diagnostique pas.
    Illisible(String),
}

impl Resultat {
    /// L'état de connecteur qui en découle.
    pub fn etat(&self) -> EtatConnecteur {
        match self {
            Self::Jetons(_) => EtatConnecteur::Connecte,
            Self::Absent => EtatConnecteur::Absent,
            Self::Illisible(_) => EtatConnecteur::ReauthRequise,
        }
    }
}

/// L'URL d'autorisation, telle qu'on ouvre le navigateur système dessus.
///
/// Les valeurs sont **encodées** : une URL de redirection non encodée casse chez
/// certains fournisseurs et, pire, chez d'autres elle passe en tronquant
/// silencieusement les paramètres suivants.
pub fn url_autorisation(
    base: &str,
    client_id: &str,
    redirection: &str,
    pkce: &Pkce,
    portees: &[&str],
) -> String {
    let e = |s: &str| encoder_composant(s);
    format!(
        "{base}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}\
         &code_challenge_method={}&state={}&scope={}",
        e(client_id),
        e(redirection),
        e(&pkce.challenge),
        Pkce::methode(),
        e(&pkce.etat),
        e(&portees.join(" ")),
    )
}

/// Encodage de composant d'URL, réduit à ce dont on a besoin.
///
/// Écrit à la main plutôt qu'emprunté : ajouter une dépendance pour trente
/// lignes coûterait plus en surface qu'il ne rapporte, et l'ensemble des
/// caractères non réservés de la RFC 3986 tient en une ligne.
pub fn encoder_composant(s: &str) -> String {
    let mut sortie = String::with_capacity(s.len());
    for o in s.as_bytes() {
        match o {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                sortie.push(*o as char);
            }
            autre => sortie.push_str(&format!("%{autre:02X}")),
        }
    }
    sortie
}

/// Ce que le callback de la boucle locale a rendu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Retour {
    Code(String),
    /// Le fournisseur a refusé. La raison vient de lui, on la garde telle quelle.
    Refus(String),
    /// R1.2 — le `state` ne correspond pas. On jette, **sans échanger le code**.
    EtatInvalide,
}

/// Analyse la requête HTTP du callback.
///
/// **La vérification du `state` est ici, avant tout usage du code.** Un code
/// échangé sans cette vérification autorise une attaque par requête forgée : un
/// tiers fait aboutir SON autorisation dans NOTRE application, et le connecteur
/// finit branché sur le compte de l'attaquant.
pub fn analyser_callback(ligne_requete: &str, etat_attendu: &str) -> Retour {
    // « GET /cb?code=...&state=... HTTP/1.1 »
    let chemin = ligne_requete.split_whitespace().nth(1).unwrap_or("");
    let requete = chemin.split_once('?').map(|(_, q)| q).unwrap_or("");

    let mut code = None;
    let mut etat = None;
    let mut erreur = None;
    for paire in requete.split('&') {
        let Some((cle, valeur)) = paire.split_once('=') else {
            continue;
        };
        let valeur = decoder_composant(valeur);
        match cle {
            "code" => code = Some(valeur),
            "state" => etat = Some(valeur),
            "error" => erreur = Some(valeur),
            "error_description" if erreur.is_some() => {
                erreur = Some(format!("{} : {valeur}", erreur.unwrap_or_default()));
            }
            _ => {}
        }
    }

    // L'état d'abord, même en cas de refus : un refus dont l'état ne correspond
    // pas ne vient pas de notre échange, et son message ne nous concerne pas.
    if etat.as_deref() != Some(etat_attendu) {
        return Retour::EtatInvalide;
    }
    if let Some(e) = erreur {
        return Retour::Refus(e);
    }
    match code {
        Some(c) if !c.is_empty() => Retour::Code(c),
        _ => Retour::Refus("aucun code dans le callback".into()),
    }
}

/// Décodage de composant d'URL — l'inverse d'`encoder_composant`.
pub fn decoder_composant(s: &str) -> String {
    let octets = s.as_bytes();
    let mut sortie: Vec<u8> = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        match octets[i] {
            b'%' if i + 2 < octets.len() => {
                let h = std::str::from_utf8(&octets[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(h, 16) {
                    Ok(o) => {
                        sortie.push(o);
                        i += 3;
                    }
                    Err(_) => {
                        sortie.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                sortie.push(b' ');
                i += 1;
            }
            autre => {
                sortie.push(autre);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

/// L'écouteur de retour : **éphémère, local, et à usage unique**.
///
/// Trois propriétés, et chacune ferme une porte.
///
/// **Éphémère** : le port est choisi par le système (`:0`) et l'écouteur meurt
/// après un échange. Un port fixe serait un rendez-vous qu'un autre programme du
/// poste peut occuper avant nous — ou pire, occuper *à notre place* et récolter
/// le code.
///
/// **Local** : `127.0.0.1` et jamais `0.0.0.0`. Le second exposerait le callback
/// au réseau, et un code d'autorisation qui passe par le Wi-Fi d'un café n'est
/// plus un secret. `127.0.0.1` plutôt que `localhost` parce que le nom peut
/// résoudre en IPv6 — ou, sur un poste mal configuré, ailleurs.
///
/// **À usage unique** : une seule connexion est servie. Rien ne reste ouvert
/// derrière, et un second code arrivé plus tard ne trouve personne.
pub struct EcouteurRetour {
    ecouteur: std::net::TcpListener,
    port: u16,
}

impl EcouteurRetour {
    /// Ouvre l'écouteur sur un port libre.
    pub fn ouvrir() -> std::io::Result<Self> {
        let ecouteur = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let port = ecouteur.local_addr()?.port();
        Ok(Self { ecouteur, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// L'URL de redirection à déclarer au fournisseur.
    pub fn redirection(&self) -> String {
        format!("http://127.0.0.1:{}/cb", self.port)
    }

    /// Attend le retour du navigateur, **au plus** `delai_max`.
    ///
    /// Une attente sans borne bloquerait l'application si l'opérateur ferme
    /// l'onglet sans répondre — et il le fera, parce que c'est un geste normal.
    pub fn attendre(self, delai_max: std::time::Duration, etat_attendu: &str) -> Retour {
        self.ecouteur
            .set_nonblocking(false)
            .and_then(|()| {
                // Le délai s'applique à l'ACCEPT, qui est la partie longue : le
                // navigateur peut mettre une minute à revenir, mais une fois
                // connecté il envoie sa requête tout de suite.
                let debut = std::time::Instant::now();
                self.ecouteur.set_nonblocking(true)?;
                loop {
                    match self.ecouteur.accept() {
                        Ok((flux, _)) => return Ok(flux),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if debut.elapsed() >= delai_max {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::TimedOut,
                                    "aucun retour du navigateur",
                                ));
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        Err(e) => return Err(e),
                    }
                }
            })
            .map_or_else(
                |e| Retour::Refus(format!("ecouteur de retour : {e}")),
                |mut flux| servir(&mut flux, etat_attendu),
            )
    }
}

/// Lit la requête, répond une page, et rend le verdict.
fn servir(flux: &mut std::net::TcpStream, etat_attendu: &str) -> Retour {
    use std::io::{BufRead, BufReader, Write};

    let _ = flux.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let mut lecteur = BufReader::new(match flux.try_clone() {
        Ok(f) => f,
        Err(e) => return Retour::Refus(format!("flux illisible : {e}")),
    });
    let mut ligne = String::new();
    if lecteur.read_line(&mut ligne).is_err() {
        return Retour::Refus("requete de retour illisible".into());
    }
    let verdict = analyser_callback(ligne.trim_end(), etat_attendu);

    // La page que voit l'opérateur. Elle ne dit RIEN du code ni de l'état : cette
    // page est rendue par le navigateur, elle entre dans l'historique, et son
    // contenu est lisible par les extensions installées.
    let message = match &verdict {
        Retour::Code(_) => "Connexion etablie. Vous pouvez fermer cet onglet.",
        Retour::Refus(_) => "Connexion refusee. Vous pouvez fermer cet onglet.",
        Retour::EtatInvalide => "Retour invalide, ignore. Vous pouvez fermer cet onglet.",
    };
    let corps = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Noe</title>\
         <body style=\"font:16px system-ui;margin:3rem\"><p>{message}</p>"
    );
    let reponse = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{corps}",
        corps.len()
    );
    let _ = flux.write_all(reponse.as_bytes());
    let _ = flux.flush();
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporaire(nom: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("noe-oauth-{nom}-{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn jetons(expire_a_ms: u64, avec_rafraichissement: bool) -> Jetons {
        Jetons {
            acces: "jeton-d-acces".into(),
            rafraichissement: avec_rafraichissement.then(|| "jeton-de-refresh".to_string()),
            expire_a_ms,
            instance: "https://exemple.my.salesforce.com".into(),
        }
    }

    // -- PKCE ---------------------------------------------------------------

    #[test]
    fn le_verificateur_tient_la_norme() {
        // RFC 7636 : entre 43 et 128 caracteres, et seulement des caracteres non
        // reserves. Trente-deux octets en base64url sans remplissage en donnent
        // exactement 43 — le minimum, et deja 256 bits d'entropie.
        let p = Pkce::tirer().unwrap();
        let v = p.consommer();
        assert!((43..=128).contains(&v.len()), "{} caracteres", v.len());
        assert!(
            v.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "{v}"
        );
    }

    #[test]
    fn deux_tirages_ne_donnent_jamais_le_meme_couple() {
        // Reutiliser un verificateur revient a avoir un secret client — c'est-a-
        // dire a annuler tout l'interet de PKCE.
        let a = Pkce::tirer().unwrap();
        let b = Pkce::tirer().unwrap();
        assert_ne!(a.challenge, b.challenge);
        assert_ne!(a.etat, b.etat);
        assert_ne!(a.consommer(), b.consommer());
    }

    #[test]
    fn le_challenge_est_le_condensat_du_verificateur() {
        // C'est TOUT le mecanisme : on n'envoie que le condensat, et le
        // verificateur ne part qu'au moment d'echanger le code.
        let p = Pkce::tirer().unwrap();
        let challenge = p.challenge.clone();
        let v = p.consommer();
        let attendu = base64url(ring::digest::digest(&ring::digest::SHA256, v.as_bytes()).as_ref());
        assert_eq!(challenge, attendu);
    }

    #[test]
    fn la_methode_est_s256_et_jamais_plain() {
        // `plain` transmet le verificateur en clair dans l'URL, ce qui vide PKCE
        // de son sens — et certains fournisseurs l'acceptent encore.
        assert_eq!(Pkce::methode(), "S256");
    }

    #[test]
    fn le_verificateur_ne_s_imprime_pas() {
        // Un `{:?}` dans un message d'erreur suffirait a poser le secret dans un
        // journal. Le `Debug` est donc ecrit a la main.
        let p = Pkce::tirer().unwrap();
        let rendu = format!("{p:?}");
        assert!(rendu.contains("masque"), "{rendu}");
        let v = p.consommer();
        assert!(!rendu.contains(&v), "le verificateur a fuite dans Debug");
    }

    // -- base64url ----------------------------------------------------------

    #[test]
    fn base64url_n_a_pas_de_remplissage() {
        // La RFC 7636 l'interdit explicitement. Un serveur strict rejette le
        // condensat, et le message d'erreur ne dit jamais pourquoi.
        for n in 1..40 {
            let e = base64url(&vec![0xABu8; n]);
            assert!(!e.contains('='), "{n} octets : {e}");
            assert!(!e.contains('+') && !e.contains('/'), "{n} octets : {e}");
        }
    }

    #[test]
    fn base64url_encode_les_vecteurs_de_la_rfc() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64url_utilise_bien_l_alphabet_url() {
        // Les octets qui produisent `+` et `/` en base64 classique doivent donner
        // `-` et `_` ici, sinon l'URL casse a l'encodage.
        let e = base64url(&[0xfb, 0xff]);
        assert!(e.contains('-') || e.contains('_'), "{e}");
    }

    // -- L'URL d'autorisation ----------------------------------------------

    #[test]
    fn l_url_encode_ses_valeurs() {
        // Une URL de redirection non encodee casse chez certains fournisseurs et,
        // pire, passe chez d'autres en tronquant silencieusement les parametres
        // suivants.
        let p = Pkce::tirer().unwrap();
        let u = url_autorisation(
            "https://login.salesforce.com/services/oauth2/authorize",
            "3MVG9_id",
            "http://127.0.0.1:53123/cb",
            &p,
            &["api", "refresh_token"],
        );
        assert!(
            u.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A53123%2Fcb"),
            "{u}"
        );
        assert!(u.contains("scope=api%20refresh_token"), "{u}");
        assert!(u.contains("code_challenge_method=S256"), "{u}");
        assert!(!u.contains(' '), "une URL ne contient pas d espace : {u}");
    }

    #[test]
    fn l_url_ne_porte_jamais_le_verificateur() {
        let p = Pkce::tirer().unwrap();
        let u = url_autorisation(
            "https://x/auth",
            "id",
            "http://127.0.0.1:1/cb",
            &p,
            &["api"],
        );
        let v = p.consommer();
        assert!(
            !u.contains(&v),
            "le verificateur ne doit JAMAIS partir dans l URL"
        );
    }

    // -- Le callback -------------------------------------------------------

    #[test]
    fn un_callback_valide_rend_le_code() {
        let r = analyser_callback("GET /cb?code=abc123&state=ETAT HTTP/1.1", "ETAT");
        assert_eq!(r, Retour::Code("abc123".into()));
    }

    #[test]
    fn un_etat_qui_ne_correspond_pas_fait_jeter_le_code() {
        // Sans cette verification, un tiers fait aboutir SON autorisation dans
        // notre application, et le connecteur finit branche sur le compte de
        // l'attaquant. Le code n'est meme pas regarde.
        let r = analyser_callback("GET /cb?code=abc123&state=AUTRE HTTP/1.1", "ETAT");
        assert_eq!(r, Retour::EtatInvalide);
    }

    #[test]
    fn un_callback_sans_etat_est_refuse() {
        let r = analyser_callback("GET /cb?code=abc123 HTTP/1.1", "ETAT");
        assert_eq!(r, Retour::EtatInvalide);
    }

    #[test]
    fn un_refus_du_fournisseur_garde_sa_raison() {
        let r = analyser_callback(
            "GET /cb?error=access_denied&error_description=refus%20utilisateur&state=ETAT HTTP/1.1",
            "ETAT",
        );
        match r {
            Retour::Refus(m) => {
                assert!(m.contains("access_denied"), "{m}");
                assert!(m.contains("refus utilisateur"), "{m}");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_refus_dont_l_etat_ne_correspond_pas_ne_nous_concerne_pas() {
        // Il ne vient pas de notre echange : son message n'a rien a nous
        // apprendre, et l'afficher ferait croire a un refus de l'operateur.
        let r = analyser_callback("GET /cb?error=access_denied&state=AUTRE HTTP/1.1", "ETAT");
        assert_eq!(r, Retour::EtatInvalide);
    }

    #[test]
    fn l_encodage_d_url_fait_l_aller_retour() {
        for t in [
            "abc",
            "http://127.0.0.1:1234/cb",
            "api refresh_token",
            "a+b&c=d",
            "accentue: e",
        ] {
            assert_eq!(decoder_composant(&encoder_composant(t)), t, "{t}");
        }
    }

    // -- Le rafraichissement (R1.3) ----------------------------------------

    #[test]
    fn on_rafraichit_avant_l_expiration_pas_apres() {
        // Attendre le 401 marcherait, mais couterait un aller-retour rate au pire
        // moment : celui ou une lecture d'etat est en cours et ou l'episode se
        // ferme dans quelques secondes.
        let j = jetons(1_000_000, true);
        assert!(!j.a_rafraichir(1_000_000 - MARGE_RAFRAICHISSEMENT_MS - 1));
        assert!(j.a_rafraichir(1_000_000 - MARGE_RAFRAICHISSEMENT_MS));
        assert!(j.a_rafraichir(1_000_000));
        assert_eq!(MARGE_RAFRAICHISSEMENT_MS, 5 * 60 * 1000);
    }

    #[test]
    fn sans_jeton_de_rafraichissement_il_n_y_a_rien_a_tenter() {
        // Le dire evite une boucle d'echecs : on ne rafraichit pas ce qu'on ne
        // peut pas rafraichir.
        let j = jetons(1_000, false);
        assert!(!j.a_rafraichir(1_000_000));
        assert!(j.expire(1_000_000));
    }

    // -- Le coffre (R1.2, R1.3) --------------------------------------------

    #[test]
    fn les_jetons_font_l_aller_retour_par_dpapi() {
        let p = temporaire("aller-retour");
        let j = jetons(42_000, true);
        CoffreJetons::enregistrer(&p, &j).unwrap();
        assert_eq!(CoffreJetons::charger(&p), Resultat::Jetons(Box::new(j)));
        let _ = CoffreJetons::effacer(&p);
    }

    #[test]
    fn le_fichier_ne_contient_aucun_jeton_en_clair() {
        // R1.2 : « NE DOIVENT JAMAIS apparaitre dans un fichier suivi, un log ou
        // un export non enveloppe ». Le disque non plus.
        let p = temporaire("chiffre");
        CoffreJetons::enregistrer(&p, &jetons(42_000, true)).unwrap();
        let brut = std::fs::read(&p).unwrap();
        let texte = String::from_utf8_lossy(&brut);
        assert!(!texte.contains("jeton-d-acces"), "l acces a fuite");
        assert!(!texte.contains("jeton-de-refresh"), "le refresh a fuite");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn un_coffre_absent_n_est_pas_une_erreur() {
        let p = temporaire("absent");
        assert_eq!(CoffreJetons::charger(&p), Resultat::Absent);
        assert_eq!(Resultat::Absent.etat(), EtatConnecteur::Absent);
    }

    #[test]
    fn un_blob_corrompu_demande_une_reconnexion_sans_crasher() {
        // Le test que la tache 2 reclame nommement. Contrairement a la cle HMAC —
        // dont la perte rend tout le corpus muet et fait echouer le demarrage —
        // perdre un jeton coute une reconnexion. On rend `ReauthRequise` et
        // l'application continue : R1.3 dit « sans crash ni perte d'episode ».
        let p = temporaire("corrompu");
        std::fs::write(&p, b"ceci n est pas un blob DPAPI").unwrap();
        let r = CoffreJetons::charger(&p);
        match &r {
            Resultat::Illisible(raison) => assert!(raison.len() > 10, "{raison}"),
            autre => panic!("{autre:?}"),
        }
        assert_eq!(r.etat(), EtatConnecteur::ReauthRequise);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn un_blob_tronque_aussi() {
        let p = temporaire("tronque");
        CoffreJetons::enregistrer(&p, &jetons(1, true)).unwrap();
        let mut brut = std::fs::read(&p).unwrap();
        brut.truncate(brut.len() / 2);
        std::fs::write(&p, &brut).unwrap();
        assert_eq!(
            CoffreJetons::charger(&p).etat(),
            EtatConnecteur::ReauthRequise
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn effacer_un_coffre_absent_ne_leve_pas() {
        // La panique efface les jetons ; elle ne doit pas echouer parce qu'il n'y
        // en avait pas.
        let p = temporaire("efface-deux-fois");
        assert!(CoffreJetons::effacer(&p).is_ok());
        CoffreJetons::enregistrer(&p, &jetons(1, false)).unwrap();
        assert!(CoffreJetons::effacer(&p).is_ok());
        assert!(CoffreJetons::effacer(&p).is_ok());
    }

    #[test]
    fn chaque_etat_a_son_infobulle() {
        for e in [
            EtatConnecteur::Absent,
            EtatConnecteur::Connecte,
            EtatConnecteur::ReauthRequise,
        ] {
            assert!(e.infobulle().len() > 5, "{e:?}");
        }
    }

    // -- L'ecouteur de retour (R1.2) ---------------------------------------

    #[test]
    fn l_ecouteur_n_ecoute_que_la_boucle_locale() {
        // `0.0.0.0` exposerait le callback au reseau, et un code d'autorisation
        // qui passe par le Wi-Fi d'un cafe n'est plus un secret.
        let e = EcouteurRetour::ouvrir().unwrap();
        let adresse = e.ecouteur.local_addr().unwrap();
        assert!(adresse.ip().is_loopback(), "{adresse}");
        assert_eq!(adresse.ip().to_string(), "127.0.0.1", "ni IPv6, ni un nom");
    }

    #[test]
    fn le_port_est_choisi_par_le_systeme() {
        // Un port fixe serait un rendez-vous qu'un autre programme du poste peut
        // occuper avant nous — ou pire, occuper A NOTRE PLACE et recolter le code.
        let a = EcouteurRetour::ouvrir().unwrap();
        let b = EcouteurRetour::ouvrir().unwrap();
        assert_ne!(a.port(), b.port());
        assert_ne!(a.port(), 0);
    }

    #[test]
    fn la_redirection_pointe_sur_le_port_ouvert() {
        let e = EcouteurRetour::ouvrir().unwrap();
        assert_eq!(e.redirection(), format!("http://127.0.0.1:{}/cb", e.port()));
    }

    #[test]
    fn un_navigateur_qui_ne_revient_jamais_ne_bloque_pas() {
        // L'operateur ferme l'onglet sans repondre — c'est un geste normal, et il
        // le fera. Une attente sans borne bloquerait l'application.
        let e = EcouteurRetour::ouvrir().unwrap();
        let debut = std::time::Instant::now();
        let r = e.attendre(std::time::Duration::from_millis(200), "ETAT");
        assert!(debut.elapsed() < std::time::Duration::from_secs(3));
        match r {
            Retour::Refus(m) => assert!(m.contains("ecouteur"), "{m}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_retour_complet_rend_le_code_et_repond_une_page() {
        use std::io::{Read, Write};

        let e = EcouteurRetour::ouvrir().unwrap();
        let url = e.redirection();
        let fil = std::thread::spawn(move || e.attendre(std::time::Duration::from_secs(5), "ETAT"));

        // Le navigateur, joue a la main.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let port = url.rsplit(':').next().unwrap().trim_end_matches("/cb");
        let mut flux =
            std::net::TcpStream::connect(format!("127.0.0.1:{port}")).expect("connexion");
        flux.write_all(b"GET /cb?code=LECODE&state=ETAT HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut reponse = String::new();
        let _ = flux.read_to_string(&mut reponse);

        assert_eq!(fil.join().unwrap(), Retour::Code("LECODE".into()));
        assert!(reponse.starts_with("HTTP/1.1 200"), "{reponse}");
        assert!(reponse.contains("fermer cet onglet"), "{reponse}");
    }

    #[test]
    fn la_page_de_retour_ne_montre_ni_code_ni_etat() {
        use std::io::{Read, Write};

        // Cette page est rendue par le navigateur : elle entre dans l'historique
        // et son contenu est lisible par les extensions installees. Y ecrire le
        // code reviendrait a le publier.
        let e = EcouteurRetour::ouvrir().unwrap();
        let port = e.port();
        let fil =
            std::thread::spawn(move || e.attendre(std::time::Duration::from_secs(5), "ETATSECRET"));

        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut flux = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        flux.write_all(b"GET /cb?code=CODESECRET&state=ETATSECRET HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut reponse = String::new();
        let _ = flux.read_to_string(&mut reponse);
        let _ = fil.join();

        assert!(
            !reponse.contains("CODESECRET"),
            "le code a fuite :\n{reponse}"
        );
        assert!(
            !reponse.contains("ETATSECRET"),
            "l etat a fuite :\n{reponse}"
        );
    }
}
