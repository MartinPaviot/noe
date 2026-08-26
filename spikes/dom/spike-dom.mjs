/**
 * Spike DOM (decisions.md, D20) — mesure les ancrages navigateur.
 *
 * Même protocole que le spike UIA, à la lettre : 5 occurrences scriptées
 * identiques sur l'org de démo, normalisation post-pipeline, stabilité calculée
 * en intersection ÷ union sur les actions d'état résolues. Réutilise le MÊME
 * parcours que le spike UIA (`../occurrence/occurrence.mjs`) : si le parcours
 * différait, l'écart mesuré entre UIA et DOM mélangerait deux causes.
 *
 * ⚠️ Occurrences scriptées — banc capteur, PAS donnée comportementale (D11).
 *
 * La question de D20 : « les ancrages DOM sont-ils plus stables que role+nom
 * UIA ? ». On ne choisit donc pas l'ancrage avant de mesurer — on compare six
 * formules candidates et on laisse les nombres désigner la gagnante.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { chromium, dodo, LEAD, log, ORG, occurrence } from '../occurrence/occurrence.mjs';
import { parcoursLarge } from './parcours-large.mjs';

const ICI = dirname(fileURLToPath(import.meta.url));

const arg = (n, d) => {
  const i = process.argv.indexOf(n);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : d;
};

const N = Number(arg('--occurrences', '5'));
const PROFIL = arg('--profil', 'C:/Users/marti/noe/spikes/occurrence/profil');

/**
 * Deux phases, deux questions.
 *
 * `parcours` rejoue le scénario du spike UIA à l'identique : c'est la mesure
 * comparable, et la seule qui autorise à mettre les deux mondes côte à côte.
 * `large` clique beaucoup plus de contrôles distincts pour que la stabilité
 * porte sur un échantillon qui vaut quelque chose — 100 % sur quatre signatures
 * n'est pas un résultat.
 */
const PHASE = arg('--phase', 'parcours');
const DEST = join(ICI, 'resultats', `spike-dom-${PHASE}.json`);

/** Actions d'état déclarées par occurrence : statut, note, enregistrement. */
const DECLAREES_PARCOURS = 3;

// --- Formules d'ancrage comparées ----------------------------------------
// `nue` reproduit le témoin du spike UIA (rôle + nom brut) : c'est le point de
// comparaison entre les deux mondes. `complet` est l'ancrage enrichi que D20
// décrit : data-* + rôle ARIA + chemin structurel + nom.
const dataDe = (o) =>
  Object.keys(o.data)
    .sort()
    .map((k) => `${k}=${o.data[k]}`)
    .join(',');

const FORMULES = {
  nue: (o) => `${o.role}|${o.nom_brut}`,
  nom: (o) => `${o.role}|${o.nom}`,
  chemin: (o) => `${o.role}|${o.chemin}`,
  data: (o) => `${o.role}|${dataDe(o)}`,
  testid: (o) => `${o.role}|${o.testid}`,
  complet: (o) => `${o.role}|${o.nom}|${dataDe(o)}|${o.chemin}`,
};

/**
 * Stabilité = |∩ des occurrences| ÷ |∪ des occurrences|.
 *
 * Reproduction exacte de `stabilite_par()` du binaire Rust. Une signature qui
 * n'apparaît que dans certaines répétitions n'est pas un point d'ancrage : le
 * dénominateur est donc l'union, pas la moyenne des tailles.
 */
function stabilite(observations, cle) {
  const parOcc = new Map();
  for (const o of observations) {
    if (!o.etat || !o.resolu) continue;
    if (!parOcc.has(o.occurrence)) parOcc.set(o.occurrence, new Set());
    parOcc.get(o.occurrence).add(cle(o));
  }
  if (parOcc.size < 2) return { pct: 0, communes: 0, union: 0 };

  const jeux = [...parOcc.values()];
  let communes = new Set(jeux[0]);
  let union = new Set(jeux[0]);
  for (const j of jeux.slice(1)) {
    communes = new Set([...communes].filter((s) => j.has(s)));
    union = new Set([...union, ...j]);
  }
  if (union.size === 0) return { pct: 0, communes: 0, union: 0 };
  return {
    pct: (communes.size * 100) / union.size,
    communes: communes.size,
    union: union.size,
  };
}

/**
 * Stabilité clé par clé — le diagnostic qui sert vraiment à la spec.
 *
 * Savoir que l'ancrage complet tient à 40 % ne dit pas sur quoi construire.
 * Savoir que `data-target-selection-name` tient à 100 % et
 * `data-aura-rendered-by` à 0 %, si.
 */
