/**
 * Tâche 12 — les canaris sur une capture RÉELLE.
 *
 * Le sweep de la spec 001 vérifie qu'aucun canari ne ressort d'un **rejeu** du
 * corpus doré. C'est nécessaire et ce n'est pas suffisant : le corpus doré est
 * écrit à la main, donc il ne prouve rien sur ce que la *capture* laisse passer.
 *
 * Ce banc-ci fait l'autre moitié. Il saisit les quatre formes interdites de
 * `canaris.json` dans un vrai formulaire, à travers un vrai navigateur, un vrai
 * pont et une vraie capture — puis on regarde l'épisode produit. Si une seule
 * ressort en clair, la rédaction a échoué là où elle compte : sur le chemin
 * qu'empruntent les vraies données.
 *
 * Les canaris entrent par **trois portes**, parce qu'elles ne se rédactent pas
 * au même endroit :
 *
 * 1. la **valeur** d'un champ — que le capteur ne doit jamais lire ;
 * 2. le **nom accessible** d'un contrôle — que le rédacteur doit tokeniser ;
 * 3. le **titre du document** — qui remonte par les noms de fenêtre.
 *
 * Usage : `node canaris.mjs` (Chrome, extension et page sont lancés ici).
 */
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const ICI = dirname(fileURLToPath(import.meta.url));
const RACINE = resolve(ICI, '..', '..');
const EXTENSION = resolve(ICI, '..', 'extension').replace(/\\/g, '/');
const PAGE = `http://127.0.0.1:${process.env['NOE_PAGE'] ?? 4180}/`;

const { interdites } = JSON.parse(
  readFileSync(join(RACINE, 'packages', 'harness', 'golden', 'canaris.json'), 'utf8'),
);
const CANARIS = interdites.chaines;
console.log(`${CANARIS.length} canaris interdits a saisir :`, CANARIS.join(' · '));

const profil = mkdtempSync(join(tmpdir(), 'noe-canaris-'));
const PORT = Number(process.env['NOE_CDP'] ?? 9337);
const CHROME = process.env['NOE_CHROME'] ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';

const chrome = spawn(
  CHROME,
  [
    `--user-data-dir=${profil}`,
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
const session = await nav.newBrowserCDPSession();
const { id } = await session.send('Extensions.loadUnpacked', { path: EXTENSION });
console.log(`extension chargee : ${id}`);
await attendre(1500);

for (const [i, canari] of CANARIS.entries()) {
  await page.goto(PAGE, { waitUntil: 'load' });
  await attendre(900);

  // Porte 1 : la VALEUR d'un champ. Le capteur ne doit jamais la lire.
  const description = page.locator('textarea#d');
  await description.click();
  await description.fill(`Contact ${canari} a rappeler`);
  await description.blur();
  await attendre(400);

  // Porte 2 : le NOM ACCESSIBLE d'un controle. Le redacteur doit le tokeniser.
  // Porte 3 : le TITRE du document, qui remonte par les noms de fenetre.
  await page.evaluate(
    ([c, n]) => {
      document.title = `Fiche ${c} — banc ${n}`;
      const bouton = document.querySelector('button[aria-label="Ajouter une note"]');
      if (bouton) bouton.setAttribute('aria-label', `Rappeler ${c}`);
    },
    [canari, i + 1],
  );
  await attendre(500);

  await page.locator(`button[aria-label="Rappeler ${canari}"]`).click();
  await attendre(300);
  await page.locator('button[data-label="Enregistrer"]').click();
  await attendre(500);

  console.log(`  canari ${i + 1}/${CANARIS.length} saisi : ${canari}`);
}

console.log('SAISIE TERMINEE');
await nav.close();
chrome.kill();
try {
  rmSync(profil, { recursive: true, force: true });
} catch {
  // Le dossier temporaire du systeme sait s'en occuper.
}
