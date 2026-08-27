/**
 * Le banc du pont DOM : cinq répétitions, de bout en bout, en une commande.
 *
 * La tâche 6c de la spec 002 demande de démontrer **la capture d'un changement
 * de valeur, sur cinq répétitions**. C'est le trou que le spike DOM a
 * explicitement laissé : il n'avait pas observé d'`input`/`change`, sa saisie
 * n'atteignait pas le champ visé, et il a refusé de conclure.
 *
 * Le banc regarde ce qui sort **au bout du tuyau nommé** — pas ce que la page
 * croit avoir émis. Entre les deux il y a le service worker, l'hôte de native
 * messaging et le protocole de Chrome, et c'est précisément là que les défauts
 * se logent.
 *
 * ## Pourquoi il lance Chrome lui-même
 *
 * Trois pièges, tous rencontrés, tous coûteux :
 *
 * 1. `--load-extension` est **ignoré depuis Chrome 137**. La voie qui reste est
 *    la méthode CDP `Extensions.loadUnpacked`, qui exige
 *    `--enable-unsafe-extension-debugging`.
 * 2. Une extension chargée par cette méthode **ne survit pas au redémarrage** du
 *    navigateur. Un banc qui suppose qu'elle est encore là mesure le vide et
 *    l'appelle un échec de capture.
 * 3. Un script de contenu ne s'injecte **pas** dans un onglet déjà ouvert : il
 *    s'injecte à la navigation, donc après le chargement de l'extension.
 *
 * D'où : un profil neuf, Chrome lancé ici, extension chargée, PUIS navigation.
 * Aucune étape n'est laissée à l'état d'une exécution précédente.
 *
 * Usage : `pnpm --filter @noe/extension-banc banc`
 */
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const ICI = dirname(fileURLToPath(import.meta.url));
const EXTENSION = resolve(ICI, '..', 'extension').replace(/\\/g, '/');
const PAGE = `http://127.0.0.1:${process.env['NOE_PAGE'] ?? 4180}/`;
/**
 * Le nombre de répétitions, cinq par défaut.
 *
 * La première version écrivait `Number(argv[i + 1] || 5)`. Quand l'option est
 * absente, `indexOf` rend `-1`, `argv[0]` est le chemin de Node — une chaîne
 * **vraie** — et le `|| 5` ne se déclenche jamais : `Number` rendait `NaN`, et
 * `for (n = 1; n <= NaN; …)` ne tournait pas une seule fois. Le banc annonçait
 * « PILOTAGE TERMINE » sans avoir rien piloté, et l'absence d'observations se
 * lisait comme un échec de capture. Une heure perdue à chercher un défaut qui
 * n'existait pas.
 */
const REPETITIONS = (() => {
  const i = process.argv.indexOf('--repetitions');
  const n = i < 0 ? 5 : Number(process.argv[i + 1]);
  if (!Number.isInteger(n) || n < 1) {
    throw new Error(`--repetitions doit etre un entier positif, recu ${process.argv[i + 1]}`);
  }
  return n;
})();

const profil = mkdtempSync(join(tmpdir(), 'noe-banc-'));
const PORT = Number(process.env['NOE_CDP'] ?? 9336);
const CHROME = process.env['NOE_CHROME'] ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';

// Chrome est lancé ICI et pas par Playwright : `Extensions.loadUnpacked` n'est
// exposé qu'à une session CDP de NIVEAU NAVIGATEUR, et un contexte persistant
// de Playwright n'en offre pas — `ctx.browser()` y est nul. La méthode répond
// alors « Method not available », ce qui se lit comme une absence de support
// alors que c'est une absence de session.
const chrome = spawn(
  CHROME,
  [
    `--user-data-dir=${profil}`,
    // Sans lui, `Extensions.loadUnpacked` est refusé.
    '--enable-unsafe-extension-debugging',
    '--no-first-run',
    '--no-default-browser-check',
    `--remote-debugging-port=${PORT}`,
    'about:blank',
  ],
  { detached: false, stdio: 'ignore' },
);

const attendre = (ms) => new Promise((f) => setTimeout(f, ms));
await attendre(2500);

const nav = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
const ctx = nav.contexts()[0];
const page = ctx.pages()[0] ?? (await ctx.newPage());

// L'extension AVANT toute navigation vers la page mesurée.
const session = await nav.newBrowserCDPSession();
const { id } = await session.send('Extensions.loadUnpacked', { path: EXTENSION });
console.log(`extension chargee : ${id}`);
// Le service worker doit avoir ouvert son pont : c'est lui qui fait démarrer
// l'hôte, donc qui rend le tuyau joignable.
await page.waitForTimeout(1500);

/**
 * Une répétition complète du protocole.
 *
 * La page est rechargée à chaque tour, et la valeur saisie **change** : remplir
 * un champ avec ce qu'il contient déjà n'émet aucun `change`, et le banc
 * mesurerait alors sa propre inertie.
 */
async function repetition(n) {
  await page.goto(PAGE, { waitUntil: 'load' });
  // Le contrôle tardif apparaît à 400 ms : sans cette attente on mesurerait
  // l'absence de rebalayage au lieu du rebalayage.
  await page.waitForTimeout(900);

  await page.locator('button[aria-label="Ajouter une note"]').click();
  await page.locator('button[data-label="Modifier"]').click();

  const description = page.locator('textarea#d');
  await description.click();
  await description.fill(`Relance apres echange ${n}`);
  await description.blur();
  await page.waitForTimeout(400);

  await page.locator('select#s').selectOption(n % 2 === 0 ? 'qualifie' : 'perdu');
  await page.waitForTimeout(300);

  await page.locator('button[data-label="Enregistrer"]').click();
  await page.waitForTimeout(200);
  await page.locator('button[aria-label="Contrôle tardif"]').click();
  await page.waitForTimeout(500);
}

for (let n = 1; n <= REPETITIONS; n += 1) {
  await repetition(n);
  console.log(`repetition ${n}/${REPETITIONS}`);
}

console.log('PILOTAGE TERMINE');
await nav.close();
chrome.kill();
try {
  rmSync(profil, { recursive: true, force: true });
} catch {
  // Un profil qui résiste n'est pas un échec du banc : il est dans le dossier
  // temporaire du système, qui sait s'en occuper.
}
