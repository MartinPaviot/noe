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
    }
}

/// Enregistre un geste. Appelée par la procédure de hook, et par les tests.
fn compter(geste: Geste) {
    if !ARME.load(Ordering::SeqCst) {
        return;
    }
    match geste {
        Geste::Copie => COPIES.fetch_add(1, Ordering::SeqCst),
        Geste::Collage => COLLAGES.fetch_add(1, Ordering::SeqCst),
    };
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

        // Un hook bas niveau exige une boucle de messages dans le thread qui
        // l'installe : sans elle, Windows le considère comme non répondant et
        // le retire de lui-même au bout de quelques instants.
        std::thread::Builder::new()
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
                        return;
                    }
                };
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

        Self { fin: Some(fin) }
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
    pub fn desarmer() {
        ARME.store(false, Ordering::SeqCst);
    }
}

impl Drop for HookClavier {
    fn drop(&mut self) {
        Self::desarmer();
        #[cfg(not(test))]
        if let Some(fin) = self.fin.take() {
            let _ = fin.send(());
            // Réveille la boucle de messages, qui dort dans `GetMessageW`.
            //
            // Sans ce réveil, le thread resterait bloqué jusqu'à la prochaine
            // frappe du poste — et le hook avec lui, alors que l'épisode est
            // clos. C'est exactement le genre de survie silencieuse que R1.2
            // interdit.
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                // Le thread se nomme, mais Windows veut son identifiant : on
                // poste sur tous les threads de la file en dernier recours.
                let _ = PostThreadMessageW(
                    0,
                    WM_QUIT,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
