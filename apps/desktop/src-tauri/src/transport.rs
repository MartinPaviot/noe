//! Le transport HTTP des connecteurs (spec 003, R5.2 et R5.3).
//!
//! Les deux adaptateurs — CRM et Gmail — isolent leur appel réseau derrière le
//! trait `Transport`, ce qui les rend vérifiables sans org. Ce module est
//! l'implémentation qui parle vraiment. Elle **ne dépend d'aucune org** non plus :
//! tout ce qu'elle fait se prouve contre un serveur de boucle locale.
//!
//! ## Ce qu'un transport doit refuser
//!
//! Un client HTTP qui porte un jeton de porteur n'est pas un client HTTP
//! ordinaire. Le jeton part avec **chaque** requête, et il suffit qu'il parte une
//! fois au mauvais endroit pour donner à quelqu'un d'autre la lecture du CRM.
//! D'où quatre refus, et chacun ferme une porte connue :
//!
//! 1. **Rien en clair.** `http://` enverrait le jeton lisible sur le réseau.
//! 2. **Aucune redirection suivie.** Un `302` vers un autre hôte y emporterait
//!    l'en-tête `Authorization` — c'est la fuite classique, et elle est
//!    silencieuse parce que la requête *réussit*.
//! 3. **Aucun identifiant dans l'URL.** `https://login.salesforce.com@ailleurs`
//!    a l'air de pointer sur Salesforce ; l'hôte réel est `ailleurs`.
//! 4. **L'hôte doit tomber dans la liste blanche, sur une frontière de point.**
//!    L'URL d'instance d'un CRM arrive dans la réponse de jeton, c'est-à-dire du
//!    réseau : la croire sans la vérifier laisserait la réponse choisir où notre
//!    jeton s'en va. Et `evilsalesforce.com` finit par `salesforce.com` sans être
//!    Salesforce.
//!
//! ## Les bornes
//!
//! Un délai global, parce que R3.2 refuse qu'une clôture attende plus de soixante
//! secondes et qu'un serveur muet attendrait sinon pour toujours. Et un plafond
//! de taille de réponse, parce que R7.1 donne à l'application un budget mémoire
//! de 200 Mo et qu'une réponse sans borne le mange.
//!
//! ## Pas encore appelé
//!
//! Personne ne construit encore de `ClientHttp` : il manque un jeton, donc une
//! application connectée, donc la tâche 0. Tout ce qui est ici se prouve quand
//! même — contre un serveur de boucle locale, et sans org.
#![allow(dead_code)] // retiré quand la tâche 0 rend l'échange OAuth possible

use std::time::Duration;

/// Ce qu'une réponse rapporte.
///
/// **`retry_after_ms` est un champ et pas un détail.** R5.1 exige de respecter
/// `Retry-After`, et on ne peut pas respecter un en-tête qu'on ne lit pas. Une
/// signature qui ne rendait que le statut et le corps rendait cette exigence
/// inatteignable sans qu'aucun test ne le dise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReponseHttp {
    pub statut: u16,
    pub corps: String,
    /// `Retry-After`, quand le serveur en donne un et qu'il est en secondes.
    ///
    /// La RFC autorise aussi une **date**. On ne la lit pas, et on ne fait pas
    /// semblant : sans valeur, le client retombe sur son propre calcul, qui est
    /// borné. Mal analyser une date donnerait une attente fausse, ce qui est pire
    /// que pas d'attente du tout.
    pub retry_after_ms: Option<u64>,
}

impl ReponseHttp {
    /// Une réponse sans `Retry-After`. Sert surtout aux bancs.
    pub fn simple(statut: u16, corps: impl Into<String>) -> Self {
        Self {
            statut,
            corps: corps.into(),
            retry_after_ms: None,
        }
    }
}

/// Ce qu'un adaptateur sait demander au réseau.
///
/// **Aucune écriture.** Le trait ne connaît que `get` : la promotion appartient à
/// une spec ultérieure, et une méthode exposée « pour plus tard » finit
/// implémentée puis appelée.
pub trait Transport: Send + Sync {
    /// Un GET authentifié sur un chemin relatif à la base.
    fn get(&self, chemin: &str) -> Result<ReponseHttp, String>;
}

