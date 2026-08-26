/**
 * Vérification du capteur DOM sur une page témoin, hors réseau.
 *
 * Le spike coûte cinq occurrences réelles contre une org distante. Un capteur
 * qui se trompe de cible ou qui rend des noms vides gâche le run entier et on ne
 * s'en aperçoit qu'à la fin — c'est exactement ce qui est arrivé deux fois au
 * spike UIA. Cette page témoin reproduit ce que Lightning fait de méchant :
 * shadow DOM, rôle ARIA porté par un `div`, cible de clic enfouie dans un
 * `<span>` nu, `data-*` volatil à côté d'un `data-*` stable.
 *
 * ⚠️ Les interactions passent par les locators Playwright, jamais par des
 * `dispatchEvent` fabriqués. Une première version forçait `composed: true` sur
 * un `change` : le test passait au vert alors que le capteur était aveugle aux
 * changements de valeur en conditions réelles. Un banc d'essai qui fabrique ses
 * propres événements teste sa fabrication, pas le navigateur.
 */
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from '../occurrence/occurrence.mjs';

const ICI = dirname(fileURLToPath(import.meta.url));
const capture = readFileSync(join(ICI, 'capture.js'), 'utf8');

// Le contenu du shadow root voyage dans un <script type="text/plain"> : écrire
// un gabarit JavaScript dans une chaîne JavaScript dans un fichier HTML
// multiplie les niveaux d'échappement sans rien apporter à la mesure.
const INTERIEUR = [
  '<span id="libInterne">Statut de la piste</span>',
  '<div role="button" aria-labelledby="libInterne" data-stable="statut-piste"',
  '     data-aura-rendered-by="931:0;a"><span>ouvrir</span></div>',
  '<div role="link" aria-labelledby="lib" data-stable="hors-portee">replier</div>',
  '<select aria-label="Priorité (3)" data-stable="prio">',
  '  <option value="haute">haute</option><option value="basse">basse</option>',
  '</select>',
  '<textarea aria-label="Note du 12/03/2026"></textarea>',
].join('\n');

const PAGE = [
  '<!doctype html><meta charset="utf-8"><title>temoin</title>',
  '<span id="lib">Statut de la piste</span>',
  '<div id="hote"></div>',
  `<script id="graine" type="text/plain">${INTERIEUR}</script>`,
  '<script>',
  "  const racine = document.getElementById('hote').attachShadow({ mode: 'open' });",
  "  racine.innerHTML = document.getElementById('graine').textContent;",
  '</script>',
].join('\n');

const faits = [];
const echecs = [];
const verifier = (nom, condition, detail) => {
  faits.push(nom);
  if (!condition) echecs.push(`${nom} — ${detail}`);
};

// Même moteur que le spike réel : vérifier le capteur sur un autre binaire ne
// prouverait rien sur celui qui portera la mesure.
const ctx = await chromium.launchPersistentContext('', { headless: true, channel: 'chrome' });
await ctx.addInitScript({ content: capture });
const page = await ctx.newPage();

// PAS setContent : il remplace le document et emporte avec lui les écouteurs
// posés par le script d'init. Une vraie navigation rejoue l'injection, donc
// reproduit ce qui se passe en conditions réelles.
const fichier = join(mkdtempSync(join(tmpdir(), 'noe-temoin-')), 'temoin.html');
writeFileSync(fichier, PAGE, 'utf8');
await page.goto(pathToFileURL(fichier).href, { waitUntil: 'domcontentloaded' });

// Les locators Playwright traversent le shadow DOM et produisent de vrais
// événements de navigateur — c'est tout l'intérêt.
await page.locator('[role=button] span').click();
await page.locator('[role=link]').click();
await page.locator('select').selectOption('basse');
await page.locator('textarea').pressSequentially('ab');

const lot = await page.evaluate(() => globalThis.__noeCapture.lire());
await ctx.close();

const clics = lot.obs.filter((o) => o.type === 'click');
const clic = clics.find((o) => o.role === 'button');
const lien = clics.find((o) => o.role === 'link');
const chg = lot.obs.find((o) => o.type === 'change');
const saisies = lot.obs.filter((o) => o.type === 'input');

verifier('le clic est capté', !!clic, 'aucun click de rôle button');
verifier(
  'la cible traverse le shadow DOM',
  clic?.chemin?.startsWith('div['),
  `chemin=${clic?.chemin}`,
);
verifier(
  'le rôle ARIA explicite gagne',
  clic?.role === 'button' && clic?.explicite === true,
  `role=${clic?.role} explicite=${clic?.explicite}`,
);

