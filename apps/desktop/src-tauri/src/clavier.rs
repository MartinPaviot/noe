//! Le hook clavier du cinquième déclencheur (spec 002, R2.3 — D27, option 1).
//!
//! Windows n'émet aucun événement « l'utilisateur a collé ». La seule voie
//! fiable est un hook `WH_KEYBOARD_LL`, qui voit **toutes** les frappes du
//! poste. C'est une capacité lourde pour un produit dont la première règle est
//! qu'aucun contenu utilisateur ne quitte la machine ; l'opérateur l'a accordée
//! le 2026-08-27, à condition qu'elle ne vive que pendant l'épisode.
//!
//! Quatre garanties, dans le code et pas seulement dans la phrase :
//!
//! 1. **Aucune touche n'est enregistrée.** La procédure ne fait que comparer le
//!    code de touche à quatre combinaisons et incrémenter un compteur. Elle
//!    n'écrit rien, ne transmet rien, ne garde rien.
//! 2. **La décision est pure** — [`geste_de`] vit hors du code Windows et se
//!    teste intégralement, y compris sur tout ce qui ne doit RIEN déclencher.
//! 3. **Le hook ne vit que pendant l'épisode**, et sa pose comme sa dépose sont
//!    journalisées.
//! 4. **Quatre combinaisons, pas une de plus.**
//!
//! Pourquoi le hook sert aussi aux **copies** : le numéro de séquence du
//! presse-papiers change aussi quand une autre application écrit — un
//! gestionnaire de mots de passe, précisément. Lire le contenu sur ce seul
//! signal violerait R2.3. Le hook nous dit que c'est bien l'opérateur qui a
//! copié, avant toute lecture.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Ce qu'une frappe signifie pour nous — et rien d'autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geste {
    Copie,
    Collage,
}

/// Codes de touches virtuelles, nommés pour que la table se relise.
const VK_C: u32 = 0x43;
const VK_X: u32 = 0x58;
const VK_V: u32 = 0x56;
const VK_INSERT: u32 = 0x2D;

/// La seule interprétation faite d'une frappe.
///
/// **C'est ici que vit la promesse.** Une touche entre, un `Option<Geste>` sort ;
/// il n'existe aucun chemin par lequel `vk` puisse être conservé, transmis ou
/// écrit. Le reste du module n'est que de la plomberie Windows autour de cette
/// fonction.
pub fn geste_de(vk: u32, ctrl: bool, shift: bool) -> Option<Geste> {
    match (vk, ctrl, shift) {
        (VK_C, true, _) | (VK_X, true, _) => Some(Geste::Copie),
        (VK_V, true, _) => Some(Geste::Collage),
        // Maj+Inser : le raccourci historique, encore employé.
        (VK_INSERT, _, true) => Some(Geste::Collage),
        _ => None,
    }
}

/// Compteurs partagés avec la procédure de hook.
///
/// Des atomiques statiques, parce qu'une procédure de hook Windows est une
/// fonction C sans contexte : on ne peut rien lui passer. Elle fait donc le
/// strict minimum — incrémenter — et le reste du programme lit à son rythme.
static COPIES: AtomicU64 = AtomicU64::new(0);
static COLLAGES: AtomicU64 = AtomicU64::new(0);
/// Hors épisode, la procédure ne compte rien (R1.2).
static ARME: AtomicBool = AtomicBool::new(false);

/// Ce que le programme a observé depuis la dernière lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gestes {
    pub copies: u64,
    pub collages: u64,
    /// La fenêtre au premier plan **à l'instant de la frappe**, pas à celui du
    /// relevé.
    ///
    /// Le battement passe jusqu'à une seconde après. Sans cette photo prise au
    /// bon moment, une copie faite dans un gestionnaire de mots de passe puis
    /// suivie d'une bascule vers le navigateur serait attribuée au navigateur —
    /// donc autorisée, donc lue. R2.3 nomme ce cas.
    pub fenetre_copie: isize,
    /// Le numéro de séquence du presse-papiers **avant** cette copie.
    ///
    /// S'il n'a pas bougé au moment du relevé, le `Ctrl+C` n'a rien copié : une
    /// console où il vaut interruption, une sélection vide. Sans cette
    /// vérification, on s'approprierait ce qui traînait dans le presse-papiers —
    /// le mot de passe copié trente secondes plus tôt.
    pub sequence_avant_copie: u32,
    pub fenetre_collage: isize,
}

