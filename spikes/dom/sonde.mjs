/**
 * Sonde de diagnostic — pourquoi le capteur ne voit ni `input` ni `change`.
 *
 * Le spike rend 100 % de stabilité sur une union de quatre signatures, et six
 * observations par occurrence : exactement les six clics du script. La saisie
 * d'une quarantaine de caractères ne produit aucun événement. Un chiffre parfait
 * obtenu sur un échantillon qui manque la moitié des interactions n'est pas un
 * résultat, c'est un artefact — cette sonde va chercher la cause avant que quoi
 * que ce soit ne soit consigné.
 *
 * On vérifie trois hypothèses, dans l'ordre de coût :
 *   1. le patch d'attachShadow tient-il encore après le chargement de la page ?
 *   2. Lightning utilise-t-il le shadow DOM synthétique (polyfill LWC) ?
 *   3. la saisie atteint-elle vraiment un champ éditable ?
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, dodo, LEAD, log, ORG } from '../occurrence/occurrence.mjs';

const ICI = dirname(fileURLToPath(import.meta.url));
const capture = readFileSync(join(ICI, 'capture.js'), 'utf8');

const ctx = await chromium.launchPersistentContext('C:/Users/marti/noe/spikes/occurrence/profil', {
  headless: false,
  channel: 'chrome',
  viewport: null,
  args: ['--start-maximized'],
});

// On marque la fonction native AVANT d'injecter le capteur, pour pouvoir dire
// ensuite qui, de nous ou de la page, a eu le dernier mot.
await ctx.addInitScript({
  content: `window.__attachShadowOrigine = Element.prototype.attachShadow;`,
});
await ctx.addInitScript({ content: capture });

const page = ctx.pages()[0] ?? (await ctx.newPage());
await page.goto(`${ORG}/lightning/r/${LEAD}/view`, { waitUntil: 'domcontentloaded' });
await dodo(6000);

const etat = await page.evaluate(() => ({
  patchTient:
    Element.prototype.attachShadow.name === 'attachShadow' &&
    Element.prototype.attachShadow !== globalThis.__attachShadowOrigine,
  syntheticShadow:
    typeof globalThis.$A !== 'undefined' || !!document.querySelector('[data-aura-rendered-by]'),
  lwcPresent:
    !!globalThis.LWC || !!globalThis.$lwcResetAlreadyLoaded || !!document.querySelector('[lwc-]'),
  // Combien de vraies racines shadow existent dans le document ?
  vraiesRacines: (() => {
    let n = 0;
    const parcourir = (r) => {
      for (const el of r.querySelectorAll('*')) {
        if (el.shadowRoot) {
          n++;
          parcourir(el.shadowRoot);
        }
      }
    };
    parcourir(document);
    return n;
  })(),
  frames: window.frames.length,
}));

log(`patch attachShadow tient : ${etat.patchTient}`);
log(`shadow synthetique (Aura) : ${etat.syntheticShadow}`);
log(`LWC present               : ${etat.lwcPresent}`);
log(`vraies racines shadow     : ${etat.vraiesRacines}`);
log(`frames                    : ${etat.frames}`);

// --- La saisie atteint-elle un champ editable ? -------------------------
await page.getByRole('tab', { name: 'Détails' }).click({ timeout: 30000 });
await dodo(1500);

const bouton = page
  .locator('button[title*="Statut de la piste"], button[aria-label*="Statut de la piste"]')
  .first();
await bouton.scrollIntoViewIfNeeded();
await bouton.click();
await dodo(1500);

await page.evaluate(() => globalThis.__noeCapture.vider());

const zone = page.getByLabel('Description').or(page.locator('textarea')).first();
await zone.waitFor({ state: 'visible', timeout: 15000 });
await zone.click();
await zone.pressSequentially('abc', { delay: 60 });
await dodo(800);

const apres = await page.evaluate(() => {
  const a = document.activeElement;
  const lot = globalThis.__noeCapture.lire();
  return {
    actifTag: a?.tagName,
    actifRacine: a?.shadowRoot ? 'a un shadowRoot' : 'aucun',
    actifValeur: (a?.value ?? '').slice(0, 20),
    // La cible reelle du focus traverse le shadow DOM.
    profondTag: (() => {
      let n = document.activeElement;
      while (n?.shadowRoot?.activeElement) n = n.shadowRoot.activeElement;
      return n?.tagName;
    })(),
    profondValeur: (() => {
      let n = document.activeElement;
      while (n?.shadowRoot?.activeElement) n = n.shadowRoot.activeElement;
      return (n?.value ?? '').slice(0, 20);
    })(),
    types: lot.obs.map((o) => `${o.type}:${o.role}`),
  };
});

log(
  `activeElement (document) : ${apres.actifTag} (${apres.actifRacine}) valeur=${JSON.stringify(apres.actifValeur)}`,
);
log(`activeElement (profond)  : ${apres.profondTag} valeur=${JSON.stringify(apres.profondValeur)}`);
log(`observations apres saisie : ${JSON.stringify(apres.types)}`);

await ctx.close();
