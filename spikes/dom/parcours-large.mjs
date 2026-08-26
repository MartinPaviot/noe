/**
 * Parcours large — seconde phase du spike DOM.
 *
 * La phase « parcours » rejoue le scénario du spike UIA à l'identique : c'est ce
 * qui rend les deux mondes comparables, et ça reste la mesure de référence. Mais
 * ce scénario ne touche que six contrôles, et une stabilité de 100 % sur une
 * union de quatre signatures ne prouve à peu près rien.
 *
 * Cette phase-ci cherche le volume. Deux versions ont échoué avant celle-ci et
 * les deux échecs valaient la peine :
 *
 *   1. la première survolait les éléments — un survol n'émet ni clic ni
 *      changement, le capteur ne voyait rien ;
 *   2. la seconde visait des sélecteurs Salesforce devinés
 *      (`one-app-nav-bar-item-root`, « Afficher plus d'actions ») dont la
 *      plupart étaient absents de cette org : sept contrôles cliqués sur la
 *      vingtaine espérée.
 *
 * On ne devine donc plus rien : on ÉNUMÈRE ce que la page offre réellement, on
 * garde les contrôles nommés et sûrs dans l'ordre du document, et on clique la
 * même liste à chaque répétition. La liste est recalculée à chaque fois depuis
 * le même état de départ ; si elle variait, la stabilité mesurerait la
 * variation du script et non celle des ancrages — c'est pourquoi elle est
 * journalisée et comparée.
 */
import { dodo, LEAD, log, ORG } from '../occurrence/occurrence.mjs';

const FICHE = `${ORG}/lightning/r/${LEAD}/view`;

/**
 * Inventaire figé, partagé par toutes les promenades.
 *
 * Une version precedente cliquait `cibles.nth(index)` a partir d un inventaire
 * recalcule a chaque tour. Chaque clic modifiant le DOM, l index ne designait
 * deja plus le controle inventorie : les repetitions cliquaient des ensembles
 * differents, et la stabilite mesuree — 76,9 % puis 7,7 % sur deux executions du
 * meme protocole — decrivait la derive du banc, pas celle des ancrages.
 *
 * On cible donc par NOM, et la liste ne bouge plus.
 */
let LISTE = null;

/** Retrouve un controle par son nom accessible, quelle qu en soit la source. */
const parNom = (page, nom) =>
  page
    .getByLabel(nom, { exact: true })
    .or(page.getByTitle(nom, { exact: true }))
    .or(page.getByRole('tab', { name: nom, exact: true }))
    .first();

/**
 * Verbes écartés. La phase doit rester en lecture seule : elle sert à compter
 * des ancrages, pas à écrire dans l'org. Un « Supprimer » cliqué par erreur
 * coûterait la fiche sur laquelle repose tout le banc.
 */
const INTERDITS =
  /supprim|delete|envoy|send|fusion|merge|convert|enregistr|\bsave\b|cr[ée]er|create|nouveau|nouvelle|\bnew\b|d[ée]connex|logout/i;

/** Combien de contrôles au maximum — au-delà, la répétition devient trop longue. */
const PLAFOND = 24;

/**
 * Inventaire des contrôles nommés, sûrs et visibles, dans l'ordre du document.
 *
 * On s'appuie sur le rôle et le nom accessible, jamais sur une classe CSS ou un
 * nom de composant Salesforce : c'est précisément ce que le capteur ancre, donc
 * le banc doit viser la même chose que lui.
 */
async function inventaire(page) {
  const cibles = page.locator('[role="tab"], button[aria-label], button[title], a[href]');
  const nb = Math.min(await cibles.count().catch(() => 0), 120);
  const gardes = [];

  for (let i = 0; i < nb && gardes.length < PLAFOND; i++) {
    const el = cibles.nth(i);
    try {
      if (!(await el.isVisible({ timeout: 500 }))) continue;
      const nom = (
        (await el.getAttribute('aria-label')) ??
        (await el.getAttribute('title')) ??
        (await el.innerText().catch(() => '')) ??
        ''
      )
        .trim()
        .slice(0, 60);
      if (!nom || INTERDITS.test(nom)) continue;
      if (gardes.some((g) => g.nom === nom)) continue;
      gardes.push({ nom });
    } catch {
      /* element parti entre-temps : sans consequence */
    }
  }
  return gardes;
}

export async function parcoursLarge(page, n, N, calibrage = false) {
  log(`promenade ${n + 1}/${N}`);

  await page.goto(FICHE, { waitUntil: 'domcontentloaded' });
  await dodo(3500);

  // L inventaire est fige a la premiere promenade et rejoue tel quel ensuite :
  // recalculer la liste a chaque tour la ferait varier avec l etat de la page,
  // et la stabilite mesurerait cette variation-la.
  if (!LISTE) {
    LISTE = await inventaire(page);
    log(`  inventaire fige : ${LISTE.length} contrôles nommés et sûrs`);
  }
  let cliques = 0;
  let absents = 0;

  const reussis = [];

  for (const g of LISTE) {
    try {
      const el = parNom(page, g.nom);
      await el.scrollIntoViewIfNeeded({ timeout: 3000 });
      await el.click({ timeout: 4000 });
      cliques++;
      reussis.push(g);
      await dodo(350);
      // Referme tout ce qu'un clic aurait pu ouvrir : menu, panneau, modale.
      await page.keyboard.press('Escape').catch(() => {});
      await dodo(250);
      // Un clic a pu naviguer ailleurs ; on revient au point de depart pour que
      // la suite de la liste porte sur la meme page qu'a l'inventaire.
      if (!page.url().includes(LEAD)) {
        await page.goto(FICHE, { waitUntil: 'domcontentloaded' });
        await dodo(2200);
      }
    } catch {
      // Un controle devenu injoignable ne fait pas echouer la promenade : il
      // manquera simplement a l intersection, ce qui est exactement ce que la
      // stabilite doit refleter.
      absents++;
    }
  }

  log(`  ${cliques} contrôles cliqués, ${absents} introuvables`);
  if (calibrage) {
    // Ne garder que ce qui a REELLEMENT repondu. Sans cette passe, la premiere
    // repetition n avait atteint qu un controle sur vingt-quatre, et comme la
    // stabilite est une intersection sur TOUTES les repetitions, cette seule
    // repetition ratee la plafonnait a 1/11 — 9 %. Le chiffre decrivait un
    // ratage du banc, pas la derive des ancrages.
    LISTE = reussis;
    log(`  calibrage : ${LISTE.length} contrôles retenus pour la mesure`);
  }
  return cliques;
}