/// Lit un `Retry-After` en secondes. Rend `None` pour la forme datée.
pub fn retry_after_ms(brut: Option<&str>) -> Option<u64> {
    brut?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|s| s.saturating_mul(1000))
}

/// Le délai global d'un appel.
///
/// Quinze secondes, et pas soixante : R3.2 borne la **clôture** à soixante
/// secondes, or une clôture lit plusieurs entités et le client robuste réessaie.
/// Un appel qui s'autoriserait tout le budget le prendrait à tous les autres.
pub const DELAI_GLOBAL: Duration = Duration::from_secs(15);

/// Le délai d'établissement de connexion.
pub const DELAI_CONNEXION: Duration = Duration::from_secs(5);

/// Le plafond de taille d'une réponse.
///
/// Une réponse plus grosse est **refusée**, pas tronquée : un JSON coupé au
/// milieu produirait une erreur d'analyse qui parlerait de syntaxe, et personne
/// ne remonterait de là au vrai problème.
pub const PLAFOND_REPONSE: u64 = 8 * 1024 * 1024;

/// Où le jeton a le droit d'aller.
#[derive(Debug, Clone)]
pub struct Politique {
    /// Les suffixes d'hôte admis, en minuscules. Un hôte doit être égal à l'un
    /// d'eux ou s'y terminer **après un point**.
    pub suffixes: Vec<String>,
    /// Autorise `http://` vers la boucle locale.
    ///
    /// **Faux en production**, et un test le vérifie sur le constructeur. Ce
    /// n'est ouvert que pour les tests de ce module, qui doivent bien parler à
    /// quelque chose.
    pub clair_en_boucle_locale: bool,
}

impl Politique {
    /// Les hôtes d'une org Salesforce.
    ///
    /// `force.com` couvre les domaines personnalisés (`monorg.my.salesforce.com`
    /// est en `salesforce.com`, les sites en `force.com`).
    pub fn salesforce() -> Self {
        Self {
            suffixes: vec!["salesforce.com".into(), "force.com".into()],
            clair_en_boucle_locale: false,
        }
    }

    /// Les hôtes de l'API Google.
    pub fn google() -> Self {
        Self {
            suffixes: vec!["googleapis.com".into(), "oauth2.googleapis.com".into()],
            clair_en_boucle_locale: false,
        }
    }
}

/// Vérifie qu'une URL a le droit de recevoir le jeton.
///
/// Une fonction pure, séparée de l'appel : c'est la partie qui doit être vraie
/// dans tous les cas tordus, et elle se teste sans réseau.
pub fn verifier_url(url: &str, politique: &Politique) -> Result<(), String> {
    let minuscule = url.to_lowercase();
    let (schema, reste) = match minuscule.split_once("://") {
        Some((s, r)) => (s, r),
        None => return Err("url sans schema".into()),
    };

    // L'autorité s'arrête au premier `/`, `?` ou `#`.
    let autorite = reste
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_owned();
    if autorite.is_empty() {
        return Err("url sans hote".into());
    }

    // Piège 3 : `https://login.salesforce.com@ailleurs.example/` a l'air de
    // pointer sur Salesforce. L'hôte réel est `ailleurs.example`. Plutôt que de
    // parser finement ce qui n'a aucun usage légitime ici, on refuse.
    if autorite.contains('@') {
        return Err("identifiants dans l url".into());
    }

    // Le port ne fait pas partie de l'hôte. Une adresse IPv6 littérale est entre
    // crochets et contient des `:` : le dernier `:` ne sépare un port que si ce
    // qui suit est un nombre **et** si ce qui précède est soit un nom sans `:`,
    // soit une adresse entre crochets fermés.
    //
    // La première écriture demandait l'inverse — que l'hôte NE finisse PAS par
    // `]` — ce qui est exactement le cas où le découpage est valable. Elle
    // laissait `[::1]:8080` entier, donc jamais reconnu comme boucle locale.
    // L'erreur était fermante et non ouvrante, mais elle était là.
    let hote = match autorite.rsplit_once(':') {
        Some((h, p))
            if !p.is_empty()
                && p.chars().all(|c| c.is_ascii_digit())
                && (h.ends_with(']') || !h.contains(':')) =>
        {
            h
        }
        _ => autorite.as_str(),
    };

    let boucle_locale = matches!(hote, "127.0.0.1" | "[::1]" | "localhost");
    match schema {
        "https" => {}
        "http" if politique.clair_en_boucle_locale && boucle_locale => {}
        // Piège 1 : en clair, le jeton de porteur est lisible par qui écoute.
        "http" => return Err("http en clair : le jeton partirait lisible".into()),
        autre => return Err(format!("schema refuse : {autre}")),
    }

    if politique.clair_en_boucle_locale && boucle_locale {
        return Ok(());
    }

    // Piège 4 : `evilsalesforce.com` finit par `salesforce.com`. Le suffixe doit
    // tomber sur une frontière de point.
    let admis = politique.suffixes.iter().any(|s| {
        hote == s
            || (hote.len() > s.len() + 1
                && hote.ends_with(s.as_str())
                && hote.as_bytes()[hote.len() - s.len() - 1] == b'.')
    });
    if admis {
        Ok(())
    } else {
        Err(format!("hote hors liste blanche : {hote}"))
    }
}