impl Gestes {
    pub fn rien(&self) -> bool {
        self.copies == 0 && self.collages == 0
    }
}

/// Relève et remet à zéro. Appelée au battement.
pub fn relever() -> Gestes {
    Gestes {
        copies: COPIES.swap(0, Ordering::SeqCst),
        collages: COLLAGES.swap(0, Ordering::SeqCst),
        fenetre_copie: FENETRE_COPIE.swap(0, Ordering::SeqCst),
        sequence_avant_copie: SEQUENCE_AVANT_COPIE.swap(0, Ordering::SeqCst),
        fenetre_collage: FENETRE_COLLAGE.swap(0, Ordering::SeqCst),
    }
}

/// Où la frappe a eu lieu, et ce que valait le presse-papiers avant elle.
///
/// Trois entiers, écrits depuis la procédure de hook. Des atomiques et non un
/// verrou : la procédure d'un hook bas niveau est sur le chemin critique du
/// clavier de tout le poste, et un verrou contesté y ferait bégayer la frappe.
///
/// Le dernier écrivain gagne. C'est le sens prudent : deux copies dans la même
/// seconde laissent la fenêtre de la seconde, et si celle-là n'est pas
/// autorisée, on refuse aussi la première. On perd un appariement, on ne fuit
/// rien.
static FENETRE_COPIE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static SEQUENCE_AVANT_COPIE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);
static FENETRE_COLLAGE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Note où la frappe a eu lieu. Production seulement : sans bureau, il n'y a pas
/// de fenêtre au premier plan.
#[cfg(not(test))]
fn situer(geste: Geste) {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY : deux lectures d'état global, sans allocation ni poignée à
    // relâcher. Appelées depuis la procédure de hook, où tout doit être bref.
    unsafe {
        let fenetre = GetForegroundWindow().0 as isize;
        match geste {
            Geste::Copie => {
                FENETRE_COPIE.store(fenetre, Ordering::SeqCst);
                SEQUENCE_AVANT_COPIE.store(GetClipboardSequenceNumber(), Ordering::SeqCst);
            }
            Geste::Collage => FENETRE_COLLAGE.store(fenetre, Ordering::SeqCst),
        }
    }
}

#[cfg(test)]
fn situer(_geste: Geste) {}

/// Enregistre un geste. Appelée par la procédure de hook, et par les tests.
fn compter(geste: Geste) {
    if !ARME.load(Ordering::SeqCst) {
        return;
    }
    match geste {
        Geste::Copie => COPIES.fetch_add(1, Ordering::SeqCst),
        Geste::Collage => COLLAGES.fetch_add(1, Ordering::SeqCst),
    };
    // Où, et dans quel état était le presse-papiers. Le compteur seul ne dit que
    // « quelqu'un a tapé Ctrl+C quelque part sur le poste » — c'est trop peu
    // pour autoriser une lecture.
    situer(geste);
}

/// Le hook posé, à relâcher pour le retirer.
///
/// R1.2 par la durée de vie : hors épisode, l'objet n'existe pas, donc le hook
/// non plus. On ne peut pas oublier de le retirer sans oublier de relâcher
/// l'épisode lui-même.
#[must_use = "relacher retire le hook clavier"]
pub struct HookClavier {
    #[cfg(not(test))]
    fin: Option<std::sync::mpsc::Sender<()>>,
    /// L'identifiant du fil qui porte le hook.
    ///
    /// Sans lui, `Drop` n'avait personne a reveiller : il postait `WM_QUIT` sur
    /// le fil `0`, qui n'existe pas — `PostThreadMessage` n'a aucune semantique
    /// de diffusion, contrairement a `HWND_BROADCAST` de `PostMessage`. L'appel
    /// echouait, le resultat etait jete, le fil restait endormi dans
    /// `GetMessageW` et `UnhookWindowsHookEx` n'etait jamais atteint. Un hook de
    /// plus par episode, tous chaines sur la meme procedure : au troisieme
    /// episode d'une session, un seul Ctrl+V comptait trois collages.
    #[cfg(not(test))]
    fil_id: u32,
    #[cfg(not(test))]
    fil: Option<std::thread::JoinHandle<()>>,
}

impl HookClavier {
    /// Pose le hook. Il ne comptera rien tant que l'épisode n'est pas ouvert.
    #[cfg(not(test))]
    pub fn poser() -> Self {
        use windows::Win32::UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
            WM_SYSKEYDOWN,
        };

