//! Le writer du journal de capture (spec 002, R3.1, R3.2).
//!
//! Append JSONL, une ligne par entrée, `seq` strictement croissant, vidage du
//! tampon toutes les 5 s ou tous les 100 événements, `fsync` à la clôture.
//!
//! Deux propriétés commandent tout le reste :
//!
//! - **Un épisode ouvert se voit depuis le disque.** Un fichier `.ouvert` posé à
//!   l'ouverture et retiré à la clôture propre suffit : si le processus meurt,
//!   le marqueur reste, et le démarrage suivant sait qu'il y a un orphelin à
//!   clore. Déduire l'état de la dernière ligne du journal ne marcherait pas —
//!   un crash en plein `write` la laisse tronquée.
//! - **Une ligne tronquée n'est pas une ligne perdue en silence.** À la reprise,
//!   elle est écartée du journal ET déclarée : R3.4 interdit qu'une
//!   discontinuité passe inaperçue.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::horloge::Horloge;
use crate::moteur::{CauseGap, EntreeJournal};

/// Vidage du tampon au plus tard après ce délai (R3.1).
pub const FLUSH_MS: u64 = 5_000;
/// …ou après ce nombre d'entrées, selon ce qui arrive en premier (R3.1).
pub const FLUSH_ENTREES: usize = 100;

const NOM_JOURNAL: &str = "journal.jsonl";
const NOM_MARQUEUR: &str = ".ouvert";

/// Ce que le marqueur contient : de quoi reprendre l'épisode après un crash.
///
/// Il ne portait que l'identifiant. Or `assembler` a besoin du `task_slug` et de
/// la borne murale d'ouverture, qui ne vivaient que dans la `Session` — donc en
/// mémoire, donc perdus au crash. Un orphelin ne pouvait pas être assemblé : on
/// écrivait son `gap{crash}`, on retirait le marqueur, et l'épisode restait sans
/// `episode.json`, invisible dans la fenêtre. R3.2 demande de le « passer au
/// pipeline de clôture normal » ; il n'y avait pas de quoi.
///
/// Écrit avant la première ligne du journal, comme le marqueur qu'il remplace :
/// un processus qui meurt entre les deux laisse un épisode vide mais nommé.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Marqueur {
    pub episode_id: String,
    pub task_slug: String,
    /// L'instant mural d'ouverture, en millisecondes depuis l'époque.
    pub t0_mural_ms: u64,
}

pub struct Journal {
    dossier: PathBuf,
    fichier: File,
    tampon: Vec<String>,
    dernier_vidage_ms: u64,
    horloge: std::sync::Arc<dyn Horloge>,
    ecrites: u64,
}

/// Ce qu'on a trouvé d'un épisode interrompu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphelin {
    pub episode_id: String,
    /// Entrées relues avec succès.
    pub entrees: Vec<EntreeJournal>,
    /// Une ligne incomplète a été écartée — R3.4.
    pub ligne_tronquee: bool,
    /// Les `seq` manquants entre la première et la dernière entrée relue.
    ///
    /// R3.4 : « toute discontinuité de `seq` détectée à la relecture DOIT
    /// produire `gap{cause:"seq_break"}` ». Une ligne qui a disparu proprement —
    /// secteur illisible, troncature du système de fichiers — ne laisse aucune
    /// trace visible : seul le compte des numéros la révèle.
    pub seq_manquants: Vec<u64>,
    /// Le `seq` de la dernière entrée saine, qui borne le trou.
    pub dernier_seq: u64,
    /// L'identité relue du marqueur, quand il est lisible.
    ///
    /// `None` pour un marqueur écrit par une version antérieure, qui ne portait
    /// que l'identifiant : l'épisode est alors clos et signalé, mais pas
    /// assemblé — on ne devine pas une tâche.
    pub marqueur: Option<Marqueur>,
}