/// Un POST de formulaire — l'échange de jetons, et rien d'autre.
///
/// **Volontairement hors du trait `Transport`.** Le trait dit « aucune
/// écriture », et il parle des systèmes de vérité : y ajouter `post` rendrait
/// cette phrase fausse au premier coup d'œil, et un adaptateur pourrait s'en
/// servir. Demander un jeton n'écrit rien chez personne.
///
/// Le corps porte le code d'autorisation et le vérificateur PKCE. Il n'est
/// **jamais** remis dans un message d'erreur : une erreur se lit dans un journal,
/// et un journal se partage.
pub fn poster_formulaire(
    url: &str,
    champs: &[(&str, String)],
    politique: &Politique,
    delai: Duration,
) -> Result<(u16, String), String> {
    verifier_url(url, politique)?;
    let corps = champs
        .iter()
        .map(|(nom, valeur)| {
            format!(
                "{}={}",
                crate::oauth::encoder_composant(nom),
                crate::oauth::encoder_composant(valeur)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(delai))
        .timeout_connect(Some(DELAI_CONNEXION))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .new_agent();

    let reponse = agent
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .send(&corps)
        // Surtout pas `{corps}` : ce message finirait dans un journal.
        .map_err(|e| format!("echange refuse : {e}"))?;
    let statut = reponse.status().as_u16();
    if (300..400).contains(&statut) {
        return Err(format!("redirection {statut} non suivie"));
    }
    let recu = reponse
        .into_body()
        .with_config()
        .limit(PLAFOND_REPONSE)
        .read_to_string()
        .map_err(|e| format!("corps illisible : {e}"))?;
    Ok((statut, recu))
}

/// Le client HTTP réel.
pub struct ClientHttp {
    agent: ureq::Agent,
    base: String,
    jeton: String,
    politique: Politique,
}

/// Le `Debug` est écrit à la main : un jeton de porteur imprimé dans une trace
/// est un jeton publié. Même règle que le vérificateur PKCE d'`oauth.rs`.
impl std::fmt::Debug for ClientHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientHttp")
            .field("base", &self.base)
            .field("jeton", &"[masque]")
            .field("politique", &self.politique)
            .finish()
    }
}

impl ClientHttp {
    /// Construit un client pour une base et un jeton.
    ///
    /// La base est vérifiée **ici**, à la construction : une base invalide doit
    /// se voir au moment où on la reçoit, pas au premier appel.
    pub fn nouveau(base: &str, jeton: &str, politique: Politique) -> Result<Self, String> {
        Self::avec_delai(base, jeton, politique, DELAI_GLOBAL)
    }