        /// La procédure de hook.
        ///
        /// # Safety
        /// Signature imposée par Windows. `lparam` pointe sur un
        /// `KBDLLHOOKSTRUCT` valide quand `code >= 0`.
        unsafe extern "system" fn procedure(
            code: i32,
            wparam: windows::Win32::Foundation::WPARAM,
            lparam: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                GetAsyncKeyState, VK_CONTROL, VK_SHIFT,
            };

            if code >= 0 && ARME.load(Ordering::SeqCst) {
                let message = wparam.0 as u32;
                if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
                    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                    // L'UNIQUE usage fait du code de touche : le passer à
                    // `geste_de`, qui ne rend qu'une intention. Il n'est ni
                    // copié, ni stocké, ni transmis.
                    let ctrl = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0;
                    let shift = GetAsyncKeyState(VK_SHIFT.0 as i32) < 0;
                    if let Some(g) = geste_de(info.vkCode, ctrl, shift) {
                        compter(g);
                    }
                }
            }
            CallNextHookEx(None, code, wparam, lparam)
        }

        let (fin, attendre_fin) = std::sync::mpsc::channel::<()>();
        // Le fil annonce son identifiant une fois le hook pose. `poser` attend
        // cette annonce : sans elle, l'episode se declarerait ouvert avant que
        // le hook existe, et `Drop` n'aurait personne a reveiller s'il tombait
        // dans l'intervalle.
        let (dire_id, savoir_id) = std::sync::mpsc::channel::<Option<u32>>();

        // Un hook bas niveau exige une boucle de messages dans le thread qui
        // l'installe : sans elle, Windows le considère comme non répondant et
        // le retire de lui-même au bout de quelques instants.
        let fil = std::thread::Builder::new()
            .name("noe-clavier".into())
            .spawn(move || {
                // SAFETY : `procedure` a la signature imposée ; `None` pour le
                // module et `0` pour le thread demandent un hook global.
                let hook: HHOOK = match unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(procedure), None, 0)
                } {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("[noe] hook clavier refuse : {e} — le collage ne sera pas vu");
                        let _ = dire_id.send(None);
                        return;
                    }
                };
                // SAFETY : lecture d'un entier propre au fil courant.
                let _ = dire_id.send(Some(unsafe {
                    windows::Win32::System::Threading::GetCurrentThreadId()
                }));
                eprintln!("[noe] hook clavier pose (4 combinaisons surveillees)");

                let mut msg = MSG::default();
                loop {
                    if attendre_fin.try_recv().is_ok() {
                        break;
                    }
                    // SAFETY : `msg` est vivant ; `None` prend tous les messages
                    // du thread.
                    let recu = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                    if recu.0 <= 0 {
                        break;
                    }
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }

                // SAFETY : `hook` vient d'un `SetWindowsHookExW` réussi.
                let _ = unsafe { UnhookWindowsHookEx(hook) };
                eprintln!("[noe] hook clavier retire");
            })
            .ok();

        // Deux secondes suffisent largement a poser un hook ; au-dela, quelque
        // chose ne va pas et il vaut mieux un episode sans detection de collage
        // qu'un demarrage bloque.
        let fil_id = savoir_id
            .recv_timeout(std::time::Duration::from_secs(2))
            .ok()
            .flatten()
            .unwrap_or(0);
        if fil_id == 0 {
            eprintln!("[noe] hook clavier : aucun fil identifie, le collage ne sera pas vu");
        }

        Self {
            fin: Some(fin),
            fil_id,
            fil,
        }
    }

    /// En test, aucun hook n'est posé : c'est la logique qui est vérifiée.
    #[cfg(test)]
    pub fn poser() -> Self {
        Self {}
    }

    /// Arme le comptage. À l'ouverture de l'épisode.
    pub fn armer() {
        COPIES.store(0, Ordering::SeqCst);
        COLLAGES.store(0, Ordering::SeqCst);
        ARME.store(true, Ordering::SeqCst);
    }

    /// Désarme. À la clôture — la procédure cesse de compter avant même que le
    /// hook ne soit retiré, si bien qu'aucune frappe ne peut se glisser entre
    /// les deux.
    ///
    /// **Et purge les compteurs.** Seul `armer()` le faisait, à l'ouverture
    /// suivante : un Ctrl+C tapé deux cents millisecondes avant le hotkey de fin
    /// laissait le compteur à un, et le battement suivant — jusqu'à une seconde
    /// après la clôture — ouvrait le presse-papiers et le lisait, hors épisode.
    /// R1.2 dit que rien n'entre après la clôture ; il n'y avait rien qui
    /// entrait au journal, mais il y avait bien une lecture.
    pub fn desarmer() {
        ARME.store(false, Ordering::SeqCst);
        COPIES.store(0, Ordering::SeqCst);
        COLLAGES.store(0, Ordering::SeqCst);
        FENETRE_COPIE.store(0, Ordering::SeqCst);
        SEQUENCE_AVANT_COPIE.store(0, Ordering::SeqCst);
        FENETRE_COLLAGE.store(0, Ordering::SeqCst);
    }
}

