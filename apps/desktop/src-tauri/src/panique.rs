//! La panique : effacer, tout de suite, sans se justifier (spec 002, R5.3).
//!
//! Trois principes, et ils se tiennent.
//!
//! **On efface des épisodes ENTIERS.** Jamais une découpe. Un épisode clos est
//! immuable — c'est la quatrième des cinq règles — et découper les cinq dernières
//! minutes d'un épisode d'une heure produirait un épisode qui n'a jamais existé,
//! dont le rejeu ne prouverait rien. Tout épisode qui *intersecte* la fenêtre
//! part en entier, même s'il n'y touche que d'une seconde.
//!
//! **C'est irréversible.** Pas de corbeille, pas de « désarchiver ». Un
//! opérateur qui appuie sur panique veut que ce soit parti ; une corbeille lui
//! rendrait la promesse fausse au moment précis où elle compte.
//!
//! **On dit ce qu'on a effacé.** Sans demander pourquoi, mais en rendant le
//! volume : le seul moyen de vérifier que le bouton a fait quelque chose. Et
//! quand une chose n'a PAS pu être datée, on le dit aussi plutôt que de trancher
//! à sa place — la supprimer détruirait ce que l'opérateur n'a pas demandé de
//! détruire, la garder en silence lui laisserait croire qu'elle est partie.

use std::path::{Path, PathBuf};

/// Les fenêtres offertes par le menu (R5.3).
pub const FENETRES_MINUTES: [u64; 3] = [5, 15, 60];

/// Un épisode candidat à l'effacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cible {
    pub id: String,
    pub chemin: PathBuf,
    /// Ses bornes murales, en millisecondes.
    pub t0_ms: u64,
    pub t1_ms: u64,
    pub octets: u64,
    /// Un épisode en quarantaine, qu'on efface au même titre.
    pub quarantaine: bool,
}

/// Ce que la panique a fait, et ce qu'elle n'a pas pu faire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bilan {
    pub episodes: usize,
    pub octets: u64,
    /// Les dossiers dont on n'a pas su lire les bornes.
    ///
    /// Ni effacés ni tus : on ne détruit pas ce que l'opérateur n'a pas demandé
    /// de détruire, et on ne laisse pas croire que c'est parti.
    pub indatables: Vec<String>,
    /// Ceux dont la suppression a échoué — fichier verrouillé, droits.
    pub echecs: Vec<String>,
}

impl Bilan {
    /// Le message rendu à l'opérateur. C'est la « confirmation du volume ».
    pub fn message(&self) -> String {
        let mut m = if self.episodes == 0 {
            "Rien a effacer dans cette fenetre.".to_string()
        } else {
            format!(
                "{} episode(s) effaces definitivement, {}.",
                self.episodes,
                lisible(self.octets)
            )
        };
        if !self.indatables.is_empty() {
            m.push_str(&format!(
                " {} dossier(s) sans bornes lisibles n'ont PAS ete touches.",
                self.indatables.len()
            ));
        }
        if !self.echecs.is_empty() {
            m.push_str(&format!(
                " ATTENTION : {} suppression(s) ont echoue.",
                self.echecs.len()
            ));
        }
        m
    }
}

fn lisible(octets: u64) -> String {
    if octets < 1024 {
        format!("{octets} o")
    } else if octets < 1024 * 1024 {
        format!("{} Ko", octets / 1024)
    } else {
        format!("{} Mo", octets / (1024 * 1024))
    }
}

/// Deux intervalles fermés se touchent-ils ?
///
/// Fermés des deux côtés à dessein : un épisode qui se termine exactement à
/// l'instant où la fenêtre commence a bien eu lieu pendant la fenêtre. Sur un
/// bouton d'urgence, le doute penche du côté qui efface.
pub fn intersecte(a0: u64, a1: u64, b0: u64, b1: u64) -> bool {
    a0 <= b1 && b0 <= a1
}

/// L'instant encodé dans un ULID, en millisecondes.
///
/// Les dix premiers caractères d'un ULID portent 48 bits de temps en base32 de
/// Crockford. C'est ce qui permet de dater un épisode en quarantaine, dont le
/// seul fichier est une raison en texte : sans ça, la panique le laisserait
/// derrière elle, ce qui est exactement ce qu'on ne veut pas d'un bouton
/// d'urgence.
pub fn instant_ulid(id: &str) -> Option<u64> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let brut = id.as_bytes();
    if brut.len() != 26 {
        return None;
    }
    let mut ms: u64 = 0;
    for c in &brut[..10] {
        let c = c.to_ascii_uppercase();
        let v = ALPHABET.iter().position(|a| *a == c)?;
        ms = ms.checked_mul(32)?.checked_add(v as u64)?;
    }
    Some(ms)
}

