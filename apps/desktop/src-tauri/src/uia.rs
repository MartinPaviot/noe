//! L'adaptateur de capture des applications natives (spec 002, tâches 6a, 7).
//!
//! Stratégie **globale filtrée**, fixée par le verdict du spike (design §2 a) :
//! un abonnement unique sur le bureau entier, filtré à quelques types
//! d'événements. C'est la seule des deux stratégies mesurées qui tienne le
//! budget CPU — 3,16 % contre 8,48 % pour l'abonnement par conteneur au focus.
//!
//! Le fichier sépare deux choses qui n'ont pas la même testabilité :
//!
//! - **le vocabulaire** (types d'événements, rôles, résolution) est pur, et se
//!   teste intégralement en CI ;
//! - **l'abonnement et la photographie** exigent un bureau Windows et ne se
//!   vérifient qu'en session.
//!
//! Tous les appels UIA vivent sur **un seul thread**, celui que `abonner` lance.
//! L'API est à cloisonnement de thread : un `UIElement` touché depuis un autre
//! thread se solde par une erreur COM difficile à diagnostiquer, et le spike y a
//! déjà perdu une itération. C'est pourquoi les demandes de photo transitent par
//! un canal au lieu d'être servies sur place.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::snapshot::Noeud;
use crate::source::{Abonnement, CaptureSource, Cible, ErreurSource, GenreEvenement, RawEvent};
// `Source` n'apparaît que dans la boucle réelle, absente du binaire de test.
#[cfg(not(test))]
use crate::source::Source;

/// Les événements auxquels on s'abonne, traduits dans notre vocabulaire.
///
/// La liste est **fermée et courte**, et c'est le filtre qui rend la stratégie
/// globale tenable : s'abonner à tout ce qu'UIA émet sur un bureau entier
/// noierait le budget CPU sous des changements de propriété sans intérêt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvenementUia {
    /// Un bouton, un lien, un élément de menu a été actionné.
    Invocation,
    /// Le texte d'un champ a changé.
    TexteChange,
    /// Un élément d'une liste ou d'un combo a été choisi.
    SelectionChangee,
    /// Le conteneur s'est réorganisé.
    StructureChangee,
    /// Le focus a bougé.
    Focus,
}

impl EvenementUia {
    pub fn genre(self, cible: Cible) -> GenreEvenement {
        match self {
            Self::Invocation => GenreEvenement::Invocation(cible),
            Self::TexteChange => GenreEvenement::Saisie(cible),
            Self::SelectionChangee => GenreEvenement::ChangementValeur(cible),
            Self::StructureChangee => GenreEvenement::ChangementStructure(cible),
            Self::Focus => GenreEvenement::Focus(cible),
        }
    }
}

/// Traduit un type de contrôle UIA dans le vocabulaire ARIA.
///
/// **C'est ce qui rend les deux sources comparables.** UIA nomme ses contrôles
/// `Button`, `Hyperlink`, `Edit` ; le DOM parle `button`, `link`, `textbox`. Sans
/// cette traduction, le même contrôle logique produirait deux ancrages
/// différents selon la surface, et la promesse de D19 — « les deux sources
/// produisent le MÊME `RawEvent` » — serait vraie sur la forme et fausse sur le
/// fond : les clés de branches de la spec 004 ne se rejoindraient jamais.
///
/// La table est volontairement incomplète et le défaut est `generic`, comme côté
/// DOM : mieux vaut un rôle honnêtement vague qu'un rôle inventé.
pub fn role_normalise(type_controle: &str) -> String {
    match type_controle {
        "Button" | "SplitButton" => "button",
        "Hyperlink" => "link",
        "Edit" => "textbox",
        "ComboBox" => "combobox",
        "CheckBox" => "checkbox",
        "RadioButton" => "radio",
        "TabItem" => "tab",
        "Tab" => "tablist",
        "MenuItem" => "menuitem",
        "List" => "list",
        "ListItem" => "listitem",
        "Table" | "DataGrid" => "table",
        "DataItem" => "row",
        "HeaderItem" => "columnheader",
        "Document" => "document",
        "Image" => "img",
        "Text" => "text",
        "Slider" => "slider",
        "Spinner" => "spinbutton",
        "TreeItem" => "treeitem",
        "Window" => "window",
        // `Pane`, `Group`, `Custom`, `Inconnu` et tout le reste : le DOM les
        // aurait appelés `generic`, on fait pareil.
        _ => "generic",
    }
    .to_string()
}