impl Drop for HookClavier {
    fn drop(&mut self) {
        Self::desarmer();
        #[cfg(not(test))]
        {
            if let Some(fin) = self.fin.take() {
                let _ = fin.send(());
            }
            // Réveille la boucle de messages, qui dort dans `GetMessageW`.
            //
            // Sur SON identifiant de fil, obtenu par `GetCurrentThreadId` à
            // l'installation. La version précédente postait sur le fil `0` en
            // croyant à une diffusion : `PostThreadMessage` n'en a pas, l'appel
            // échouait avec `ERROR_INVALID_THREAD_ID`, et le `let _ =` l'avalait.
            // Le fil restait endormi, `UnhookWindowsHookEx` n'était jamais
            // atteint, et chaque épisode ajoutait un hook à la chaîne.
            if self.fil_id != 0 {
                // SAFETY : poste un message sur une file de fil ; aucun pointeur
                // n'est transmis.
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                    if let Err(e) = PostThreadMessageW(
                        self.fil_id,
                        WM_QUIT,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    ) {
                        eprintln!("[noe] reveil du fil clavier refuse : {e}");
                    }
                }
            }
            // Et on l'attend. « Le hook est retiré » doit être une garantie, pas
            // un espoir : sans jointure, l'épisode suivant peut s'ouvrir pendant
            // que l'ancien hook est encore dans la chaîne.
            if let Some(fil) = self.fil.take() {
                if fil.join().is_err() {
                    eprintln!("[noe] le fil clavier s est termine en panique");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Les tests de ce module partagent COPIES, COLLAGES et ARME — des
    /// statiques de processus. En parallele, ils se marchent dessus : un test
    /// arme pendant qu'un autre desarme, et le rouge tombe sur le mauvais.
    /// Un banc qui varie ne prouve rien, et D21 en fait un rouge.
    static BANC: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Prend le banc. Un test qui panique empoisonne le verrou ; on reprend la
    /// valeur plutot que de faire echouer tous les suivants pour la meme cause.
    fn banc() -> std::sync::MutexGuard<'static, ()> {
        BANC.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    use super::*;

    /// Les tests touchent des compteurs statiques : ils ne peuvent pas courir
    /// en parallèle sans se marcher dessus.
    static VERROU: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn seul() -> std::sync::MutexGuard<'static, ()> {
        VERROU.lock().unwrap_or_else(|e| e.into_inner())
    }

    // -- La table des gestes : ce qui compte, et surtout ce qui ne compte pas --

    #[test]
    fn les_quatre_combinaisons_sont_reconnues() {
        assert_eq!(geste_de(VK_C, true, false), Some(Geste::Copie));
        assert_eq!(geste_de(VK_X, true, false), Some(Geste::Copie));
        assert_eq!(geste_de(VK_V, true, false), Some(Geste::Collage));
        assert_eq!(geste_de(VK_INSERT, false, true), Some(Geste::Collage));
    }

    #[test]
    fn une_frappe_ordinaire_ne_declenche_rien() {
        // LE test de la promesse faite a l'operateur. On enumere ce qu'un
        // humain tape vraiment : des lettres, des chiffres, la ponctuation.
        // Aucune ne doit rendre autre chose que `None`.
        for vk in 0x30..=0x5Au32 {
            // Sans modificateur, absolument rien ne se declenche.
            assert_eq!(geste_de(vk, false, false), None, "vk={vk:#04x} sans modif");
        }
        for vk in [0x20, 0x0D, 0x08, 0x09, 0x1B, 0xBC, 0xBE, 0xC0] {
            assert_eq!(geste_de(vk, false, false), None, "vk={vk:#04x}");
            assert_eq!(geste_de(vk, true, false), None, "ctrl+{vk:#04x}");
            assert_eq!(geste_de(vk, false, true), None, "maj+{vk:#04x}");
        }
    }

    #[test]
    fn ctrl_avec_une_autre_lettre_ne_declenche_rien() {
        // Ctrl+A, Ctrl+S, Ctrl+Z : des raccourcis tres frequents, qui ne sont
        // ni des copies ni des collages.
        for vk in [0x41u32, 0x53, 0x5A, 0x50, 0x46] {
            assert_eq!(geste_de(vk, true, false), None, "ctrl+{vk:#04x}");
        }
    }

    #[test]
    fn c_sans_ctrl_est_juste_la_lettre_c() {
        assert_eq!(geste_de(VK_C, false, false), None);
        assert_eq!(geste_de(VK_C, false, true), None, "Maj+C reste une lettre");
    }

    #[test]
    fn inser_sans_maj_ne_colle_pas() {
        assert_eq!(geste_de(VK_INSERT, false, false), None);
    }

    // -- L'armement : R1.2 par le comptage -----------------------------------

    #[test]
    fn desarme_rien_n_est_compte() {
        let _banc = banc();
        let _g = seul();
        HookClavier::desarmer();
        let _ = relever();

        compter(Geste::Copie);
        compter(Geste::Collage);

        assert!(
            relever().rien(),
            "R1.2 : hors episode, aucune frappe ne doit etre comptee"
        );
    }

    #[test]
    fn arme_les_gestes_sont_comptes() {
        let _banc = banc();
        let _g = seul();
        HookClavier::armer();

        compter(Geste::Copie);
        compter(Geste::Copie);
        compter(Geste::Collage);

        let g = relever();
        assert_eq!(g.copies, 2);
        assert_eq!(g.collages, 1);
        HookClavier::desarmer();
    }

    #[test]
    fn relever_remet_les_compteurs_a_zero() {
        let _banc = banc();
        // Sinon chaque battement re-signalerait les memes collages.
        let _g = seul();
        HookClavier::armer();
        compter(Geste::Collage);
        assert_eq!(relever().collages, 1);
        assert!(relever().rien(), "la seconde releve doit etre vide");
        HookClavier::desarmer();
    }

    #[test]
    fn armer_repart_de_zero() {
        let _banc = banc();
        // Un episode ne doit pas heriter des gestes du precedent.
        let _g = seul();
        HookClavier::armer();
        compter(Geste::Copie);
        HookClavier::armer();
        assert!(relever().rien());
        HookClavier::desarmer();
    }

    #[test]
    fn relacher_le_hook_desarme() {
        let _banc = banc();
        let _g = seul();
        HookClavier::armer();
        {
            let _hook = HookClavier::poser();
        }
        compter(Geste::Collage);
        assert!(
            relever().rien(),
            "le hook relache doit cesser de compter, sans qu on ait a y penser"
        );
    }

    #[test]
    fn desarmer_purge_les_compteurs() {
        let _banc = banc();
        // Seul `armer()` les remettait a zero, a l'ouverture SUIVANTE. Un Ctrl+C
        // tape deux cents millisecondes avant le hotkey de fin laissait le
        // compteur a un, et le battement d'apres — jusqu'a une seconde apres la
        // cloture — ouvrait le presse-papiers hors episode.
        HookClavier::armer();
        compter(Geste::Copie);
        compter(Geste::Collage);
        HookClavier::desarmer();
        let apres = relever();
        assert!(apres.rien(), "copies={} collages={}", apres.copies, apres.collages);
    }

    #[test]
    fn le_drop_d_un_ancien_hook_ne_desarme_pas_le_nouveau() {
        let _banc = banc();
        // `*x = Some(poser())` evalue la droite PUIS droppe l'ancienne valeur, et
        // `Drop` appelle `desarmer()`. L'episode qui suivait une cloture
        // automatique demarrait donc avec ARME a faux et ne comptait aucun
        // copier-coller, sans le moindre message. L'ordre correct — relacher,
        // armer, poser — est celui qu'on fige ici.
        let ancien = HookClavier::poser();
        drop(ancien);
        HookClavier::armer();
        let nouveau = HookClavier::poser();
        compter(Geste::Copie);
        assert_eq!(relever().copies, 1, "le comptage doit etre arme");
        drop(nouveau);
        HookClavier::desarmer();
    }

}