impl Journal {
    pub fn ouvrir(
        racine: &Path,
        episode_id: &str,
        horloge: std::sync::Arc<dyn Horloge>,
        task_slug: &str,
    ) -> std::io::Result<Self> {
        let dossier = racine.join(episode_id);
        std::fs::create_dir_all(&dossier)?;

        // Le marqueur est posé AVANT la première écriture : si le processus
        // meurt entre les deux, l'épisode est vide mais signalé, ce qui vaut
        // mieux qu'un journal orphelin qu'on ne saurait pas chercher.
        let marqueur = Marqueur {
            episode_id: episode_id.to_string(),
            task_slug: task_slug.to_string(),
            t0_mural_ms: horloge.mural_ms(),
        };
        let rendu = serde_json::to_string(&marqueur)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(dossier.join(NOM_MARQUEUR), rendu.as_bytes())?;

        let fichier = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dossier.join(NOM_JOURNAL))?;

        let dernier_vidage_ms = horloge.monotone_ms();
        Ok(Self {
            dossier,
            fichier,
            tampon: Vec::new(),
            dernier_vidage_ms,
            horloge,
            ecrites: 0,
        })
    }

    pub fn ecrire(&mut self, entree: &EntreeJournal) -> std::io::Result<()> {
        let ligne = serde_json::to_string(entree)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.tampon.push(ligne);
        if self.tampon.len() >= FLUSH_ENTREES {
            self.vider()?;
        }
        Ok(())
    }

    /// À appeler régulièrement : c'est elle qui honore le délai de 5 s.
    pub fn battre(&mut self) -> std::io::Result<()> {
        let maintenant = self.horloge.monotone_ms();
        if !self.tampon.is_empty() && maintenant.saturating_sub(self.dernier_vidage_ms) >= FLUSH_MS
        {
            self.vider()?;
        }
        Ok(())
    }

    fn vider(&mut self) -> std::io::Result<()> {
        if self.tampon.is_empty() {
            return Ok(());
        }
        // Une seule écriture pour tout le lot : `write_all` sur un tampon
        // assemblé laisse moins de fenêtres de troncature que N appels.
        let mut bloc = String::new();
        for l in &self.tampon {
            bloc.push_str(l);
            bloc.push('\n');
        }
        self.fichier.write_all(bloc.as_bytes())?;
        self.fichier.flush()?;
        self.ecrites += self.tampon.len() as u64;
        self.tampon.clear();
        self.dernier_vidage_ms = self.horloge.monotone_ms();
        Ok(())
    }

    /// Clôture propre : tout vider, forcer sur le disque, retirer le marqueur.
    ///
    /// L'ordre compte. `sync_all` AVANT le retrait du marqueur : si la machine
    /// s'arrête entre les deux, on retrouve un orphelin dont le journal est
    /// complet — un faux positif, sans perte. L'ordre inverse donnerait un
    /// épisode déclaré propre dont les dernières lignes ne seraient jamais
    /// arrivées sur le disque.
    pub fn clore(&mut self) -> std::io::Result<()> {
        self.vider()?;
        self.fichier.sync_all()?;
        let marqueur = self.dossier.join(NOM_MARQUEUR);
        if marqueur.exists() {
            std::fs::remove_file(marqueur)?;
        }
        Ok(())
    }

    pub fn ecrites(&self) -> u64 {
        self.ecrites
    }

    pub fn en_attente(&self) -> usize {
        self.tampon.len()
    }

    /// Le dossier de l'episode — pour que les tests puissent regarder le
    /// disque. La production n'a pas a le connaitre : elle passe par ce type.
    #[cfg(test)]
    pub fn dossier(&self) -> &Path {
        &self.dossier
    }
}

