/**
 * Sonde 2 — les événements de valeur atteignent-ils une racine shadow branchée ?
 *
 * La sonde 1 a établi que le patch d'`attachShadow` est écrasé par la page, que
 * Salesforce ouvre 270 vraies racines shadow, et que le texte arrive bien dans
 * le `<textarea>` — sans qu'aucun `input` n'atteigne `document`.
 *
 * Une première version de cette sonde posait ses écouteurs sur l'élément
 * focalisé puis tapait via le locator du parcours : les deux ne désignaient pas
 * le même champ, et les trois compteurs sont restés à zéro sans rien prouver.
 *
 * On ne vise donc plus un élément : on branche TOUTES les racines shadow du
 * document, ce qui est exactement la parade qu'une extension de production
 * emploierait puisque le patch d'`attachShadow` ne tient pas. La sonde teste
 * ainsi la solution en même temps que l'hypothèse.
 */
import { chromium, dodo, LEAD, log, ORG } from '../occurrence/occurrence.mjs';

const ctx = await chromium.launchPersistentContext('C:/Users/marti/noe/spikes/occurrence/profil', {
  headless: false,
  channel: 'chrome',
  viewport: null,
  args: ['--start-maximized'],
});
const page = ctx.pages()[0] ?? (await ctx.newPage());
await page.goto(`${ORG}/lightning/r/${LEAD}/view`, { waitUntil: 'domcontentloaded' });
await dodo(6000);

await page.getByRole('tab', { name: 'Détails' }).click({ timeout: 30000 });
await dodo(1500);
const bouton = page
  .locator('button[title*="Statut de la piste"], button[aria-label*="Statut de la piste"]')
  .first();
await bouton.scrollIntoViewIfNeeded();
await bouton.click();
await dodo(2000);

const branchement = await page.evaluate(() => {
  globalThis.__sonde = { rec: [], racines: 0, duree_ms: 0 };
  const s = globalThis.__sonde;

  const noter = (ou) => (e) => {
    s.rec.push({
      ou,
      type: e.type,
      composed: e.composed,
      bubbles: e.bubbles,
      cible: e.target?.tagName ?? '?',
    });
  };

  const vus = new WeakSet();
  const brancher = (racine, ou) => {
    if (!racine || vus.has(racine)) return false;
    vus.add(racine);
    for (const t of ['input', 'change', 'submit']) {
      racine.addEventListener(t, noter(ou), true);
    }
    return true;
  };

  const t0 = performance.now();
  brancher(document, 'document');
  const parcourir = (racine) => {
    for (const el of racine.querySelectorAll('*')) {
      if (el.shadowRoot) {
        if (brancher(el.shadowRoot, 'shadow')) s.racines++;
        parcourir(el.shadowRoot);
      }
    }
  };
  parcourir(document);
  s.duree_ms = performance.now() - t0;

  // Rebalayage a la demande : si Lightning cree de nouvelles racines au moment
  // ou l on entre dans un champ, le premier balayage les a manquees.
  globalThis.__rebalayer = () => {
    const avant = s.racines;
    const d = performance.now();
    parcourir(document);
    return { nouvelles: s.racines - avant, total: s.racines, ms: performance.now() - d };
  };

  return { racines: s.racines, duree_ms: s.duree_ms };
});

log(`racines shadow branchees : ${branchement.racines} en ${branchement.duree_ms.toFixed(1)} ms`);

// Exactement le geste du parcours mesuré, avec le meme locator.
const zone = page.getByLabel('Description').or(page.locator('textarea')).first();
await zone.waitFor({ state: 'visible', timeout: 15000 });
await zone.scrollIntoViewIfNeeded();
await zone.click();
await dodo(800);

// LE test : entrer dans le champ a-t-il fait naitre des racines que le premier
// balayage ne pouvait pas connaitre ?
const rebal = await page.evaluate(() => globalThis.__rebalayer());
log(
  `rebalayage apres le clic : +${rebal.nouvelles} racines (total ${rebal.total}) en ${rebal.ms.toFixed(1)} ms`,
);

await zone.pressSequentially('xy', { delay: 80 });
await dodo(600);
// Un `change` sur un champ texte ne part qu'au blur : on le provoque.
await page.locator('button[name="SaveEdit"]').first().focus();
await dodo(1000);

// TEMOIN. Un compteur a zero a deux lectures opposees : soit la page n emet
// rien, soit la sonde n ecoute rien. Sans temoin on ne peut pas trancher, et
// c est exactement le genre de zero qu on interprete de travers. On emet donc un
// evenement dont on sait qu il devrait etre capte, et on regarde.
const temoin = await page.evaluate(() => {
  let n = document.activeElement;
  while (n?.shadowRoot?.activeElement) n = n.shadowRoot.activeElement;
  const champs = [];
  const collecter = (racine) => {
    for (const el of racine.querySelectorAll('textarea, input')) champs.push(el);
    for (const el of racine.querySelectorAll('*')) if (el.shadowRoot) collecter(el.shadowRoot);
  };
  collecter(document);

  const avant = globalThis.__sonde.rec.length;
  const cible = champs.find((c) => c.tagName === 'TEXTAREA') ?? champs[0];
  cible?.dispatchEvent(new Event('input', { bubbles: true, composed: true }));

  return {
    champs: champs.length,
    valeurActif: (n?.value ?? '').slice(-12),
    tagActif: n?.tagName ?? '?',
    valeurCible: (cible?.value ?? '').slice(-12),
    racineCible: cible ? (cible.getRootNode() === document ? 'document' : 'shadow') : '?',
    temoinCapte: globalThis.__sonde.rec.length > avant,
  };
});

log(
  `champs trouves : ${temoin.champs} · actif=${temoin.tagActif} fin de valeur=${JSON.stringify(temoin.valeurActif)}`,
);
log(
  `cible temoin : racine=${temoin.racineCible} fin de valeur=${JSON.stringify(temoin.valeurCible)}`,
);
log(`TEMOIN capte : ${temoin.temoinCapte}`);

const r = await page.evaluate(() => globalThis.__sonde.rec);
log(`evenements captes : ${r.length}`);
const parOu = {};
for (const e of r) {
  const k = `${e.ou}/${e.type}/composed=${e.composed}/${e.cible}`;
  parOu[k] = (parOu[k] ?? 0) + 1;
}
for (const [k, n] of Object.entries(parOu)) log(`  ${n} x ${k}`);

await ctx.close();