/// La taille d'un dossier, récursivement.
fn poids(dossier: &Path) -> u64 {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return 0;
    };
    entrees
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => poids(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

/// Les bornes d'un dossier d'épisode, quelle que soit sa forme.
///
/// Trois formes coexistent sur le disque, et chacune date autrement : un épisode
/// assemblé porte `t0`/`t1` en clair, un orphelin porte son ouverture dans le
/// marqueur et sa durée dans la dernière ligne du journal, un épisode en
/// quarantaine n'a que son identifiant — d'où le décodage du ULID.
fn bornes(dossier: &Path, id: &str) -> Option<(u64, u64)> {
    // 1. L'épisode assemblé.
    if let Ok(texte) = std::fs::read_to_string(dossier.join("episode.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&texte) {
            let t0 = v.get("t0").and_then(|x| x.as_str()).and_then(depuis_iso);
            let t1 = v.get("t1").and_then(|x| x.as_str()).and_then(depuis_iso);
            if let (Some(a), Some(b)) = (t0, t1) {
                return Some((a, b));
            }
        }
    }
    // 2. L'orphelin : le marqueur porte l'ouverture depuis D30.
    if let Ok(texte) = std::fs::read_to_string(dossier.join(".ouvert")) {
        if let Ok(m) = serde_json::from_str::<crate::journal::Marqueur>(&texte) {
            let duree = derniere_duree(&dossier.join("journal.jsonl"));
            return Some((m.t0_mural_ms, m.t0_mural_ms + duree));
        }
    }
    // 3. La quarantaine, ou tout dossier qui n'a plus que son nom.
    //
    // Un instant, pas un intervalle : on ne sait pas combien de temps il a duré.
    // Le traiter comme ponctuel est le choix qui efface le moins — et la panique
    // reste une suppression, pas un ménage.
    instant_ulid(id).map(|t| (t, t))
}

/// La durée de l'épisode au moment où le journal s'est arrêté.
///
/// Les instants du journal sont comptés depuis l'ouverture (D30) : le dernier
/// donne donc la durée, sans avoir à relire tout le fichier en JSON.
fn derniere_duree(journal: &Path) -> u64 {
    let Ok(texte) = std::fs::read_to_string(journal) else {
        return 0;
    };
    texte
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find_map(|v| v.get("monotone_ms").and_then(serde_json::Value::as_u64))
        .unwrap_or(0)
}

/// `2026-01-14T09:12:03.000Z` → millisecondes depuis l'époque.
///
/// L'inverse de `assemblage::horodater`, et testé comme tel : un aller-retour
/// qui ne revient pas au même point ferait effacer la mauvaise fenêtre.
pub fn depuis_iso(iso: &str) -> Option<u64> {
    let b = iso.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let n = |a: usize, z: usize| iso.get(a..z)?.parse::<i64>().ok();
    let (annee, mois, jour) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, m, s) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let millis = if b.len() >= 23 && b[19] == b'.' {
        n(20, 23)?
    } else {
        0
    };

    // Jours depuis l'époque, algorithme civil-from-days de Howard Hinnant —
    // le même que `horodater` emploie dans l'autre sens.
    let a = if mois <= 2 { annee - 1 } else { annee };
    let era = a.div_euclid(400);
    let yoe = a - era * 400;
    let mp = if mois > 2 { mois - 3 } else { mois + 9 };
    let doy = (153 * mp + 2) / 5 + jour - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let jours = era * 146_097 + doe - 719_468;

    let total = jours * 86_400_000 + h * 3_600_000 + m * 60_000 + s * 1_000 + millis;
    u64::try_from(total).ok()
}

/// Tout ce qui intersecte la fenêtre, dossier par dossier.
pub fn inventorier(racine: &Path, debut_ms: u64, fin_ms: u64) -> (Vec<Cible>, Vec<String>) {
    let mut cibles = Vec::new();
    let mut indatables = Vec::new();

    let mut examiner = |dossier: PathBuf, id: String, quarantaine: bool| match bornes(&dossier, &id)
    {
        Some((t0, t1)) if intersecte(t0, t1, debut_ms, fin_ms) => cibles.push(Cible {
            id,
            octets: poids(&dossier),
            chemin: dossier,
            t0_ms: t0,
            t1_ms: t1,
            quarantaine,
        }),
        Some(_) => {}
        None => indatables.push(id),
    };

    if let Ok(entrees) = std::fs::read_dir(racine) {
        for e in entrees.flatten() {
            let chemin = e.path();
            if !chemin.is_dir() {
                continue;
            }
            let nom = e.file_name().to_string_lossy().to_string();
            if nom == "quarantaine" {
                if let Ok(q) = std::fs::read_dir(&chemin) {
                    for qe in q.flatten() {
                        if qe.path().is_dir() {
                            let id = qe.file_name().to_string_lossy().to_string();
                            examiner(qe.path(), id, true);
                        }
                    }
                }
                continue;
            }
            examiner(chemin, nom, false);
        }
    }

    cibles.sort_by(|a, b| a.id.cmp(&b.id));
    indatables.sort();
    (cibles, indatables)
}

