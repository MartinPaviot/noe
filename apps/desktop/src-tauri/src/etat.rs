//! La machine à états du bornage (spec 002, R1.1, R1.2, R5.1).
//!
//! Tout ce qui décide s'il y a capture ou non vit ici, et rien d'autre n'a le
//! droit d'en décider. Le module ignore Tauri, l'horloge réelle et le disque :
//! il se teste intégralement en CI, sur n'importe quelle plateforme, sans écran.
//!
//! R1.2 — « SI aucun épisode n'est ouvert, ALORS LE SYSTÈME NE DOIT capturer
//! aucun événement N3, d'aucune sorte » — n'est pas une consigne défensive
//! ajoutée après coup : c'est le type qui la porte. On ne peut pas obtenir de
//! référence d'épisode sans en avoir ouvert un.

/// Les trois états visibles de l'icône (R5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatTray {
    /// Un épisode est ouvert et la capture court.
    Observe,
    /// Rien ne se capture — soit aucun épisode, soit pause explicite.
    Pause,
    /// Une question attend l'opérateur.
    Question,
}

impl EtatTray {
    /// Le nom du fichier d'icône correspondant.
    pub fn icone(self) -> &'static str {
        match self {
            Self::Observe => "tray-observe.png",
            Self::Pause => "tray-pause.png",
            Self::Question => "tray-question.png",
        }
    }

    /// Les octets du PNG, embarqués dans le binaire.
    ///
    /// Pas de lecture disque : `resource_dir()` ne désigne pas le même endroit
    /// en développement et une fois empaqueté, si bien qu'une icône introuvable
    /// laisserait le tray muet SANS que rien n'échoue — exactement le genre de
    /// panne qu'on ne voit qu'en production. Trois PNG de 280 octets ne
    /// justifient pas ce risque.
    pub fn png(self) -> &'static [u8] {
        match self {
            Self::Observe => include_bytes!("../icons/tray-observe.png"),
            Self::Pause => include_bytes!("../icons/tray-pause.png"),
            Self::Question => include_bytes!("../icons/tray-question.png"),
        }
    }

    pub fn infobulle(self) -> &'static str {
        match self {
            Self::Observe => "Noe — observe",
            Self::Pause => "Noe — en pause, rien n'est capture",
            Self::Question => "Noe — une question attend",
        }
    }
}

/// Pourquoi une demande de bornage a été refusée.
///
/// Un refus est toujours nommé et toujours notifié : un hotkey qui ne fait rien
/// sans le dire laisse croire à une capture qui n'a pas lieu, ce qui est le pire
/// des deux mondes — ni la donnée, ni la confiance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refus {
    /// R1.1 : le hotkey de début sans tâche choisie.
    AucuneTacheActive,
    /// Deuxième appui sur « début » alors qu'un épisode court déjà.
    EpisodeDejaOuvert,
    /// « Fin » sans rien à clore.
    AucunEpisodeOuvert,
}

impl Refus {
    /// Le message montré à l'opérateur. Il dit quoi faire, pas ce qui a échoué.
    pub fn message(self) -> &'static str {
        match self {
            Self::AucuneTacheActive => "Choisir une tache active avant de demarrer.",
            Self::EpisodeDejaOuvert => "Un episode est deja ouvert. Le hotkey de fin le clot.",
            Self::AucunEpisodeOuvert => "Aucun episode ouvert.",
        }
    }
}

/// L'épisode en cours. N'existe que si la capture a le droit de tourner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeOuvert {
    pub id: String,
    pub task_slug: String,
    /// R1.1 : la borne d'ouverture, en millisecondes depuis l'epoch Unix.
    ///
    /// Murale et non monotone, a dessein : un episode doit pouvoir etre remis en
    /// face d'un courriel ou d'un enregistrement CRM, ce qu'un compteur interne
    /// au processus ne permet pas.
    pub t0_ms: u64,
}

#[derive(Debug, Default)]
pub struct Session {
    episode: Option<EpisodeOuvert>,
    en_pause: bool,
    question_en_attente: bool,
}

impl Session {
    pub fn nouvelle() -> Self {
        Self::default()
    }

