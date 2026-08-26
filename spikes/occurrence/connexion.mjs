/**
 * Connexion initiale, une seule fois, dans le profil persistant.
 *
 * Salesforce exige une vérification d'identité depuis un navigateur inconnu :
 * un code est envoyé par courriel. Le script ne peut pas lire la boîte ; il
 * s'arrête donc et attend qu'on dépose le code dans un fichier. L'orchestrateur
 * lit le courriel et écrit le fichier — aucune intervention humaine.
 *
 * Une fois la session établie dans le profil, `occurrence.mjs` la réutilise et
 * ne repasse plus jamais par là : la connexion reste HORS des mesures.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { chromium } from 'playwright';

const PROFIL = 'C:/Users/marti/noe/spikes/occurrence/profil';
const CODE_FICHIER = 'C:/Users/marti/noe/spikes/occurrence/code.txt';
const ORG_MY = 'https://orgfarm-7d442f390a-dev-ed.develop.my.salesforce.com';
const UTILISATEUR = 'contact+noespike.09cd56be5bda@agentforce.com';
const MDP = process.env.SF_MDP ?? '';

const dodo = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (m) => console.log(`[${new Date().toISOString().slice(11, 19)}] ${m}`);

/** Attend que l'orchestrateur dépose le code. Le fichier est vidé après lecture. */
async function attendreCode(max = 180) {
  writeFileSync(CODE_FICHIER, '', 'utf8');
  log('EN_ATTENTE_DU_CODE');
  for (let i = 0; i < max * 2; i++) {
    if (existsSync(CODE_FICHIER)) {
      const v = readFileSync(CODE_FICHIER, 'utf8').trim();
      if (/^\d{5,8}$/.test(v)) {
        writeFileSync(CODE_FICHIER, '', 'utf8');
        return v;
      }
    }
    await dodo(500);
  }
  throw new Error('aucun code recu dans le delai');
}

const ctx = await chromium.launchPersistentContext(PROFIL, {
  headless: false,
  channel: 'chrome',
  viewport: null,
  args: ['--start-maximized'],
});
const page = ctx.pages()[0] ?? (await ctx.newPage());

await page.goto(ORG_MY, { waitUntil: 'domcontentloaded' });
await dodo(3000);

if (page.url().includes('/lightning/')) {
  log('DEJA_CONNECTE');
  await ctx.close();
  process.exit(0);
}

// Etape 1 : identifiant.
await page.waitForSelector('#username', { timeout: 45000 });
await page.fill('#username', UTILISATEUR);
await page.click('#Login');

// Etape 2 : mot de passe.
await page.waitForSelector('#password', { state: 'visible', timeout: 60000 });
await page.fill('#password', MDP);
await page.click('#Login');
await dodo(6000);

// Etape 3 : verification d identite, si demandee.
const champCode = page.locator('#emc, input[name="emc"], #smc, input[id*="verification"]').first();
if ((await champCode.count()) > 0) {
  log(`verification demandee (url : ${page.url().slice(0, 60)})`);

  // « Ne plus demander sur ce navigateur » : evite de refaire ca a chaque phase.
  const memoriser = page.locator('input[type="checkbox"]').first();
  if ((await memoriser.count()) > 0) {
    try {
      await memoriser.check({ timeout: 5000 });
      log('  navigateur memorise');
    } catch {
      /* case absente ou non cochable — sans conséquence */
    }
  }

  const code = await attendreCode();
  log(`code recu (${code.length} chiffres), saisie...`);
  await champCode.fill(code);
  await page.locator('#save, input[type="submit"], button[type="submit"]').first().click();
}

await page.waitForURL(/lightning/, { timeout: 90000 });
log('CONNECTE');
await dodo(3000);
await ctx.close();