/// Efface. Définitivement.
///
/// Les fichiers d'épisode sont en **lecture seule** — c'est l'immutabilité de la
/// spec 001. Il faut donc lever l'attribut avant de supprimer, sinon la panique
/// échouerait précisément sur ce qu'elle est censée effacer, et le dirait après
/// coup à un opérateur qui croyait le problème réglé.
pub fn effacer(racine: &Path, debut_ms: u64, fin_ms: u64) -> Bilan {
    let (cibles, indatables) = inventorier(racine, debut_ms, fin_ms);
    let mut bilan = Bilan {
        indatables,
        ..Bilan::default()
    };

    for c in &cibles {
        rendre_inscriptible(&c.chemin);
        match std::fs::remove_dir_all(&c.chemin) {
            Ok(()) => {
                bilan.episodes += 1;
                bilan.octets += c.octets;
            }
            Err(err) => {
                eprintln!("[noe] panique : {} non efface : {err}", c.id);
                bilan.echecs.push(c.id.clone());
            }
        }
    }
    bilan
}

/// Lève l'attribut lecture seule sur tout un sous-arbre.
fn rendre_inscriptible(dossier: &Path) {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return;
    };
    for e in entrees.flatten() {
        let chemin = e.path();
        if chemin.is_dir() {
            rendre_inscriptible(&chemin);
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&chemin) {
            let mut p = meta.permissions();
            if p.readonly() {
                // Clippy previent qu'un `set_readonly(false)` rend le fichier
                // accessible en ecriture a tous SUR UNIX. Cette application est
                // Windows uniquement — `Cargo.toml` ne compile la moitie de ses
                // dependances que la — et sur Windows l'appel ne fait que lever
                // l'attribut lecture seule, ce qui est exactement le geste
                // voulu. Le fichier est efface la ligne d'apres.
                #[allow(clippy::permissions_set_readonly_false)]
                p.set_readonly(false);
                let _ = std::fs::set_permissions(&chemin, p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-01-14T09:12:00.000Z
    const T: u64 = 1_768_381_920_000;

    fn racine(nom: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("noe-panique-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn episode(racine: &Path, id: &str, t0: u64, t1: u64) -> PathBuf {
        let d = racine.join(id);
        std::fs::create_dir_all(&d).unwrap();
        let json = serde_json::json!({
            "id": id,
            "t0": crate::assemblage::horodater(t0),
            "t1": crate::assemblage::horodater(t1),
        });
        let chemin = d.join("episode.json");
        std::fs::write(&chemin, serde_json::to_string(&json).unwrap()).unwrap();
        // Comme en production : l'episode persiste est en lecture seule.
        let mut p = std::fs::metadata(&chemin).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&chemin, p).unwrap();
        d
    }

    #[test]
    fn l_aller_retour_d_horodatage_revient_au_meme_point() {
        // Un aller-retour qui derive ferait effacer la mauvaise fenetre.
        for ms in [
            0u64,
            T,
            1_767_225_600_000,
            1_835_395_200_000, // 2028-02-29, annee bissextile
            2_000_000_000_000,
        ] {
            let iso = crate::assemblage::horodater(ms);
            assert_eq!(depuis_iso(&iso), Some(ms), "{iso}");
        }
    }

    #[test]
    fn un_horodatage_illisible_ne_fait_pas_paniquer_la_panique() {
        for mauvais in [
            "",
            "hier",
            "2026-01-14",
            "2026/01/14T09:12:03.000Z",
            "xxxx-xx-xxT",
        ] {
            assert_eq!(depuis_iso(mauvais), None, "{mauvais}");
        }
    }

    #[test]
    fn un_episode_qui_effleure_la_fenetre_part_en_entier() {
        // R5.3 : jamais de decoupe partielle. Un episode d'une heure qui touche
        // la fenetre d'une seconde part en entier — l'immutabilite de la spec
        // 001 interdit de le recouper, et un episode recoupe ne rejouerait rien.
        let r = racine("effleure");
        let long = episode(&r, "01ep-long", T, T + 3_600_000);
        let bilan = effacer(&r, T + 3_599_000, T + 7_200_000);
        assert_eq!(bilan.episodes, 1);
        assert!(!long.exists(), "l episode entier doit avoir disparu");
    }

    #[test]
    fn un_episode_hors_fenetre_n_est_pas_touche() {
        let r = racine("hors-fenetre");
        let garde = episode(&r, "01ep-avant", T, T + 60_000);
        let vise = episode(&r, "01ep-pendant", T + 600_000, T + 660_000);
        let bilan = effacer(&r, T + 500_000, T + 700_000);
        assert_eq!(bilan.episodes, 1);
        assert!(garde.exists(), "hors fenetre, on ne touche pas");
        assert!(!vise.exists());
    }

    #[test]
    fn la_lecture_seule_n_empeche_pas_l_effacement() {
        // L'immutabilite protege contre l'accident ; elle ne doit pas proteger
        // contre l'operateur qui demande explicitement l'effacement. Sans la
        // levee de l'attribut, la panique echouerait precisement sur ce qu'elle
        // est censee effacer.
        let r = racine("lecture-seule");
        let d = episode(&r, "01ep-fige", T, T + 1_000);
        assert!(
            std::fs::metadata(d.join("episode.json"))
                .unwrap()
                .permissions()
                .readonly(),
            "le banc doit bien poser l attribut"
        );
        let bilan = effacer(&r, T, T + 1_000);
        assert_eq!(bilan.episodes, 1);
        assert!(bilan.echecs.is_empty(), "{:?}", bilan.echecs);
        assert!(!d.exists());
    }

    #[test]
    fn l_effacement_emporte_tout_le_dossier() {
        // « Les evenements, snapshots et derives associes » : tout ce qui est
        // dans le dossier de l'episode part avec lui. Un journal laisse derriere
        // rendrait la panique decorative.
        let r = racine("tout-le-dossier");
        let d = episode(&r, "01ep-complet", T, T + 1_000);
        std::fs::write(d.join("journal.jsonl"), b"{\"kind\":\"ui_action\"}\n").unwrap();
        std::fs::create_dir_all(d.join("snapshots")).unwrap();
        std::fs::write(d.join("snapshots").join("1.json"), b"{}").unwrap();

        effacer(&r, T, T + 1_000);
        assert!(!d.exists());
        assert!(!d.join("journal.jsonl").exists());
        assert!(!d.join("snapshots").exists());
    }

    #[test]
    fn un_orphelin_est_date_par_son_marqueur() {
        // Un episode tue par un crash n'a pas d'`episode.json`. Sans lecture du
        // marqueur, la panique le laisserait derriere elle — c'est-a-dire
        // qu'elle laisserait exactement ce que l'operateur veut voir partir.
        let r = racine("orphelin");
        let d = r.join("01ep-orphelin");
        std::fs::create_dir_all(&d).unwrap();
        let m = crate::journal::Marqueur {
            episode_id: "01ep-orphelin".into(),
            task_slug: "maj-crm".into(),
            t0_mural_ms: T,
        };
        std::fs::write(d.join(".ouvert"), serde_json::to_string(&m).unwrap()).unwrap();
        std::fs::write(
            d.join("journal.jsonl"),
            b"{\"kind\":\"cloture_auto\",\"seq\":1,\"monotone_ms\":45000}\n",
        )
        .unwrap();

        let (cibles, _) = inventorier(&r, T + 40_000, T + 50_000);
        assert_eq!(cibles.len(), 1, "l orphelin doit etre vu");
        assert_eq!(cibles[0].t1_ms, T + 45_000, "sa duree vient du journal");
        assert_eq!(effacer(&r, T + 40_000, T + 50_000).episodes, 1);
    }

    #[test]
    fn un_episode_en_quarantaine_est_date_par_son_ulid() {
        // La quarantaine ne contient qu'une raison en texte. Son identifiant est
        // un ULID, dont les dix premiers caracteres portent l'instant : c'est la
        // seule facon de le dater, et la panique ne peut pas se permettre de
        // laisser un dossier derriere elle faute de savoir quand il date.
        let r = racine("quarantaine");
        let id = "01JQA1B2C3D4E5F6G7H8J9K0M1";
        let instant = instant_ulid(id).expect("ULID datable");
        let d = r.join("quarantaine").join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("raison.txt"), b"aucune action").unwrap();

        let bilan = effacer(&r, instant - 1_000, instant + 1_000);
        assert_eq!(bilan.episodes, 1);
        assert!(!d.exists());
    }

    #[test]
    fn un_ulid_decode_le_meme_instant_que_la_bibliotheque() {
        // Si le decodage derivait, la panique effacerait la mauvaise fenetre —
        // et personne ne s'en apercevrait, puisque c'est irreversible.
        let id = ulid::Ulid::from_parts(T, 42);
        assert_eq!(instant_ulid(&id.to_string()), Some(T));
    }

    #[test]
    fn un_dossier_indatable_est_epargne_et_signale() {
        // Le supprimer detruirait ce que l'operateur n'a pas demande de
        // detruire ; le taire lui laisserait croire qu'il est parti.
        let r = racine("indatable");
        let d = r.join("pas-un-ulid");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("bruit.txt"), b"?").unwrap();

        let bilan = effacer(&r, 0, u64::MAX);
        assert_eq!(bilan.episodes, 0);
        assert_eq!(bilan.indatables, vec!["pas-un-ulid".to_string()]);
        assert!(d.exists(), "on n efface pas ce qu on n a pas su dater");
        assert!(
            bilan.message().contains("PAS ete touches"),
            "{}",
            bilan.message()
        );
    }

    #[test]
    fn le_bilan_confirme_le_volume() {
        // R5.3 : « volume efface confirme ». C'est le seul moyen pour
        // l'operateur de verifier que le bouton a fait quelque chose.
        let r = racine("volume");
        let d = episode(&r, "01ep-volume", T, T + 1_000);
        std::fs::write(d.join("gros.bin"), vec![0u8; 4096]).unwrap();
        let bilan = effacer(&r, T, T + 1_000);
        assert_eq!(bilan.episodes, 1);
        assert!(bilan.octets >= 4096, "{}", bilan.octets);
        let m = bilan.message();
        assert!(m.contains("1 episode"), "{m}");
        assert!(m.contains("Ko"), "{m}");
    }

    #[test]
    fn une_fenetre_vide_le_dit_plutot_que_de_se_taire() {
        let r = racine("vide");
        episode(&r, "01ep-loin", T, T + 1_000);
        let bilan = effacer(&r, T + 10_000_000, T + 20_000_000);
        assert_eq!(bilan.episodes, 0);
        assert!(
            bilan.message().contains("Rien a effacer"),
            "{}",
            bilan.message()
        );
    }

    #[test]
    fn l_effacement_est_irreversible() {
        // Pas de corbeille, pas de dossier « supprime ». Un operateur qui appuie
        // sur panique veut que ce soit parti ; une corbeille lui rendrait la
        // promesse fausse au moment precis ou elle compte.
        let r = racine("irreversible");
        episode(&r, "01ep-parti", T, T + 1_000);
        effacer(&r, T, T + 1_000);
        let restes: Vec<String> = std::fs::read_dir(&r)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(restes.is_empty(), "rien ne doit survivre : {restes:?}");
    }

    #[test]
    fn les_bornes_sont_fermees_des_deux_cotes() {
        // Un episode qui se termine exactement a l'instant ou la fenetre
        // commence a bien eu lieu pendant la fenetre. Sur un bouton d'urgence,
        // le doute penche du cote qui efface.
        assert!(intersecte(0, 100, 100, 200), "fin = debut");
        assert!(intersecte(100, 200, 0, 100), "debut = fin");
        assert!(!intersecte(0, 99, 100, 200));
        assert!(intersecte(0, 1_000, 400, 500), "fenetre incluse");
        assert!(intersecte(400, 500, 0, 1_000), "episode inclus");
    }

    #[test]
    fn un_episode_ouvert_a_l_instant_est_dans_toutes_les_fenetres() {
        // R5.3 : « SI un episode est ouvert, ALORS il est avorte
        // integralement ». Il est ouvert, donc il touche forcement une fenetre
        // qui se termine maintenant — quelle que soit sa duree.
        let r = racine("ouvert-maintenant");
        let d = r.join("01ep-en-cours");
        std::fs::create_dir_all(&d).unwrap();
        let m = crate::journal::Marqueur {
            episode_id: "01ep-en-cours".into(),
            task_slug: "maj-crm".into(),
            t0_mural_ms: T,
        };
        std::fs::write(d.join(".ouvert"), serde_json::to_string(&m).unwrap()).unwrap();

        for minutes in FENETRES_MINUTES {
            let maintenant = T + 30_000;
            let (cibles, _) = inventorier(&r, maintenant - minutes * 60_000, maintenant);
            assert_eq!(cibles.len(), 1, "fenetre de {minutes} min");
        }
    }

    #[test]
    fn les_trois_fenetres_du_menu_sont_celles_de_l_exigence() {
        assert_eq!(FENETRES_MINUTES, [5, 15, 60]);
    }
}