    /// L'unique porte d'entrée de la capture (R1.1).
    ///
    /// `tache_active` vient de la configuration ; `id` est fourni par l'appelant
    /// pour que le générateur d'ULID reste injectable et les tests
    /// déterministes.
    pub fn demarrer(
        &mut self,
        tache_active: Option<&str>,
        t0_ms: u64,
        id: impl FnOnce() -> String,
    ) -> Result<&EpisodeOuvert, Refus> {
        if self.episode.is_some() {
            return Err(Refus::EpisodeDejaOuvert);
        }
        let slug = tache_active.ok_or(Refus::AucuneTacheActive)?;

        // Démarrer lève la pause : sans ça, un opérateur qui a mis en pause la
        // veille croirait capturer alors que rien ne passe.
        self.en_pause = false;
        self.episode = Some(EpisodeOuvert {
            id: id(),
            task_slug: slug.to_string(),
            t0_ms,
        });
        Ok(self.episode.as_ref().expect("vient d etre pose"))
    }

    /// Clôt l'épisode et le rend à l'appelant, qui l'assemble et le valide.
    pub fn arreter(&mut self) -> Result<EpisodeOuvert, Refus> {
        self.episode.take().ok_or(Refus::AucunEpisodeOuvert)
    }

    /// Bascule la pause. Rend le nouvel état.
    pub fn basculer_pause(&mut self) -> bool {
        self.en_pause = !self.en_pause;
        self.en_pause
    }

    /// Une question attend l'opérateur — troisième état du tray (R5.1 de la 002).
    ///
    /// Son premier usage réel arrive avec la spec 003 : `reauth_required`. Un
    /// connecteur dont le jeton ne se rafraîchit plus ne doit ni crasher ni se
    /// taire — il demande, et l'icône le dit. La boucle de la spec 004 s'en
    /// servira aussi pour sa file priorisée.
    ///
    /// L'`allow(dead_code)` qui gardait ces deux méthodes est retiré : elles ont
    /// un appelant.
    pub fn poser_question(&mut self) {
        self.question_en_attente = true;
    }

    pub fn repondre_question(&mut self) {
        self.question_en_attente = false;
    }

    /// L'épisode courant, SI la capture a effectivement le droit d'écrire.
    ///
    /// C'est le seul accès. En pause, il rend `None` : le pipeline de capture ne
    /// peut donc pas écrire par inadvertance, la pause est étanche par
    /// construction et pas par vigilance.
    pub fn episode_capturable(&self) -> Option<&EpisodeOuvert> {
        if self.en_pause {
            return None;
        }
        self.episode.as_ref()
    }

    /// L'épisode ouvert, pause comprise — pour l'affichage et la clôture.
    pub fn episode(&self) -> Option<&EpisodeOuvert> {
        self.episode.as_ref()
    }

    pub fn en_pause(&self) -> bool {
        self.en_pause
    }