function parCleData(observations) {
  const etats = observations.filter((o) => o.etat && o.resolu);
  const compte = new Map();
  for (const o of etats) {
    for (const k of Object.keys(o.data)) compte.set(k, (compte.get(k) ?? 0) + 1);
  }
  const seuil = etats.length * 0.3;
  return [...compte.entries()]
    .filter(([, n]) => n >= seuil)
    .map(([k, n]) => ({
      cle: k,
      presence_pct: (n * 100) / Math.max(1, etats.length),
      stabilite_pct: stabilite(observations, (o) => `${o.role}|${o.nom}|${k}=${o.data[k] ?? ''}`)
        .pct,
    }))
    .sort((a, b) => b.stabilite_pct - a.stabilite_pct)
    .slice(0, 10);
}

async function main() {
  const capture = readFileSync(join(ICI, 'capture.js'), 'utf8');

  const ctx = await chromium.launchPersistentContext(PROFIL, {
    headless: false,
    channel: 'chrome',
    viewport: null,
    args: ['--start-maximized'],
  });

  // Sortie au fil de l'eau, réinstallée par Playwright dans chaque document.
  // C'est ce qui rend la mesure insensible aux navigations : le flux vit ici,
  // dans Node, pas dans une page qui sera rechargée trois fois.
  const flux = [];
  await ctx.exposeFunction('__noePousser', (o) => {
    flux.push(o);
  });

  // Sur le CONTEXTE, donc appliqué à toute page et toute frame, avant tout
  // script de la page. Sur la page seule, les iframes de Lightning passeraient
  // à travers.
  await ctx.addInitScript({ content: capture });

  const page = ctx.pages()[0] ?? (await ctx.newPage());
  await page.goto(`${ORG}/lightning/r/${LEAD}/view`, { waitUntil: 'domcontentloaded' });
  await dodo(4000);

  if (!page.url().includes('/lightning/')) {
    log(`session absente (${page.url().slice(0, 70)}) — lancer connexion.mjs d abord`);
    await ctx.close();
    process.exit(2);
  }
  log('session ouverte, capteur DOM injecte');

  // Diagnostic d'encapsulation, une seule fois. Une racine shadow ouverte est
  // énumérable via `el.shadowRoot` ; une racine FERMÉE rend `null` et reste
  // hors d'atteinte de tout script de page. La proportion des deux décide de ce
  // qu'un capteur DOM peut espérer voir, et c'est le fait qui commande le
  // verdict — pas une opinion sur le framework.
  const encapsulation = await page.evaluate(() => {
    let ouvertes = 0;
    let personnalises = 0;
    let sansRacine = 0;
    const parcourir = (racine) => {
      for (const el of racine.querySelectorAll('*')) {
        if (el.tagName.includes('-')) {
          personnalises++;
          if (el.shadowRoot) {
            ouvertes++;
            parcourir(el.shadowRoot);
          } else {
            sansRacine++;
          }
        }
      }
    };
    parcourir(document);
    return { personnalises, ouvertes, sansRacine };
  });
  log(
    `encapsulation : ${encapsulation.personnalises} elements personnalises, ` +
      `${encapsulation.ouvertes} racines ouvertes, ${encapsulation.sansRacine} sans racine accessible`,
  );

  // Passe de chauffe, NON mesuree, pour la phase large uniquement : elle etablit
  // quels controles repondent vraiment sur cette org, et la mesure ne porte
  // ensuite que sur ceux-la. Calibrer le banc avant de mesurer n est pas un
  // arrangement avec le resultat — c est ce qui empeche une repetition ratee de
  // dicter l intersection.
  if (PHASE === 'large') {
    log('--- chauffe (non mesuree) ---');
    await parcoursLarge(page, -1, N, true);
    flux.splice(0, flux.length);
    log('--- mesure ---');
  }

  const observations = [];
  const parOccurrence = [];
  let coutTotal = 0;
  let ecouleTotal = 0;
  let declarees = 0;

  for (let n = 0; n < N; n++) {
    const departOcc = Date.now();
    try {
      if (PHASE === 'large') {
        declarees += await parcoursLarge(page, n, N);
      } else {
        await occurrence(page, n, N);
        declarees += DECLAREES_PARCOURS;
      }
    } catch (e) {
      log(`  ECHEC occurrence ${n + 1} : ${String(e).split('\n')[0].slice(0, 120)}`);
    }
    // Tout ce que la page a poussé depuis la marque : rien à relire dans le
    // document, donc rien que les navigations puissent emporter.
    const lot = flux.splice(0, flux.length).map((o) => ({
      ...o,
      occurrence: n,
      // « Résolu » au sens UIA : un nom non vide et un rôle qui dit quelque
      // chose. `generic` ne dit rien — c'est le pendant DOM d'« Inconnu ».
      resolu: String(o.nom_brut ?? '').trim().length > 0 && o.role !== 'generic',
    }));
    observations.push(...lot);
    coutTotal += lot.reduce((s, o) => s + (o.cout_ms ?? 0), 0);
    ecouleTotal += Date.now() - departOcc;
    const etats = lot.filter((o) => o.etat).length;
    parOccurrence.push({ occurrence: n, observations: lot.length, actions_etat: etats });
    log(`  recolte : ${lot.length} observations, ${etats} actions d etat`);
    await dodo(1500);
  }

  await ctx.close();

  const etats = observations.filter((o) => o.etat);
  const resolus = etats.filter((o) => o.resolu);

  // Garde-fou contre la stabilité par fusion.
  //
  // Une clé plus grossière paraît TOUJOURS plus stable : en confondant deux
  // contrôles distincts, elle réduit l'union et fait monter le rapport. Le cas
  // extrême est visible à l'œil nu — `testid` sort à 100 % alors qu'aucun
  // élément ne porte de `data-testid` : la formule se réduit à `rôle|` et range
  // tous les boutons de la page dans le même seau.
  //
  // Le témoin est le nombre de contrôles réellement cliqués par répétition : une
  // formule dont l'union descend nettement en dessous n'a pas gagné en
  // stabilité, elle a perdu en pouvoir de distinction. Son chiffre ne compte pas.
  const clicsParOccurrence = N > 0 ? declarees / N : 0;
  const formules = Object.fromEntries(
    Object.entries(FORMULES).map(([nom, f]) => {
      const v = stabilite(observations, f);
      const distincts = new Set(observations.filter((o) => o.etat && o.resolu).map((o) => f(o)))
        .size;
      return [
        nom,
        {
          ...v,
          distincts,
          // 0,9 laisse passer la variation normale du banc, pas une fusion.
          degenere: clicsParOccurrence > 0 && v.union < clicsParOccurrence * 0.9,
        },
      ];
    }),
  );

  // Couverture : observé ÷ déclaré, plafonné à 100 %. Le brut sert à voir le
  // bruit : sur-capturer n'est pas manquer, mais ce n'est pas gratuit non plus.
  const brut = declarees > 0 ? (etats.length * 100) / declarees : 0;

  const resultat = {
    application_cible: 'Salesforce Lightning (org de démo)',
    phase: PHASE,
    date: new Date().toISOString(),
    occurrences: N,
    encapsulation,
    actions_etat_declarees: declarees,
    actions_etat_observees: etats.length,
    observations_totales: observations.length,
    couverture_etat_pct: Math.min(100, brut),
    couverture_brute_pct: brut,
    roles_explicites_pct: etats.length
      ? (etats.filter((o) => o.explicite).length * 100) / etats.length
      : 0,
    resolus_pct: etats.length ? (resolus.length * 100) / etats.length : 0,
    testid_presents_pct: etats.length
      ? (etats.filter((o) => o.testid).length * 100) / etats.length
      : 0,
    surcout_in_page_pct: ecouleTotal > 0 ? (coutTotal * 100) / ecouleTotal : 0,
    cout_total_ms: Math.round(coutTotal),
    ecoule_total_ms: Math.round(ecouleTotal),
    clics_par_occurrence: clicsParOccurrence,
    stabilite: formules,
    cles_data: parCleData(observations),
    par_occurrence: parOccurrence,
  };

  writeFileSync(DEST, JSON.stringify(resultat, null, 2), 'utf8');
  writeFileSync(
    join(ICI, 'resultats', `observations-dom-${PHASE}.json`),
    JSON.stringify(observations, null, 2),
    'utf8',
  );

  log('--- RESULTATS ---');
  for (const [nom, v] of Object.entries(formules)) {
    log(
      `  stabilite ${nom.padEnd(8)} ${v.pct.toFixed(1).padStart(5)} %  (${v.communes}/${v.union} union)${v.degenere ? '  << DEGENERE : fusionne des controles distincts, chiffre non recevable' : ''}`,
    );
  }
  log(`  couverture      ${resultat.couverture_etat_pct.toFixed(1)} % (brut ${brut.toFixed(0)} %)`);
  log(`  surcout in-page ${resultat.surcout_in_page_pct.toFixed(2)} %`);
  log(
    `  roles explicites ${resultat.roles_explicites_pct.toFixed(0)} % · testid ${resultat.testid_presents_pct.toFixed(0)} %`,
  );
  log(`ecrit dans ${DEST}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