/// Les rôles qui peuvent servir de **région** en remontant l'arbre.
///
/// Une région donne à l'ancrage son contexte : deux boutons « Enregistrer » dans
/// deux panneaux différents ne sont pas le même contrôle.
pub fn est_region(role: &str) -> bool {
    matches!(role, "generic" | "document" | "window" | "table" | "list")
}

/// Fabrique la cible, en appliquant R2.2 : rôle + nom + région, rien d'autre.
pub fn cible_de(type_controle: &str, nom: &str, region: Option<&str>) -> Cible {
    let mut c = Cible::new(&role_normalise(type_controle), nom.trim());
    if let Some(r) = region {
        let r = r.trim();
        if !r.is_empty() {
            c = c.dans(r);
        }
    }
    c
}

/// Une demande de photo : le canal par lequel la réponse doit revenir.
type Demande = Sender<Option<Noeud>>;

/// Au-delà, on renonce à la photo.
///
/// Un écran qui met une demi-seconde à se décrire est un écran en train de se
/// recharger : la photo serait fausse. Mieux vaut un déclencheur sans photo
/// qu'un moteur bloqué par un bureau qui ne répond plus.
const DELAI_PHOTO_MS: u64 = 500;

/// Le photographe natif (R2.3).
///
/// **Il ne regarde pas l'écran lui-même.** Le moteur appelle `photographier()`
/// depuis le thread qui traite les événements, pas depuis celui qui parle à UIA.
/// La demande transite donc par un canal jusqu'au thread de capture, qui la sert
/// et répond.
pub struct SnapshotteurUia {
    demandes: Sender<Demande>,
}

impl crate::moteur::Snapshotteur for SnapshotteurUia {
    fn photographier(&self) -> Option<Noeud> {
        let (repondre, reponse) = std::sync::mpsc::channel();
        self.demandes.send(repondre).ok()?;
        reponse
            .recv_timeout(std::time::Duration::from_millis(DELAI_PHOTO_MS))
            .ok()
            .flatten()
    }
}

/// La source native. Ne vit qu'avec un bureau Windows en face.
pub struct UiaSource {
    actif: Arc<AtomicBool>,
    demandes: Sender<Demande>,
    reception: Option<std::sync::mpsc::Receiver<Demande>>,
}

impl Default for UiaSource {
    fn default() -> Self {
        let (demandes, reception) = std::sync::mpsc::channel();
        Self {
            actif: Arc::new(AtomicBool::new(false)),
            demandes,
            reception: Some(reception),
        }
    }
}

impl UiaSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Le photographe branché sur cette source. À passer au moteur.
    pub fn snapshotteur(&self) -> SnapshotteurUia {
        SnapshotteurUia {
            demandes: self.demandes.clone(),
        }
    }
}

impl CaptureSource for UiaSource {
    fn abonner(&mut self, puits: Sender<RawEvent>) -> Result<Abonnement, ErreurSource> {
        if self.actif.load(Ordering::SeqCst) {
            return Err(ErreurSource::DejaAbonne);
        }
        let reception = self.reception.take().ok_or(ErreurSource::DejaAbonne)?;
        let actif = self.actif.clone();
        let abonnement = Abonnement::nouveau(actif.clone());

        // Un thread dédié, et un seul : voir la note d'en-tête sur le
        // cloisonnement de thread d'UIA.
        std::thread::Builder::new()
            .name("noe-uia".into())
            .spawn(move || boucle_uia(&puits, &actif, &reception))
            .map_err(|_| ErreurSource::DejaAbonne)?;

        Ok(abonnement)
    }
}