    /// La question l'emporte sur tout : c'est celui des trois états qui demande
    /// une action de l'opérateur, donc celui qu'il doit voir.
    pub fn etat_tray(&self) -> EtatTray {
        if self.question_en_attente {
            return EtatTray::Question;
        }
        match self.episode_capturable() {
            Some(_) => EtatTray::Observe,
            None => EtatTray::Pause,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-01-01T00:00:00Z, comme l'horloge simulee.
    const T0: u64 = 1_767_225_600_000;

    fn id_fixe() -> String {
        "01JGXAAAAAAAAAAAAAAAAAAAAA".to_string()
    }

    #[test]
    fn sans_tache_active_le_demarrage_est_refuse() {
        let mut s = Session::nouvelle();
        assert_eq!(
            s.demarrer(None, T0, id_fixe).unwrap_err(),
            Refus::AucuneTacheActive
        );
        assert!(s.episode().is_none(), "rien ne doit avoir ete ouvert");
        assert!(s.episode_capturable().is_none(), "R1.2 : aucune capture");
    }

    #[test]
    fn le_refus_porte_un_message_actionnable() {
        assert!(Refus::AucuneTacheActive.message().contains("Choisir"));
    }

    #[test]
    fn avec_une_tache_active_l_episode_s_ouvre_et_porte_le_slug() {
        let mut s = Session::nouvelle();
        let ep = s
            .demarrer(Some("maj-crm-post-echange"), T0, id_fixe)
            .unwrap();
        assert_eq!(ep.task_slug, "maj-crm-post-echange");
        assert_eq!(ep.id, id_fixe());
        assert_eq!(ep.t0_ms, T0, "R1.1 : l episode porte sa borne d ouverture");
        assert_eq!(s.etat_tray(), EtatTray::Observe);
    }

    #[test]
    fn deux_demarrages_de_suite_sont_refuses() {
        let mut s = Session::nouvelle();
        s.demarrer(Some("a-faire"), T0, id_fixe).unwrap();
        assert_eq!(
            s.demarrer(Some("a-faire"), T0, id_fixe).unwrap_err(),
            Refus::EpisodeDejaOuvert
        );
    }

    #[test]
    fn arreter_sans_episode_est_refuse() {
        let mut s = Session::nouvelle();
        assert_eq!(s.arreter().unwrap_err(), Refus::AucunEpisodeOuvert);
    }

    #[test]
    fn arreter_rend_l_episode_et_ferme_la_capture() {
        let mut s = Session::nouvelle();
        s.demarrer(Some("a-faire"), T0, id_fixe).unwrap();
        let ep = s.arreter().unwrap();
        assert_eq!(ep.task_slug, "a-faire");
        assert!(s.episode_capturable().is_none(), "R1.2 apres cloture");
        assert_eq!(s.etat_tray(), EtatTray::Pause);
    }

    #[test]
    fn la_pause_rend_l_episode_incapturable_sans_le_fermer() {
        let mut s = Session::nouvelle();
        s.demarrer(Some("a-faire"), T0, id_fixe).unwrap();

        assert!(s.basculer_pause());
        assert!(
            s.episode_capturable().is_none(),
            "R5.2 : la pause est etanche par construction"
        );
        assert!(s.episode().is_some(), "mais l episode reste ouvert");
        assert_eq!(s.etat_tray(), EtatTray::Pause);

        assert!(!s.basculer_pause());
        assert!(s.episode_capturable().is_some());
        assert_eq!(s.etat_tray(), EtatTray::Observe);
    }

    #[test]
    fn demarrer_leve_une_pause_heritee() {
        let mut s = Session::nouvelle();
        s.basculer_pause();
        s.demarrer(Some("a-faire"), T0, id_fixe).unwrap();
        assert!(
            s.episode_capturable().is_some(),
            "sinon on croirait capturer sans que rien ne passe"
        );
    }

    #[test]
    fn la_question_l_emporte_sur_les_deux_autres_etats() {
        let mut s = Session::nouvelle();
        s.demarrer(Some("a-faire"), T0, id_fixe).unwrap();
        s.poser_question();
        assert_eq!(s.etat_tray(), EtatTray::Question);
        s.repondre_question();
        assert_eq!(s.etat_tray(), EtatTray::Observe);
    }

    #[test]
    fn les_png_embarques_sont_valides_et_distincts() {
        let signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let etats = [EtatTray::Observe, EtatTray::Pause, EtatTray::Question];
        for e in etats {
            let octets = e.png();
            assert!(octets.len() > 100, "{e:?} : PNG suspect");
            assert_eq!(octets[..8], signature, "{e:?} : ce n est pas un PNG");
        }
        let uniques: std::collections::BTreeSet<_> = etats.iter().map(|e| e.png()).collect();
        assert_eq!(uniques.len(), 3, "les trois icones doivent differer");
    }

    #[test]
    fn les_trois_etats_ont_trois_icones_distinctes() {
        let icones = [
            EtatTray::Observe.icone(),
            EtatTray::Pause.icone(),
            EtatTray::Question.icone(),
        ];
        let uniques: std::collections::BTreeSet<_> = icones.iter().collect();
        assert_eq!(uniques.len(), 3, "R5.1 exige trois etats DISTINCTS");
    }
}