/// Les épisodes que le démarrage précédent n'a pas clos (R3.2).
pub fn orphelins(racine: &Path) -> std::io::Result<Vec<Orphelin>> {
    let mut trouves = Vec::new();
    let Ok(entrees) = std::fs::read_dir(racine) else {
        return Ok(trouves);
    };

    for e in entrees.flatten() {
        let dossier = e.path();
        if !dossier.join(NOM_MARQUEUR).exists() {
            continue;
        }
        let episode_id = dossier
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let marqueur = std::fs::read_to_string(dossier.join(NOM_MARQUEUR))
            .ok()
            .and_then(|t| serde_json::from_str::<Marqueur>(&t).ok());

        let (entrees, ligne_tronquee) = relire(&dossier.join(NOM_JOURNAL))?;
        let dernier_seq = entrees.last().map(EntreeJournal::seq).unwrap_or(0);
        let seq_manquants = seq_manquants(&entrees);
        trouves.push(Orphelin {
            episode_id,
            entrees,
            ligne_tronquee,
            seq_manquants,
            dernier_seq,
            marqueur,
        });
    }
    trouves.sort_by(|a, b| a.episode_id.cmp(&b.episode_id));
    Ok(trouves)
}

/// Relit un journal, en écartant une éventuelle dernière ligne incomplète.
///
/// Une ligne illisible AILLEURS qu'à la fin n'est pas une troncature : c'est une
/// corruption, et elle est signalée de la même façon plutôt qu'ignorée. Dans les
/// deux cas le lecteur doit savoir qu'il manque quelque chose.
fn relire(chemin: &Path) -> std::io::Result<(Vec<EntreeJournal>, bool)> {
    let Ok(fichier) = File::open(chemin) else {
        return Ok((Vec::new(), false));
    };
    let lignes: Vec<String> = BufReader::new(fichier)
        .lines()
        .map_while(Result::ok)
        .collect();

    let mut entrees = Vec::new();
    let mut illisible = false;
    for ligne in &lignes {
        if ligne.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<EntreeJournal>(ligne) {
            Ok(e) => entrees.push(e),
            Err(_) => illisible = true,
        }
    }
    Ok((entrees, illisible))
}

/// Les `seq` absents d'une suite relue (R3.4).
///
/// On ne cherche PAS un compte : on cherche les numéros manquants, parce que
/// savoir qu'il manque trois lignes ne dit pas où, et que le trou doit se situer
/// dans le journal pour être exploitable en revue.
fn seq_manquants(entrees: &[EntreeJournal]) -> Vec<u64> {
    let mut vus: Vec<u64> = entrees.iter().map(EntreeJournal::seq).collect();
    vus.sort_unstable();
    vus.dedup();
    let (Some(&premier), Some(&dernier)) = (vus.first(), vus.last()) else {
        return Vec::new();
    };
    let mut manquants = Vec::new();
    let mut attendu = premier;
    for &s in &vus {
        while attendu < s {
            manquants.push(attendu);
            attendu += 1;
        }
        attendu = s + 1;
    }
    debug_assert!(attendu == dernier + 1);
    manquants
}

