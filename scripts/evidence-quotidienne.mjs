#!/usr/bin/env node
/**
 * L'evidence quotidienne (decisions.md, D26).
 *
 * « Le fondateur doit pouvoir VOIR l'avancement, pas le lire. » Huit specs de
 * tests verts ne montrent rien ; ce script produit chaque jour une image de ce
 * que le produit est **réellement** devenu.
 *
 * Deux contraintes commandent tout ce fichier :
 *
 * 1. **Jamais l'écran, seulement le produit.** Le dépôt est public. Une capture
 *    plein écran y publierait le bureau de l'opérateur — courriels ouverts, noms
 *    de clients dans la barre des tâches. Ce serait la fuite que la première
 *    règle du projet interdit, commise par l'outil censé la prévenir. Le script
 *    ne sait donc composer que des pixels appartenant au produit, et il n'a
 *    aucun moyen de photographier autre chose.
 * 2. **Automatisée.** Une preuve visuelle qui dépend de la discipline de
 *    quelqu'un cesse d'exister en trois semaines.
 *
 * Tant que le squelette traversant (tâche 8bis) n'existe pas, l'image montre ce
 * que le produit possède : ses trois icônes de barre d'état, et l'état vérifié
 * de son avancement. Le jour où la fenêtre naît, ce script capture cette
 * fenêtre-là.
 */
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const RACINE = join(ICI, '..');
const DEST = join(RACINE, 'docs', 'evidence', 'daily');
const ICONES = join(RACINE, 'apps', 'desktop', 'src-tauri', 'icons');

const arg = (n, d) => {
  const i = process.argv.indexOf(n);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : d;
};

// Jour LOCAL, pas UTC.
//
// `toISOString` rend la date en temps universel : passe une certaine heure du
// soir, la preuve du jour serait classee la veille. D26 parle de « journee de
// build », c est-a-dire celle de l operateur.
const jour = (() => {
  const d = new Date();
  const p = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
})();
const sujet = arg('--sujet', 'etat');

/** Lit une source de vérité du dépôt, jamais une valeur recopiée à la main. */
const lire = (chemin) => readFileSync(join(RACINE, chemin), 'utf8');