    /// Le même, avec un délai global choisi.
    ///
    /// Existe pour les tests : prouver qu'un serveur muet ne bloque pas demande
    /// d'attendre le délai, et attendre quinze secondes dans une suite de tests
    /// la rendrait assez pénible pour qu'on cesse de la lancer.
    pub fn avec_delai(
        base: &str,
        jeton: &str,
        politique: Politique,
        delai: Duration,
    ) -> Result<Self, String> {
        verifier_url(base, &politique)?;
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(delai))
            .timeout_connect(Some(DELAI_CONNEXION))
            // Piège 2 : une redirection vers un autre hôte y emporterait
            // l'en-tête `Authorization`, et la requête RÉUSSIRAIT.
            .max_redirects(0)
            // Un 404 n'est pas une panne de transport : c'est une réponse, et
            // c'est l'adaptateur qui sait ce qu'elle veut dire.
            .http_status_as_error(false)
            .build();
        Ok(Self {
            agent: config.new_agent(),
            base: base.trim_end_matches('/').to_owned(),
            jeton: jeton.to_owned(),
            politique,
        })
    }
}

impl Transport for ClientHttp {
    fn get(&self, chemin: &str) -> Result<ReponseHttp, String> {
        let url = format!("{}{chemin}", self.base);
        // Re-vérifiée à chaque appel : la base est sûre, mais un chemin qui
        // commencerait par `//` ou par un schéma changerait l'hôte.
        verifier_url(&url, &self.politique)?;

        let reponse = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.jeton))
            .header("Accept", "application/json")
            .call()
            .map_err(|e| format!("appel refuse : {e}"))?;

        let statut = reponse.status().as_u16();
        // Une redirection n'est pas suivie ; la rendre telle quelle laisserait
        // l'adaptateur croire à une réponse vide. On la nomme.
        if (300..400).contains(&statut) {
            return Err(format!("redirection {statut} non suivie"));
        }
        let attente = retry_after_ms(
            reponse
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
        );

        let corps = reponse
            .into_body()
            .with_config()
            .limit(PLAFOND_REPONSE)
            .read_to_string()
            .map_err(|e| format!("corps illisible ou au-dela de {PLAFOND_REPONSE} octets : {e}"))?;
        Ok(ReponseHttp {
            statut,
            corps,
            retry_after_ms: attente,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn sf() -> Politique {
        Politique::salesforce()
    }

    fn locale() -> Politique {
        Politique {
            suffixes: Vec::new(),
            clair_en_boucle_locale: true,
        }
    }

    // -- Ce que le transport refuse (fonction pure, sans reseau) -------------

    #[test]
    fn une_url_en_clair_est_refusee() {
        // Le jeton de porteur part avec chaque requete : en clair, il est lisible
        // par qui ecoute le reseau.
        let e = verifier_url("http://login.salesforce.com/x", &sf()).unwrap_err();
        assert!(e.contains("clair"), "{e}");
    }

    #[test]
    fn un_hote_hors_liste_est_refuse() {
        let e = verifier_url("https://ailleurs.example/x", &sf()).unwrap_err();
        assert!(e.contains("liste blanche"), "{e}");
    }

    #[test]
    fn le_suffixe_doit_tomber_sur_un_point() {
        // `evilsalesforce.com` finit bien par `salesforce.com`. Un `ends_with`
        // nu enverrait le jeton chez lui.
        assert!(verifier_url("https://evilsalesforce.com/x", &sf()).is_err());
        assert!(verifier_url("https://monorg.my.salesforce.com/x", &sf()).is_ok());
        assert!(verifier_url("https://salesforce.com/x", &sf()).is_ok());
        assert!(verifier_url("https://x.force.com/y", &sf()).is_ok());
    }

    #[test]
    fn des_identifiants_dans_l_url_sont_refuses() {
        // `https://login.salesforce.com@ailleurs.example/` a l'air de pointer sur
        // Salesforce. L'hote reel est `ailleurs.example`.
        let e = verifier_url("https://login.salesforce.com@ailleurs.example/x", &sf()).unwrap_err();
        assert!(e.contains("identifiants"), "{e}");
    }

    #[test]
    fn la_casse_ne_contourne_pas_la_liste_blanche() {
        assert!(verifier_url("HTTPS://MONORG.MY.SALESFORCE.COM/x", &sf()).is_ok());
        assert!(verifier_url("HTTPS://AILLEURS.EXAMPLE/x", &sf()).is_err());
    }

    #[test]
    fn un_port_ne_change_pas_l_hote() {
        assert!(verifier_url("https://monorg.my.salesforce.com:8443/x", &sf()).is_ok());
        assert!(verifier_url("https://ailleurs.example:443/x", &sf()).is_err());
    }

    #[test]
    fn une_adresse_ipv6_locale_garde_son_hote() {
        // La garde du port etait ecrite a l'envers : elle laissait `[::1]:8080`
        // entier, donc jamais reconnu comme boucle locale.
        assert!(verifier_url("http://[::1]:8080/cb", &locale()).is_ok());
        assert!(verifier_url("http://[::1]/cb", &locale()).is_ok());
        // Et une IPv6 quelconque ne devient pas Salesforce pour autant.
        assert!(verifier_url("https://[2001:db8::1]:443/x", &sf()).is_err());
    }

    #[test]
    fn un_deux_points_sans_port_ne_mange_pas_l_hote() {
        assert!(verifier_url("https://monorg.my.salesforce.com:/x", &sf()).is_err());
        assert!(verifier_url("https://monorg.my.salesforce.com:abc/x", &sf()).is_err());
    }

    #[test]
    fn une_url_sans_schema_ou_sans_hote_est_refusee() {
        assert!(verifier_url("monorg.my.salesforce.com/x", &sf()).is_err());
        assert!(verifier_url("file:///c:/secrets", &sf()).is_err());
        assert!(verifier_url("https:///x", &sf()).is_err());
    }

    #[test]
    fn la_production_ne_parle_jamais_en_clair() {
        // Le confort des tests ne doit pas devenir une porte en production : les
        // deux politiques reelles ferment la boucle locale en clair.
        for p in [Politique::salesforce(), Politique::google()] {
            assert!(!p.clair_en_boucle_locale, "{p:?}");
            assert!(verifier_url("http://127.0.0.1:9/x", &p).is_err());
        }
    }

    #[test]
    fn une_base_invalide_est_refusee_a_la_construction() {
        // Une base invalide doit se voir au moment ou on la recoit — l'URL
        // d'instance vient de la reponse de jeton, donc du reseau.
        assert!(ClientHttp::nouveau("https://ailleurs.example", "J", sf()).is_err());
    }

    // -- Les appels reels, contre un serveur de boucle locale ----------------

    /// Un serveur minuscule : il sert `nb` connexions, rend `reponse`, et
    /// raconte ce qu'il a recu.
    fn servir(reponse: Vec<u8>, nb: usize) -> (u16, std::sync::mpsc::Receiver<String>) {
        let ecouteur = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = ecouteur.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..nb {
                let Ok((mut flux, _)) = ecouteur.accept() else {
                    return;
                };
                let mut tete = Vec::new();
                let mut octet = [0_u8; 1];
                while flux.read_exact(&mut octet).is_ok() {
                    tete.push(octet[0]);
                    if tete.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                // Le corps, quand il y en a un : sans lui, on ne peut rien
                // verifier de ce qu'un POST envoie.
                let entete = String::from_utf8_lossy(&tete).to_lowercase();
                let taille = entete
                    .split("content-length:")
                    .nth(1)
                    .and_then(|r| r.split(['\r', '\n']).next())
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let mut corps_recu = vec![0_u8; taille];
                if taille > 0 {
                    let _ = flux.read_exact(&mut corps_recu);
                }
                let _ = tx.send(format!(
                    "{}{}",
                    String::from_utf8_lossy(&tete),
                    String::from_utf8_lossy(&corps_recu)
                ));
                let _ = flux.write_all(&reponse);
                let _ = flux.flush();
            }
        });
        (port, rx)
    }

    fn reponse(statut: &str, corps: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {statut}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{corps}",
            corps.len()
        )
        .into_bytes()
    }

    /// Le delai global des bancs. Court, pour que la suite reste rapide.
    const DELAI_BANC: Duration = Duration::from_millis(800);

    fn client(port: u16) -> ClientHttp {
        ClientHttp::avec_delai(
            &format!("http://127.0.0.1:{port}"),
            "LEJETON",
            locale(),
            DELAI_BANC,
        )
        .unwrap()
    }

    #[test]
    fn le_statut_et_le_corps_remontent() {
        let (port, _rx) = servir(reponse("404 Not Found", "{\"e\":1}"), 1);
        // Un 404 n'est pas une panne de transport : c'est une reponse, et c'est
        // l'adaptateur qui sait ce qu'elle veut dire.
        let r = client(port).get("/x").unwrap();
        assert_eq!(r.statut, 404);
        assert_eq!(r.corps, "{\"e\":1}");
    }

    #[test]
    fn le_jeton_part_en_porteur_et_l_appel_demande_du_json() {
        let (port, rx) = servir(reponse("200 OK", "{}"), 1);
        client(port).get("/x").unwrap();
        let tete = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(tete.contains("authorization: Bearer LEJETON"), "{tete}");
        assert!(
            tete.to_lowercase().contains("accept: application/json"),
            "{tete}"
        );
    }

    #[test]
    fn un_retry_after_en_secondes_remonte_jusqu_au_client() {
        // R5.1 : on ne peut pas respecter un en-tete qu'on ne lit pas.
        let mut r = reponse("429 Too Many Requests", "{}");
        let entete = b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 12\r\n";
        r.splice(
            0..b"HTTP/1.1 429 Too Many Requests\r\n".len(),
            entete.iter().copied(),
        );
        let (port, _rx) = servir(r, 1);
        let recu = client(port).get("/x").unwrap();
        assert_eq!(recu.statut, 429);
        assert_eq!(recu.retry_after_ms, Some(12_000));
    }

    #[test]
    fn un_retry_after_date_ne_devient_pas_une_attente_fausse() {
        // La RFC autorise une date. On ne la lit pas, et sans valeur le client
        // retombe sur son propre calcul, qui est borne. Mal analyser une date
        // donnerait une attente fausse, ce qui est pire que pas d'attente.
        assert_eq!(retry_after_ms(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after_ms(Some("12")), Some(12_000));
        assert_eq!(retry_after_ms(Some("  30 ")), Some(30_000));
        assert_eq!(retry_after_ms(Some("")), None);
        assert_eq!(retry_after_ms(None), None);
    }

    #[test]
    fn une_redirection_n_est_pas_suivie() {
        // La fuite classique : un 302 vers un autre hote y emporte l'en-tete
        // `Authorization`, et la requete REUSSIT — donc personne ne le voit.
        let mut r = reponse("302 Found", "");
        let tete = b"HTTP/1.1 302 Found\r\nLocation: https://ailleurs.example/vol\r\n";
        r.splice(0..b"HTTP/1.1 302 Found\r\n".len(), tete.iter().copied());
        let (port, _rx) = servir(r, 1);
        let e = client(port).get("/x").unwrap_err();
        assert!(e.contains("302") || e.contains("redirection"), "{e}");
    }

    #[test]
    fn un_serveur_muet_ne_bloque_pas_pour_toujours() {
        // Il accepte la connexion et ne repond jamais. Sans delai global, la
        // cloture attendrait au-dela des soixante secondes que R3.2 borne.
        let ecouteur = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = ecouteur.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let garde = ecouteur.accept();
            std::thread::sleep(Duration::from_secs(10));
            drop(garde);
        });
        let debut = std::time::Instant::now();
        assert!(
            client(port).get("/x").is_err(),
            "l appel n a pas rendu la main"
        );

        // **La borne est large, et c'est voulu.** La propriete eprouvee est « ca
        // rend la main », pas « ca rend la main en cinq secondes » : sans delai
        // global, le serveur muet tient la connexion et l'appel ne revient
        // jamais. Une borne serree mesurerait la charge de la machine — ce banc
        // a echoue trois fois sur quatre pendant une compilation concurrente,
        // avec un client regle sur huit cents millisecondes.
        //
        // Vingt fois le delai configure distingue encore un delai absent d'un
        // delai present, sans transformer un test de comportement en test de
        // vitesse.
        let borne = DELAI_BANC * 20;
        assert!(
            debut.elapsed() < borne,
            "rendu en {:?}, au-dela de {borne:?}",
            debut.elapsed()
        );
    }

    #[test]
    fn un_corps_au_dela_du_plafond_est_refuse() {
        // Refuse et pas tronque : un JSON coupe au milieu produirait une erreur
        // d'analyse qui parlerait de syntaxe, et personne ne remonterait de la au
        // vrai probleme.
        let gros = "a".repeat(PLAFOND_REPONSE as usize + 1024);
        let (port, _rx) = servir(reponse("200 OK", &gros), 1);
        let e = client(port).get("/x").unwrap_err();
        assert!(e.contains("corps"), "{e}");
    }

    #[test]
    fn un_chemin_ne_peut_pas_changer_d_hote() {
        // Le chemin arrive d'un adaptateur, donc de code ; mais un identifiant
        // interpole suffirait. L'URL complete est re-verifiee a chaque appel.
        let c = ClientHttp::nouveau("https://monorg.my.salesforce.com", "J", sf()).unwrap();
        let e = c.get("@ailleurs.example/x").unwrap_err();
        assert!(e.contains("identifiants"), "{e}");
    }

    #[test]
    fn un_post_de_formulaire_encode_et_envoie_son_corps() {
        let (port, rx) = servir(reponse("200 OK", "{}"), 1);
        let champs = vec![
            ("grant_type", "authorization_code".to_owned()),
            ("code", "LE CODE/+".to_owned()),
        ];
        let (statut, _) = poster_formulaire(
            &format!("http://127.0.0.1:{port}/token"),
            &champs,
            &locale(),
            Duration::from_millis(800),
        )
        .unwrap();
        assert_eq!(statut, 200);
        let vu = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(vu.to_lowercase().contains("post /token"), "{vu}");
        assert!(
            vu.to_lowercase()
                .contains("content-type: application/x-www-form-urlencoded"),
            "{vu}"
        );
        // Les caracteres reserves sont encodes : un `+` non encode se relit
        // comme une espace cote serveur, et le code ne correspond plus a rien.
        assert!(vu.contains("code=LE%20CODE%2F%2B"), "{vu}");
    }

    #[test]
    fn un_echange_qui_echoue_ne_recrache_pas_son_corps() {
        // Le corps porte le code d'autorisation et le verificateur PKCE. Ce
        // message finit dans un journal, et un journal se partage.
        let ecouteur = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = ecouteur.local_addr().unwrap().port();
        drop(ecouteur); // plus personne n'ecoute : l'appel echoue.
        let champs = vec![
            ("code", "CODESECRET".to_owned()),
            ("code_verifier", "VERIFSECRET".to_owned()),
        ];
        let e = poster_formulaire(
            &format!("http://127.0.0.1:{port}/token"),
            &champs,
            &locale(),
            Duration::from_millis(500),
        )
        .unwrap_err();
        assert!(!e.contains("CODESECRET"), "{e}");
        assert!(!e.contains("VERIFSECRET"), "{e}");
    }

    #[test]
    fn un_post_hors_liste_blanche_est_refuse_avant_de_partir() {
        let e = poster_formulaire(
            "https://collecteur.example/token",
            &[],
            &sf(),
            Duration::from_millis(500),
        )
        .unwrap_err();
        assert!(e.contains("liste blanche"), "{e}");
    }

    #[test]
    fn le_jeton_ne_s_imprime_pas() {
        // Un jeton de porteur dans une trace est un jeton publie.
        let c = ClientHttp::nouveau("https://monorg.my.salesforce.com", "SECRET", sf()).unwrap();
        let trace = format!("{c:?}");
        assert!(!trace.contains("SECRET"), "{trace}");
        assert!(trace.contains("masque"), "{trace}");
    }
}