/// Clôt un orphelin avec le trou qui explique son interruption (R3.2).
///
/// Le `seq` du gap suit le dernier `seq` sain : la continuité que R3.1 exige ne
/// se rompt pas au moment même où l'on documente une rupture.
pub fn clore_orphelin(racine: &Path, orphelin: &Orphelin) -> std::io::Result<EntreeJournal> {
    let dossier = racine.join(&orphelin.episode_id);
    let derniere_ms = orphelin
        .entrees
        .last()
        .map(EntreeJournal::monotone_ms)
        .unwrap_or(0);

    let gap = EntreeJournal::Gap {
        seq: orphelin.dernier_seq + 1,
        monotone_ms: derniere_ms,
        // `SeqBreak` si une ligne a ete perdue en route, `Crash` sinon : les
        // deux causes ne racontent pas la meme panne et la spec les distingue.
        cause: if orphelin.ligne_tronquee || !orphelin.seq_manquants.is_empty() {
            CauseGap::SeqBreak
        } else {
            CauseGap::Crash
        },
        debut_ms: derniere_ms,
        fin_ms: derniere_ms,
    };

    let mut fichier = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dossier.join(NOM_JOURNAL))?;
    let ligne = serde_json::to_string(&gap)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fichier.write_all(format!("{ligne}\n").as_bytes())?;
    fichier.sync_all()?;

    let marqueur = dossier.join(NOM_MARQUEUR);
    if marqueur.exists() {
        std::fs::remove_file(marqueur)?;
    }
    Ok(gap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horloge::HorlogeSimulee;
    use crate::moteur::Declencheur;
    use std::sync::Arc;
    use std::time::Duration;

    fn racine(nom: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "noe-journal-{nom}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn action(seq: u64) -> EntreeJournal {
        EntreeJournal::Declencheur {
            seq,
            monotone_ms: seq * 10,
            quoi: Declencheur::Soumission,
        }
    }

    fn lignes(dossier: &Path) -> Vec<String> {
        std::fs::read_to_string(dossier.join(NOM_JOURNAL))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn rien_n_atteint_le_disque_avant_le_seuil() {
        let r = racine("seuil");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h, "banc").unwrap();

        for i in 1..=10 {
            j.ecrire(&action(i)).unwrap();
        }
        assert_eq!(j.en_attente(), 10);
        assert!(lignes(j.dossier()).is_empty(), "le tampon ne doit pas fuir");
    }

    #[test]
    fn cent_entrees_declenchent_le_vidage() {
        let r = racine("cent");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h, "banc").unwrap();

        for i in 1..=FLUSH_ENTREES as u64 {
            j.ecrire(&action(i)).unwrap();
        }
        assert_eq!(j.en_attente(), 0, "R3.1 : vidage a {FLUSH_ENTREES} entrees");
        assert_eq!(lignes(j.dossier()).len(), FLUSH_ENTREES);
    }

    #[test]
    fn cinq_secondes_declenchent_le_vidage() {
        let r = racine("cinq-s");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();

        j.ecrire(&action(1)).unwrap();
        h.avancer(Duration::from_millis(FLUSH_MS - 1));
        j.battre().unwrap();
        assert_eq!(j.en_attente(), 1, "pas encore : {} ms", FLUSH_MS - 1);

        h.avancer(Duration::from_millis(1));
        j.battre().unwrap();
        assert_eq!(j.en_attente(), 0, "R3.1 : vidage a {FLUSH_MS} ms");
        assert_eq!(lignes(j.dossier()).len(), 1);
    }

    #[test]
    fn un_tampon_vide_ne_declenche_rien() {
        let r = racine("vide");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        h.avancer(Duration::from_secs(60));
        j.battre().unwrap();
        assert!(lignes(j.dossier()).is_empty());
    }

    #[test]
    fn la_cloture_propre_retire_le_marqueur() {
        let r = racine("cloture");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h, "banc").unwrap();
        j.ecrire(&action(1)).unwrap();

        assert!(r.join("ep1").join(NOM_MARQUEUR).exists(), "marqueur pose");
        j.clore().unwrap();
        assert!(
            !r.join("ep1").join(NOM_MARQUEUR).exists(),
            "marqueur retire"
        );
        assert_eq!(lignes(&r.join("ep1")).len(), 1, "le tampon a bien ete vide");
    }

    #[test]
    fn un_episode_clos_proprement_n_est_pas_un_orphelin() {
        let r = racine("pas-orphelin");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h, "banc").unwrap();
        j.ecrire(&action(1)).unwrap();
        j.clore().unwrap();

        assert!(orphelins(&r).unwrap().is_empty());
    }

    #[test]
    fn un_episode_jamais_clos_est_un_orphelin() {
        let r = racine("orphelin");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        for i in 1..=3 {
            j.ecrire(&action(i)).unwrap();
        }
        // Vidage sans cloture : c'est exactement l'etat qu'un crash laisse.
        h.avancer(Duration::from_millis(FLUSH_MS));
        j.battre().unwrap();
        drop(j);

        let trouves = orphelins(&r).unwrap();
        assert_eq!(trouves.len(), 1);
        assert_eq!(trouves[0].episode_id, "ep1");
        assert_eq!(trouves[0].entrees.len(), 3);
    }

    #[test]
    fn une_derniere_ligne_tronquee_est_ecartee_et_declaree() {
        let r = racine("tronque");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        j.ecrire(&action(1)).unwrap();
        j.ecrire(&action(2)).unwrap();
        h.avancer(Duration::from_millis(FLUSH_MS));
        j.battre().unwrap();
        drop(j);

        // Un crash en plein `write` laisse une ligne coupee au milieu.
        let chemin = r.join("ep1").join(NOM_JOURNAL);
        let mut contenu = std::fs::read_to_string(&chemin).unwrap();
        contenu.push_str("{\"kind\":\"declencheur\",\"seq\":3,\"monot");
        std::fs::write(&chemin, contenu).unwrap();

        let trouves = orphelins(&r).unwrap();
        assert_eq!(trouves[0].entrees.len(), 2, "les lignes saines survivent");
        assert!(
            trouves[0].ligne_tronquee,
            "R3.4 : une discontinuite ne doit JAMAIS passer inapercue"
        );
    }

    #[test]
    fn clore_un_orphelin_ecrit_un_gap_et_retire_le_marqueur() {
        let r = racine("clore-orphelin");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        for i in 1..=3 {
            j.ecrire(&action(i)).unwrap();
        }
        h.avancer(Duration::from_millis(FLUSH_MS));
        j.battre().unwrap();
        drop(j);

        let orphelin = orphelins(&r).unwrap().remove(0);
        let gap = clore_orphelin(&r, &orphelin).unwrap();

        match gap {
            EntreeJournal::Gap { seq, cause, .. } => {
                assert_eq!(cause, CauseGap::Crash);
                assert_eq!(seq, 4, "R3.1 : le seq du gap suit le dernier seq sain");
            }
            autre => panic!("ce n est pas un gap : {autre:?}"),
        }
        assert!(!r.join("ep1").join(NOM_MARQUEUR).exists());
        assert!(orphelins(&r).unwrap().is_empty(), "plus d orphelin apres");
        assert_eq!(lignes(&r.join("ep1")).len(), 4);
    }

    #[test]
    fn une_troncature_donne_seq_break_et_non_crash() {
        // Les deux causes ne racontent pas la meme panne : « crash » dit que le
        // processus est mort, « seq_break » dit qu une ligne manque. Les
        // confondre priverait la revue de l information la plus utile.
        let r = racine("seq-break");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        j.ecrire(&action(1)).unwrap();
        h.avancer(Duration::from_millis(FLUSH_MS));
        j.battre().unwrap();
        drop(j);

        let chemin = r.join("ep1").join(NOM_JOURNAL);
        let mut contenu = std::fs::read_to_string(&chemin).unwrap();
        contenu.push_str("{\"kind\":\"gap\",\"seq\"");
        std::fs::write(&chemin, contenu).unwrap();

        let orphelin = orphelins(&r).unwrap().remove(0);
        match clore_orphelin(&r, &orphelin).unwrap() {
            EntreeJournal::Gap { cause, .. } => assert_eq!(cause, CauseGap::SeqBreak),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn plusieurs_orphelins_sortent_dans_un_ordre_stable() {
        let r = racine("plusieurs");
        let h = Arc::new(HorlogeSimulee::new());
        for id in ["ep3", "ep1", "ep2"] {
            let mut j = Journal::ouvrir(&r, id, h.clone(), "banc").unwrap();
            j.ecrire(&action(1)).unwrap();
            h.avancer(Duration::from_millis(FLUSH_MS));
            j.battre().unwrap();
        }
        let ids: Vec<String> = orphelins(&r)
            .unwrap()
            .into_iter()
            .map(|o| o.episode_id)
            .collect();
        assert_eq!(ids, vec!["ep1", "ep2", "ep3"]);
    }

    #[test]
    fn les_seq_relus_restent_strictement_croissants() {
        let r = racine("seq");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        for i in 1..=250 {
            j.ecrire(&action(i)).unwrap();
        }
        j.clore().unwrap();

        let (entrees, tronquee) = relire(&r.join("ep1").join(NOM_JOURNAL)).unwrap();
        assert!(!tronquee);
        assert_eq!(entrees.len(), 250);
        for paire in entrees.windows(2) {
            assert!(paire[1].seq() > paire[0].seq(), "R3.1");
        }
    }

    // ---------------------------------------------------------------------
    // R3.4 : une discontinuite de seq ne passe jamais inapercue.
    // ---------------------------------------------------------------------

    /// Ecrit un journal a la main, en sautant certains numeros.
    fn journal_trous(r: &Path, id: &str, seqs: &[u64]) {
        let dossier = r.join(id);
        std::fs::create_dir_all(&dossier).unwrap();
        std::fs::write(dossier.join(NOM_MARQUEUR), id).unwrap();
        let contenu: String = seqs
            .iter()
            .map(|s| format!("{}\n", serde_json::to_string(&action(*s)).unwrap()))
            .collect();
        std::fs::write(dossier.join(NOM_JOURNAL), contenu).unwrap();
    }

    #[test]
    fn une_suite_continue_ne_signale_aucun_manque() {
        let r = racine("continue");
        journal_trous(&r, "ep1", &[1, 2, 3, 4, 5]);
        assert!(orphelins(&r).unwrap()[0].seq_manquants.is_empty());
    }

    #[test]
    fn un_seq_saute_est_nomme_precisement() {
        // Savoir qu il manque une ligne ne dit pas laquelle. Le numero, si :
        // c est ce qui permet de situer le trou entre deux evenements connus.
        let r = racine("saute");
        journal_trous(&r, "ep1", &[1, 2, 5, 6]);
        assert_eq!(orphelins(&r).unwrap()[0].seq_manquants, vec![3, 4]);
    }

    #[test]
    fn plusieurs_trous_sont_tous_nommes() {
        let r = racine("plusieurs-trous");
        journal_trous(&r, "ep1", &[1, 3, 4, 8]);
        assert_eq!(orphelins(&r).unwrap()[0].seq_manquants, vec![2, 5, 6, 7]);
    }

    #[test]
    fn un_journal_vide_ne_fabrique_pas_de_manque() {
        let r = racine("vide-seq");
        journal_trous(&r, "ep1", &[]);
        assert!(orphelins(&r).unwrap()[0].seq_manquants.is_empty());
    }

    #[test]
    fn une_seule_entree_ne_fabrique_pas_de_manque() {
        // Le trou se cherche ENTRE la premiere et la derniere : avec une seule
        // ligne, il n y a pas d intervalle, et surtout rien ne dit que le
        // journal aurait du commencer a 1.
        let r = racine("une-seule");
        journal_trous(&r, "ep1", &[7]);
        assert!(orphelins(&r).unwrap()[0].seq_manquants.is_empty());
    }

    #[test]
    fn une_discontinuite_donne_seq_break_meme_sans_ligne_tronquee() {
        let r = racine("disco");
        journal_trous(&r, "ep1", &[1, 2, 9]);
        let orphelin = orphelins(&r).unwrap().remove(0);

        assert!(!orphelin.ligne_tronquee, "aucune ligne n est coupee");
        assert!(!orphelin.seq_manquants.is_empty());

        match clore_orphelin(&r, &orphelin).unwrap() {
            EntreeJournal::Gap { cause, .. } => assert_eq!(
                cause,
                CauseGap::SeqBreak,
                "R3.4 : une ligne disparue proprement reste une discontinuite"
            ),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_journal_ecrit_normalement_n_a_aucune_discontinuite() {
        // Le controle temoin : si celui-ci rougissait, la detection crierait au
        // loup sur chaque episode et on finirait par la debrancher.
        let r = racine("temoin-seq");
        let h = Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep1", h.clone(), "banc").unwrap();
        for i in 1..=150 {
            j.ecrire(&action(i)).unwrap();
        }
        h.avancer(Duration::from_millis(FLUSH_MS));
        j.battre().unwrap();
        drop(j);

        let orphelin = orphelins(&r).unwrap().remove(0);
        assert!(
            orphelin.seq_manquants.is_empty(),
            "faux positif : {:?}",
            orphelin.seq_manquants
        );
        match clore_orphelin(&r, &orphelin).unwrap() {
            EntreeJournal::Gap { cause, .. } => assert_eq!(cause, CauseGap::Crash),
            autre => panic!("{autre:?}"),
        }
    }

    // -- R3.2 : de quoi reprendre un orphelin ------------------------------

    #[test]
    fn le_marqueur_porte_de_quoi_reassembler() {
        // `assembler` a besoin du slug de tache et de la borne murale
        // d'ouverture. Les deux ne vivaient que dans la Session, donc en
        // memoire, donc perdus au crash : un orphelin ne pouvait pas etre
        // assemble, et R3.2 restait a moitie implementee.
        let r = racine("marqueur-identite");
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        h.avancer(Duration::from_millis(1_000));
        let j = Journal::ouvrir(&r, "ep-meta", h.clone(), "maj-crm-post-echange").unwrap();
        drop(j);

        let brut = std::fs::read_to_string(r.join("ep-meta").join(NOM_MARQUEUR)).unwrap();
        let m: Marqueur = serde_json::from_str(&brut).expect("marqueur illisible");
        assert_eq!(m.episode_id, "ep-meta");
        assert_eq!(m.task_slug, "maj-crm-post-echange");
        assert!(m.t0_mural_ms > 0, "la borne murale doit etre reelle");
    }

    #[test]
    fn un_orphelin_rend_son_identite_a_la_reprise() {
        let r = racine("orphelin-identite");
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut j = Journal::ouvrir(&r, "ep-crash", h.clone(), "relance-devis").unwrap();
        j.ecrire(&EntreeJournal::Declencheur {
            seq: 1,
            monotone_ms: 10,
            quoi: Declencheur::Soumission,
        })
        .unwrap();
        j.vider().unwrap();
        // Pas de `clore` : on simule la mort du processus.
        std::mem::forget(j);

        let trouves = orphelins(&r).unwrap();
        assert_eq!(trouves.len(), 1);
        let m = trouves[0].marqueur.as_ref().expect("identite relue");
        assert_eq!(m.task_slug, "relance-devis");
    }

    #[test]
    fn un_marqueur_d_une_version_anterieure_ne_fait_pas_echouer_la_reprise() {
        // Le marqueur ne portait que l'identifiant en clair. Un tel dossier doit
        // rester trouvable et clos — sans identite, donc sans assemblage, mais
        // jamais ignore : un orphelin qu'on n'ouvre pas est un trou silencieux.
        let r = racine("marqueur-ancien");
        let d = r.join("ep-ancien");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(NOM_MARQUEUR), b"ep-ancien").unwrap();
        std::fs::write(d.join(NOM_JOURNAL), b"").unwrap();

        let trouves = orphelins(&r).unwrap();
        assert_eq!(trouves.len(), 1, "l orphelin doit rester trouvable");
        assert!(
            trouves[0].marqueur.is_none(),
            "identite illisible, et c est dit"
        );
        assert!(clore_orphelin(&r, &trouves[0]).is_ok());
        assert!(!d.join(NOM_MARQUEUR).exists());
    }
}