// Le clic a été porté par le <span> intérieur : sans remontée, le capteur
// enregistrerait un `generic` sans nom, et l'action serait perdue.
verifier(
  'la cible brute est bien le span nu',
  clic?.cible_role === 'generic',
  `cible_role=${clic?.cible_role}`,
);
verifier(
  "l'acteur est résolu en remontant le chemin composé",
  clic?.remontees >= 1,
  `remontees=${clic?.remontees}`,
);

verifier(
  'aria-labelledby est résolu dans le même arbre',
  clic?.nom_brut === 'Statut de la piste',
  `nom_brut=${JSON.stringify(clic?.nom_brut)}`,
);

// Une IDREF est résolue dans l'arbre de l'élément, jamais au-dessus : un
// aria-labelledby qui pointe vers le DOM clair depuis un shadow root ne résout
// PAS, pour un lecteur d'écran comme pour nous. On retombe donc sur le texte.
// Ce n'est pas un défaut du capteur : c'est la raison pour laquelle tant de
// composants Lightning n'ont pas de nom accessible exploitable de l'extérieur.
verifier(
  "une IDREF hors de l'arbre ne résout pas, et on retombe sur le texte",
  lien?.nom_brut === 'replier',
  `nom_brut=${JSON.stringify(lien?.nom_brut)}`,
);

verifier(
  "le clic sur rôle actionnable est une action d'état",
  clic?.etat === true,
  `etat=${clic?.etat}`,
);
verifier(
  'les data-* sont collectés',
  clic?.data?.['data-stable'] === 'statut-piste',
  `data=${JSON.stringify(clic?.data)}`,
);

// RÉSULTAT, pas bug. Les motifs partagés sont réglés sur des noms lisibles par
// un humain ; un identifiant de rendu Aura (« 931:0;a ») ne ressemble à aucun
// d'eux et traverse la normalisation intact. Il restera donc différent à chaque
// re-rendu et empoisonnera tout ancrage qui l'inclut. C'est précisément ce que
// l'analyse clé par clé du spike doit faire apparaître — on ne le rustine pas
// ici : truquer la normalisation pour embellir un chiffre viderait la mesure.
verifier(
  "l'identifiant de rendu Aura survit à la normalisation (ancrage empoisonné)",
  clic?.data?.['data-aura-rendered-by'] === '931:0;a',
  `valeur=${JSON.stringify(clic?.data?.['data-aura-rendered-by'])}`,
);

// LE test qui compte : `change` est spécifié `composed: false` et meurt à la
// frontière de sa racine shadow. Sans l'instrumentation d'attachShadow, il
// n'atteint jamais `document` et le capteur ne voit aucun changement de valeur.
verifier(
  'le change franchit la frontière shadow via attachShadow',
  !!chg,
  'aucune observation de type change',
);
verifier("le change est une action d'état", chg?.etat === true, `etat=${chg?.etat}`);
verifier(
  'le compteur entre parenthèses est normalisé',
  chg?.nom === 'priorité n',
  `nom=${JSON.stringify(chg?.nom)}`,
);

verifier(
  'la saisie est captée frappe par frappe',
  saisies.filter((o) => o.role === 'textbox').length === 2,
  `input textbox=${saisies.filter((o) => o.role === 'textbox').length}`,
);

// Changer la valeur d'un <select> émet `input` PUIS `change` — les deux, dans
// cet ordre. Compter les `input` sans distinguer leur origine ferait donc
// apparaître une frappe fantôme à chaque choix dans une liste.
verifier(
  'un choix dans une liste émet input ET change',
  saisies.filter((o) => o.role === 'combobox').length === 1,
  `input combobox=${saisies.filter((o) => o.role === 'combobox').length}`,
);
verifier(
  "la saisie n'est PAS une action d'état",
  saisies.every((o) => o.etat === false),
  'un input compté comme etat',
);

// `click` est composed:true : il remonte jusqu'à document ET traverse la racine
// shadow branchée. Sans le garde, il serait compté deux fois.
verifier(
  "un événement composé n'est compté qu'une fois",
  clics.length === 2,
  `clics=${clics.length}`,
);

verifier(
  'le coût in-page est mesuré',
  lot.cout_ms > 0 && lot.ecoule_ms > 0,
  `cout=${lot.cout_ms} ecoule=${lot.ecoule_ms}`,
);

console.log(`\n${faits.length - echecs.length}/${faits.length} verifications passent`);
if (echecs.length) {
  console.error('\nECHECS :');
  for (const e of echecs) console.error(`  x ${e}`);
  process.exit(1);
}
console.log('capteur DOM conforme — le spike peut tourner.');