/** Les tâches et leur état, lues dans `tasks.md` — jamais recopiées. */
function avancementSpec002() {
  const t = lire('specs/002-capture-bornee/tasks.md');
  const taches = [...t.matchAll(/^- \[( |x)\] \*\*(.+?)\*\*/gm)].map((m) => ({
    faite: m[1] === 'x',
    // Le titre porte parfois un marqueur d'amendement en code : il n'apporte
    // rien à une vignette, et il la ferait déborder.
    titre: m[2]
      .replace(/`[^`]*`\s*/g, '')
      .replace(/\s+/g, ' ')
      .trim(),
  }));
  return { taches, faites: taches.filter((x) => x.faite).length, total: taches.length };
}

/** Les derniers arbitrages, pour montrer ce qui a bougé et pas seulement combien. */
function dernieresDecisions(n) {
  return [...lire('docs/decisions.md').matchAll(/^## \d{4}-\d{2}-\d{2} — (D\d+) : (.+)$/gm)]
    .slice(-n)
    .map((m) => ({ code: m[1], quoi: m[2] }));
}

/**
 * Compte les tests en les LISANT, jamais en les supposant.
 *
 * Un chiffre recopié à la main dans une image quotidienne devient faux au
 * troisième jour, et une preuve fausse est pire qu'une preuve absente.
 *
 * La première version supposait quand même deux choses, et les deux étaient
 * fausses. Elle **énumérait les fichiers à la main** — sept modules ajoutés
 * depuis n'y figuraient pas — et elle comptait `fn nom() {`, c'est-à-dire toute
 * fonction sans argument, helpers de banc compris. Elle annonçait 148 quand la
 * suite en comptait 247 : une preuve fausse, exactement ce que son propre
 * commentaire interdisait.
 *
 * On balaie donc l'arborescence, et on compte l'attribut `#[test]`, qui est la
 * seule marque qu'un test soit un test.
 */
function compterTests() {
  const sources = [];
  const balayer = (dossier) => {
    for (const e of readdirSync(dossier, { withFileTypes: true })) {
      const chemin = join(dossier, e.name);
      if (e.isDirectory()) balayer(chemin);
      else if (e.name.endsWith('.rs')) sources.push(chemin);
    }
  };
  for (const racine of ['apps/desktop/src-tauri/src', 'apps/desktop/src-tauri/tests']) {
    try {
      balayer(join(RACINE, racine));
    } catch {
      // Un dossier absent n'est pas une panne : il vaut zéro test.
    }
  }
  const rust = sources.reduce(
    (n, f) => n + [...readFileSync(f, 'utf8').matchAll(/^\s*#\[test\]/gm)].length,
    0,
  );
  return { rust };
}

const enBase64 = (nom) => readFileSync(join(ICONES, nom)).toString('base64');

const ETATS = [
  ['tray-observe.png', 'observe', 'un episode court', '#2e9e5b'],
  ['tray-pause.png', 'pause', 'rien n est capture', '#d98a1f'],
  ['tray-question.png', 'question', 'une reponse attend', '#3b7fd4'],
];

const { taches, faites, total } = avancementSpec002();
const decisions = dernieresDecisions(3);
const { rust } = compterTests();

const page = `<!doctype html><meta charset="utf-8"><title>Noe — ${jour}</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body {
    margin: 0; width: 1280px; height: 800px;
    background: #14161a; color: #e8eaed;
    font: 15px/1.5 "Segoe UI", system-ui, sans-serif;
    padding: 44px 56px 36px; display: flex; flex-direction: column; gap: 26px;
  }
  h1 { margin: 0; font-size: 30px; font-weight: 650; letter-spacing: -0.4px; }
  .date { color: #8b929d; font-size: 15px; margin-top: 6px; }
  .titre { font-size: 12px; letter-spacing: 1.4px; text-transform: uppercase;
           color: #8b929d; margin-bottom: 13px; }
  .icones { display: flex; gap: 44px; }
  .etat { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
  .rangee { display: flex; align-items: flex-end; gap: 14px; }
  .rangee img { image-rendering: pixelated; }
  .nom { font-weight: 600; font-size: 16px; }
  .quoi { color: #8b929d; font-size: 13px; }
  .taches { display: flex; flex-wrap: wrap; gap: 8px; max-width: 1130px; }
  .t { padding: 6px 13px; border-radius: 999px; font-size: 13px;
       border: 1px solid #2b2f36; color: #8b929d; }
  .t.ok { border-color: #2e9e5b; color: #7fd6a4; background: #16281f; }
  .decisions { display: flex; flex-direction: column; gap: 9px; }
  .d { display: flex; gap: 14px; align-items: baseline; }
  .d b { color: #7fd6a4; font-weight: 600; min-width: 34px; font-size: 14px; }
  .d span { color: #b6bcc6; font-size: 14px; }
  .chiffres { display: flex; gap: 52px; margin-top: auto; align-items: flex-start;
              border-top: 1px solid #23262c; padding-top: 22px; }
  .c b { display: block; font-size: 28px; font-weight: 650; letter-spacing: -0.6px;
          line-height: 1.15; }
  .c span { color: #8b929d; font-size: 13px; }
  .note { color: #6b7280; font-size: 12px; }
</style>
<div>
  <h1>Noe — l'esprit qui comprend le travail avant de le faire</h1>
  <div class="date">Etat du ${jour} · spec 002, la capture bornee</div>
</div>

<div>
  <div class="titre">Ce que le produit montre aujourd'hui — barre d'etat</div>
  <div class="icones">
    ${ETATS.map(
      ([f, nom, quoi, couleur]) => `
    <div class="etat">
      <div class="rangee">
        <img src="data:image/png;base64,${enBase64(f)}" width="32" height="32" alt="">
        <img src="data:image/png;base64,${enBase64(f)}" width="64" height="64" alt="">
      </div>
      <div class="nom" style="color:${couleur}">${nom}</div>
      <div class="quoi">${quoi}</div>
    </div>`,
    ).join('')}
  </div>
</div>

<div>
  <div class="titre">Spec 002 — ${faites} taches sur ${total}</div>
  <div class="taches">
    ${taches.map((t) => `<div class="t${t.faite ? ' ok' : ''}">${t.titre}</div>`).join('')}
  </div>
</div>

<div>
  <div class="titre">Derniers arbitrages</div>
  <div class="decisions">
    ${decisions.map((d) => `<div class="d"><b>${d.code}</b><span>${d.quoi}</span></div>`).join('')}
  </div>
</div>

<div class="chiffres">
  <div class="c"><b>${rust}</b><span>tests Rust</span></div>
  <div class="c"><b>${faites}/${total}</b><span>taches spec 002</span></div>
  <div class="c"><b>0</b><span>episode capture</span></div>
  <div class="c" style="margin-left:auto">
    <span class="note">Aucune fenetre produit avant la tache 8bis.<br>
    Cette image ne montre que des pixels du produit — jamais l'ecran (D26).</span>
  </div>
</div>`;

mkdirSync(DEST, { recursive: true });
const html = join(DEST, `.${jour}-${sujet}.html`);
writeFileSync(html, page, 'utf8');

const sortie = join(DEST, `${jour}-${sujet}.png`);
const sortieVue = join(DEST, `${jour}-vue.png`);
const pilote = join(RACINE, 'spikes', 'occurrence', 'node_modules', 'playwright', 'index.mjs');

const { chromium } = await import(pathToFileURL(pilote).href);
const navigateur = await chromium.launch({ channel: 'chrome' });
const page2 = await navigateur.newPage({ viewport: { width: 1280, height: 800 } });
await page2.goto(pathToFileURL(html).href, { waitUntil: 'load' });
await page2.screenshot({ path: sortie });

/**
 * La vue produit elle-même (D26).
 *
 * Depuis la tâche 8bis, il existe des pixels de produit à montrer. On les
 * capture depuis le build servi par un serveur local : chargée en `file://`,
 * la page ne peut pas lire ses fixtures — le navigateur y bloque `fetch`.
 *
 * Sur les **fixtures versionnées**, jamais sur les épisodes du poste : une image
 * quotidienne prise sur des données réelles les publierait dans un dépôt public,
 * ce que la première règle du projet interdit. C'est donc la vue telle qu'elle
 * est, avec des données qui ne sont celles de personne.
 */
const APP = join(RACINE, 'apps', 'desktop');
let serveur;
try {
  readFileSync(join(APP, 'dist', 'index.html'));
  serveur = spawn('pnpm', ['exec', 'vite', 'preview', '--port', '4174', '--strictPort'], {
    cwd: APP,
    shell: true,
    stdio: 'ignore',
  });

  const p3 = await navigateur.newPage({ viewport: { width: 1280, height: 800 } });
  // Le serveur met un instant à écouter ; on réessaie plutôt que de dormir une
  // durée devinée.
  let servie = false;
  for (let essai = 0; essai < 25 && !servie; essai++) {
    try {
      await p3.goto('http://localhost:4174/', { waitUntil: 'load', timeout: 1_500 });
      servie = true;
    } catch {
      await new Promise((r) => setTimeout(r, 400));
    }
  }
  if (!servie) throw new Error('le serveur de previsualisation n a pas repondu');

  await p3.waitForSelector('#app[data-pret="oui"]', { timeout: 10_000 });
  await p3.screenshot({ path: sortieVue, fullPage: true });
  console.log(`  ${sortieVue.replace(RACINE, '.')}`);
} catch (e) {
  console.log(`  (vue non capturee : ${e instanceof Error ? e.message : e})`);
} finally {
  // `kill()` ne suffit pas sous Windows : `shell: true` interpose un
  // interpreteur, et tuer celui-ci laisse le serveur vivant — il ecoutait
  // encore apres plusieurs executions, et faisait echouer les tests visuels du
  // port voisin. On tue l arbre.
  if (serveur?.pid) {
    try {
      execFileSync('taskkill', ['/pid', String(serveur.pid), '/T', '/F'], {
        stdio: 'ignore',
      });
    } catch {
      serveur.kill();
    }
  }
}

await navigateur.close();

// Le HTML intermediaire ne sert qu'au rendu : le committer ferait deux sources
// pour une seule verite.
execFileSync(process.execPath, ['-e', `require('fs').unlinkSync(${JSON.stringify(html)})`]);

console.log(`  ${sortie.replace(RACINE, '.')}`);
console.log(`  spec 002 : ${faites}/${total} · ${rust} tests Rust`);
