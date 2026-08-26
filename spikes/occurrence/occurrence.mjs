/**
 * Occurrence scriptée — banc capteur, PAS donnée comportementale (decisions.md, D11).
 *
 * Rejoue la même séquence à l'identique, autant de fois que demandé. C'est
 * précisément la reproductibilité qui rend les deux stratégies d'abonnement
 * comparables : si un humain rejouait, sa variance entre les deux phases se
 * mélangerait à l'effet mesuré.
 *
 * Le parcours imite un vrai « maj-crm-post-echange » : ouvrir la fiche, changer
 * le statut, saisir une note, enregistrer — avec les pauses humaines que les
 * déclencheurs de la spec 002 attendent (2 s d'inactivité après saisie, une
 * bascule d'onglet).
 *
 * Après chaque occurrence, le script écrit lui-même la borne dans le fichier de
 * contrôle du binaire de mesure : une seule commande pilote toute la phase.
 */
import { appendFileSync } from 'node:fs';
import { chromium } from 'playwright';

const arg = (n, d) => {
  const i = process.argv.indexOf(n);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : d;
};

const PROFIL = arg('--profil', 'C:/Users/marti/noe/spikes/occurrence/profil');
const CONTROLE = arg('--controle', '');
const N = Number(arg('--occurrences', '5'));
const LEAD = arg('--lead', '00Qaj00000uTdnJEAS');
const ORG = 'https://orgfarm-7d442f390a-dev-ed.develop.lightning.force.com';
const UTILISATEUR = 'contact+noespike.09cd56be5bda@agentforce.com';
const MDP = process.env.SF_MDP ?? '';

/**
 * UN SEUL statut, volontairement.
 *
 * La v1 cyclait trois valeurs : les options cliquées différaient donc d'une
 * occurrence à l'autre, et la stabilité mesurait la variété du script au lieu de
 * la stabilité des éléments. Le critère (a) du prompt maître demande si un MÊME
 * élément interagi garde un rôle+nom stable d'une occurrence à l'autre — il faut
 * donc que le parcours soit identique.
 *
 * La note, elle, change à chaque passage : l'enregistrement reste une vraie
 * écriture, et le déclencheur « saisie + 2 s » se produit bien.
 */
const STATUTS = ['Working - Contacted'];

const dodo = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (m) => console.log(`[${new Date().toISOString().slice(11, 19)}] ${m}`);

/**
 * Clique un bouton par son aria-label, en traversant le shadow DOM.
 *
 * Le défilement préalable n'est pas cosmétique : « Modifier Description » vit en
 * bas d'une fiche longue, et sans lui le clic expire.
 */
async function cliquerParLabel(page, motif, timeout = 20000) {
  const bouton = page.locator(`button[title*="${motif}"], button[aria-label*="${motif}"]`).first();
  await bouton.waitFor({ state: 'attached', timeout });
  await bouton.scrollIntoViewIfNeeded({ timeout });
  await dodo(300);
  await bouton.click({ timeout });
}

async function occurrence(page, n) {
  const statut = STATUTS[n % STATUTS.length];
  log(`occurrence ${n + 1}/${N} — statut visé « ${statut} »`);

  // 1. Ouvrir la fiche.
  await page.goto(`${ORG}/lightning/r/${LEAD}/view`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('tab', { name: 'Détails' }).click({ timeout: 30000 });
  await dodo(1200);

  // 2. Changer le statut — edition en ligne.
  await cliquerParLabel(page, 'Statut de la piste');
  await dodo(600);
  const combo = page.locator('button[role="combobox"], lightning-combobox button').first();
  await combo.click({ timeout: 15000 });
  await dodo(400);
  await page.getByRole('option', { name: statut }).first().click({ timeout: 15000 });

  // 3. Saisir une note, puis 2 s d'inactivite — declencheur « saisie + pause ».
  //
  // Pas de clic sur « Modifier Description » : cliquer « Modifier Statut » a
  // bascule TOUT le panneau en edition, les boutons individuels ont disparu et
  // les champs sont deja editables. On vise donc la zone de texte directement.
  try {
    const zone = page.getByLabel('Description').or(page.locator('textarea')).first();
    await zone.waitFor({ state: 'visible', timeout: 15000 });
    await zone.scrollIntoViewIfNeeded();
    await dodo(300);
    await zone.click();
    await zone.pressSequentially(`Echange du jour, suite donnee. Passage ${n + 1}.`, { delay: 45 });
    log('  saisie faite, 2 s d inactivite (declencheur spec 002)');
    await dodo(2000);
  } catch (e) {
    // Un echec silencieux ici fausserait la couverture sans qu'on le sache.
    log(`  note NON saisie — ${String(e).split('\n')[0].slice(0, 110)}`);
  }

  // 4. Bascule d'onglet avec retour < 60 s — autre declencheur de la spec 002.
  const second = await page.context().newPage();
  await second.goto('about:blank');
  await dodo(900);
  await second.close();
  await page.bringToFront();
  await dodo(600);

  // 5. Enregistrer.
  await page.locator('button[name="SaveEdit"]').first().click({ timeout: 20000 });
  await dodo(2500);
  log(`  enregistre`);
}

async function main() {
  const ctx = await chromium.launchPersistentContext(PROFIL, {
    headless: false,
    channel: 'chrome',
    viewport: null,
    args: ['--start-maximized'],
  });
  const page = ctx.pages()[0] ?? (await ctx.newPage());

  // Connexion, une seule fois, HORS des occurrences mesurees.
  await page.goto(`${ORG}/lightning/page/home`, { waitUntil: 'domcontentloaded' });
  await dodo(4000);

  if (!page.url().includes('/lightning/')) {
    log(`session absente (url : ${page.url().slice(0, 70)}), connexion...`);
    if (!MDP) {
      console.error('SF_MDP absent de l environnement.');
      await ctx.close();
      process.exit(2);
    }
    // Le formulaire vit sur le domaine my.salesforce.com, pas sur lightning.
    await page.goto('https://orgfarm-7d442f390a-dev-ed.develop.my.salesforce.com/', {
      waitUntil: 'domcontentloaded',
    });
    // Connexion en DEUX temps : l identifiant d abord, le mot de passe ensuite.
    // Le champ #password n existe pas tant que l identifiant n est pas soumis.
    await page.waitForSelector('#username', { timeout: 45000 });
    await page.fill('#username', UTILISATEUR);
    await page.click('#Login');
    log('  identifiant soumis, attente du champ mot de passe...');
    await page.waitForSelector('#password', { state: 'visible', timeout: 60000 });
    await page.fill('#password', MDP);
    await page.click('#Login');
    await page.waitForURL(/lightning/, { timeout: 90000 });
    log('connecte');
  } else {
    log('session deja ouverte');
  }
  await dodo(2000);

  for (let n = 0; n < N; n++) {
    try {
      await occurrence(page, n);
    } catch (e) {
      log(`  ECHEC occurrence ${n + 1} : ${String(e).slice(0, 120)}`);
    }
    // 3 actions d'etat par occurrence : statut, note, enregistrement.
    if (CONTROLE) appendFileSync(CONTROLE, 'fait 3\n', 'utf8');
    await dodo(1500);
  }

  log('phase terminee');
  await ctx.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