/// La boucle réelle. Compilée seulement hors test — elle exige un bureau.
#[cfg(not(test))]
fn boucle_uia(
    puits: &Sender<RawEvent>,
    actif: &Arc<AtomicBool>,
    demandes: &std::sync::mpsc::Receiver<Demande>,
) {
    use uiautomation::events::{
        CustomEventHandlerFn, CustomFocusChangedEventHandlerFn, UIEventHandler, UIEventType,
        UIFocusChangedEventHandler,
    };
    use uiautomation::types::TreeScope;
    use uiautomation::{UIAutomation, UIElement};

    let Ok(automation) = UIAutomation::new() else {
        eprintln!("[noe] UIA indisponible : aucune capture native");
        return;
    };
    let Ok(racine) = automation.get_root_element() else {
        eprintln!("[noe] racine UIA introuvable");
        return;
    };

    let debut = std::time::Instant::now();
    let abonnes: &[(UIEventType, EvenementUia)] = &[
        (UIEventType::Invoke_Invoked, EvenementUia::Invocation),
        (UIEventType::Text_TextChanged, EvenementUia::TexteChange),
        (
            UIEventType::SelectionItem_ElementSelected,
            EvenementUia::SelectionChangee,
        ),
        (
            UIEventType::StructureChanged,
            EvenementUia::StructureChangee,
        ),
    ];

    let mut poignees = Vec::new();
    for &(brut, notre) in abonnes {
        let puits = puits.clone();
        let auto = automation.clone();
        let h: UIEventHandler = (Box::new(move |sender: &UIElement, _k| {
            let cible = cible_depuis(&auto, sender);
            let _ = puits.send(RawEvent {
                source: Source::Uia,
                monotone_ms: debut.elapsed().as_millis() as u64,
                genre: notre.genre(cible),
            });
            Ok(())
        }) as Box<CustomEventHandlerFn>)
            .into();

        if automation
            .add_automation_event_handler(brut, &racine, TreeScope::Descendants, None, &h)
            .is_err()
        {
            eprintln!("[noe] abonnement UIA refuse pour {brut:?}");
            continue;
        }
        poignees.push(h);
    }

    // R2.1 nomme le focus au même rang que l'invocation et le changement de
    // valeur. Il passe par un handler d'un AUTRE type — c'est ce que le spike
    // avait raté en croyant qu'un abonnement ordinaire suffisait.
    let focus: Option<UIFocusChangedEventHandler> = {
        let puits = puits.clone();
        let auto = automation.clone();
        let h: UIFocusChangedEventHandler = (Box::new(move |sender: &UIElement| {
            let cible = cible_depuis(&auto, sender);
            let _ = puits.send(RawEvent {
                source: Source::Uia,
                monotone_ms: debut.elapsed().as_millis() as u64,
                genre: EvenementUia::Focus.genre(cible),
            });
            Ok(())
        }) as Box<CustomFocusChangedEventHandlerFn>)
            .into();
        if automation
            .add_focus_changed_event_handler(None, &h)
            .is_err()
        {
            eprintln!("[noe] abonnement au focus refuse");
            None
        } else {
            Some(h)
        }
    };

    if poignees.is_empty() && focus.is_none() {
        eprintln!("[noe] aucun abonnement UIA n a pris : capture native inerte");
        return;
    }
    // Le focus est compté à part : il passe par un autre type de handler, et
    // c'est précisément celui-là qui peut échouer seul. Un décompte global
    // masquerait sa perte.
    eprintln!(
        "[noe] UIA : {} abonnements + focus {} (globale filtree)",
        poignees.len(),
        if focus.is_some() { "oui" } else { "NON" }
    );

    // Les poignées doivent rester vivantes tant que l'abonnement l'est : les
    // relâcher désabonnerait aussitôt. Le thread sert entre-temps les demandes
    // de photo — c'est le SEUL endroit d'où l'arbre UIA peut être lu.
    while actif.load(Ordering::SeqCst) {
        match demandes.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(repondre) => {
                let _ = repondre.send(photographier_actif(&automation));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            // Plus personne pour demander : la source a été relâchée.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = automation.remove_all_event_handlers();
}

/// Remonte l'arbre pour trouver la région, dans les budgets du spike.
#[cfg(not(test))]
fn cible_depuis(automation: &uiautomation::UIAutomation, el: &uiautomation::UIElement) -> Cible {
    let type_controle = el
        .get_control_type()
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|_| "Inconnu".into());
    let nom = el.get_name().unwrap_or_default();

    let mut region = None;
    if let Ok(walker) = automation.create_tree_walker() {
        let mut courant = el.clone();
        for _ in 0..crate::snapshot::PROFONDEUR_MAX {
            let Ok(parent) = walker.get_parent(&courant) else {
                break;
            };
            let r = parent
                .get_control_type()
                .map(|c| role_normalise(&format!("{c:?}")))
                .unwrap_or_else(|_| "generic".into());
            if region.is_none() && est_region(&r) {
                let n = parent.get_name().unwrap_or_default();
                if !n.trim().is_empty() {
                    region = Some(n);
                }
            }
            courant = parent;
        }
    }
    cible_de(&type_controle, &nom, region.as_deref())
}

/// Photographie le conteneur au premier plan (R2.3).
///
/// On part de la **fenêtre qui porte le focus**, pas de la racine du bureau : un
/// snapshot du bureau entier dépasserait tous les budgets et décrirait surtout
/// des choses qui n'ont rien à voir avec la tâche.
#[cfg(not(test))]
fn photographier_actif(automation: &uiautomation::UIAutomation) -> Option<Noeud> {
    let focalise = automation.get_focused_element().ok()?;
    let walker = automation.create_tree_walker().ok()?;

    // Remonte jusqu'à la fenêtre qui contient l'élément focalisé.
    let mut racine = focalise.clone();
    for _ in 0..crate::snapshot::PROFONDEUR_MAX {
        let r = racine
            .get_control_type()
            .map(|c| role_normalise(&format!("{c:?}")))
            .unwrap_or_else(|_| "generic".into());
        if r == "window" {
            break;
        }
        match walker.get_parent(&racine) {
            Ok(p) => racine = p,
            Err(_) => break,
        }
    }

    let mut budget = crate::snapshot::NOEUDS_MAX;
    Some(descendre(&walker, &racine, 0, &mut budget))
}

/// Descente bornée par la profondeur et le budget de nœuds du spike.
#[cfg(not(test))]
fn descendre(
    walker: &uiautomation::UITreeWalker,
    el: &uiautomation::UIElement,
    profondeur: usize,
    budget: &mut usize,
) -> Noeud {
    let role = el
        .get_control_type()
        .map(|c| role_normalise(&format!("{c:?}")))
        .unwrap_or_else(|_| "generic".into());
    let nom = el.get_name().unwrap_or_default();
    let mut noeud = Noeud::feuille(&role, &nom);

    // La valeur est ce qui distingue deux états du même écran : sans elle, un
    // diff entre deux snapshots ne verrait jamais un champ changer.
    if let Ok(v) = el.get_property_value(uiautomation::types::UIProperty::ValueValue) {
        let v = v.to_string();
        if !v.trim().is_empty() {
            noeud = noeud.valant(&v);
        }
    }

    *budget = budget.saturating_sub(1);
    if profondeur + 1 >= crate::snapshot::PROFONDEUR_MAX || *budget == 0 {
        return noeud;
    }

    let mut enfants = Vec::new();
    let mut courant = walker.get_first_child(el).ok();
    while let Some(c) = courant {
        if *budget == 0 {
            break;
        }
        enfants.push(descendre(walker, &c, profondeur + 1, budget));
        courant = walker.get_next_sibling(&c).ok();
    }
    noeud.avec(enfants)
}

/// En test, il n'y a pas de bureau : la boucle honore le drapeau et répond
/// « rien à montrer » plutôt que de laisser l'appelant attendre son délai.
#[cfg(test)]
fn boucle_uia(
    _puits: &Sender<RawEvent>,
    actif: &Arc<AtomicBool>,
    demandes: &std::sync::mpsc::Receiver<Demande>,
) {
    while actif.load(Ordering::SeqCst) {
        match demandes.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(repondre) => {
                let _ = repondre.send(None);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moteur::Snapshotteur;

    // -- Le vocabulaire partagé par les deux sources -------------------------

    #[test]
    fn les_roles_uia_parlent_le_vocabulaire_du_dom() {
        // Sans cette table, `source:"uia"` et `source:"dom"` decriraient le meme
        // bouton avec deux mots differents, et les cles de branches de la spec
        // 004 ne se rejoindraient jamais.
        for (uia, attendu) in [
            ("Button", "button"),
            ("SplitButton", "button"),
            ("Hyperlink", "link"),
            ("Edit", "textbox"),
            ("ComboBox", "combobox"),
            ("CheckBox", "checkbox"),
            ("RadioButton", "radio"),
            ("TabItem", "tab"),
            ("MenuItem", "menuitem"),
            ("ListItem", "listitem"),
            ("Table", "table"),
            ("Document", "document"),
        ] {
            assert_eq!(role_normalise(uia), attendu, "{uia}");
        }
    }

    #[test]
    fn un_type_inconnu_devient_generic_et_pas_autre_chose() {
        // Le DOM aurait dit `generic`. Inventer un role serait pire que vague :
        // il creerait une distinction que rien ne soutient.
        for inconnu in ["Pane", "Group", "Custom", "Inconnu", "", "Chose"] {
            assert_eq!(role_normalise(inconnu), "generic", "{inconnu:?}");
        }
    }

    #[test]
    fn les_roles_rendus_sont_tous_en_minuscules() {
        // Une seule majuscule qui traverse et l'egalite de cible casse en
        // silence entre les deux sources.
        for t in ["Button", "Hyperlink", "Edit", "Chose", "Window"] {
            let r = role_normalise(t);
            assert_eq!(r, r.to_lowercase(), "{t} → {r}");
        }
    }

    // -- La cible (R2.2, R2.4) ----------------------------------------------

    #[test]
    fn la_cible_porte_role_nom_et_region_et_rien_d_autre() {
        let c = cible_de("Button", "  Enregistrer  ", Some("Details"));
        assert_eq!(c.role, "button");
        assert_eq!(c.nom, "Enregistrer", "le nom est trime");
        assert_eq!(c.region.as_deref(), Some("Details"));
        assert!(c.resolue());
    }

    #[test]
    fn une_region_vide_n_est_pas_posee() {
        // Une region vide vaut moins que pas de region : elle ferait croire a un
        // contexte connu.
        for vide in [Some("   "), Some(""), None] {
            assert_eq!(cible_de("Button", "X", vide).region, None, "{vide:?}");
        }
    }

    #[test]
    fn un_element_sans_nom_est_non_resolu_mais_existe() {
        // R2.4 : jamais d'evenement muet. La cible est construite, marquee.
        let c = cible_de("Custom", "   ", None);
        assert!(!c.resolue());
        assert_eq!(c.role, "generic");
    }

    // -- La traduction des événements ---------------------------------------

    #[test]
    fn chaque_evenement_uia_a_son_genre() {
        let c = || Cible::new("button", "Enregistrer");
        assert!(matches!(
            EvenementUia::Invocation.genre(c()),
            GenreEvenement::Invocation(_)
        ));
        assert!(matches!(
            EvenementUia::TexteChange.genre(c()),
            GenreEvenement::Saisie(_)
        ));
        assert!(matches!(
            EvenementUia::SelectionChangee.genre(c()),
            GenreEvenement::ChangementValeur(_)
        ));
        assert!(matches!(
            EvenementUia::StructureChangee.genre(c()),
            GenreEvenement::ChangementStructure(_)
        ));
        assert!(matches!(
            EvenementUia::Focus.genre(c()),
            GenreEvenement::Focus(_)
        ));
    }

    #[test]
    fn les_regions_candidates_sont_des_conteneurs() {
        assert!(est_region("generic"));
        assert!(est_region("document"));
        assert!(est_region("window"));
        // Un bouton ne contient rien : le prendre pour region donnerait un
        // contexte plus etroit que l'element lui-meme.
        assert!(!est_region("button"));
        assert!(!est_region("textbox"));
        assert!(!est_region("link"));
    }

    // -- L'abonnement --------------------------------------------------------

    #[test]
    fn un_second_abonnement_est_refuse() {
        let mut s = UiaSource::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        let _a = s.abonner(tx).expect("premier abonnement");
        let (tx2, _rx2) = std::sync::mpsc::channel();
        assert!(matches!(s.abonner(tx2), Err(ErreurSource::DejaAbonne)));
    }

    #[test]
    fn relacher_l_abonnement_coupe_vraiment() {
        // C'etait une promesse de commentaire avant d'etre une propriete du
        // type : le drapeau doit retomber quand l'abonnement tombe.
        let mut s = UiaSource::new();
        let drapeau = s.actif.clone();
        let (tx, _rx) = std::sync::mpsc::channel();

        let abonnement = s.abonner(tx).expect("abonnement");
        assert!(drapeau.load(Ordering::SeqCst), "actif pendant l abonnement");

        drop(abonnement);
        assert!(!drapeau.load(Ordering::SeqCst), "coupe apres relachement");
    }

    // -- Le photographe ------------------------------------------------------

    #[test]
    fn une_demande_de_photo_sans_thread_ne_bloque_pas() {
        // Personne ne sert le canal : l'appelant doit repartir sans photo, pas
        // attendre. Un moteur bloque sur un declencheur perdrait toute la suite
        // de l'episode.
        let s = UiaSource::new();
        let photographe = s.snapshotteur();
        drop(s); // le recepteur tombe avec la source

        let debut = std::time::Instant::now();
        assert_eq!(photographe.photographier(), None);
        assert!(
            debut.elapsed() < std::time::Duration::from_millis(DELAI_PHOTO_MS),
            "l appel doit echouer vite, pas attendre le delai complet"
        );
    }

    #[test]
    fn le_photographe_repond_par_le_thread_de_capture() {
        // Sans bureau, la boucle de test repond « rien a montrer ». Ce qui est
        // verifie ici, c'est l'aller-retour : la demande part, une reponse
        // revient, et l'appelant n'a pas touche a UIA lui-meme.
        let mut s = UiaSource::new();
        let photographe = s.snapshotteur();
        let (tx, _rx) = std::sync::mpsc::channel();
        let _abonnement = s.abonner(tx).expect("abonnement");

        let debut = std::time::Instant::now();
        assert_eq!(photographe.photographier(), None);
        assert!(debut.elapsed() < std::time::Duration::from_millis(DELAI_PHOTO_MS));
    }
}
