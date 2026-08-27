/**
 * Sonde l'org de démo : est-elle vivante, et que contient-elle ?
 *
 * Première étape de la tâche 0. On ne peuple rien tant qu'on ne sait pas ce qui
 * est déjà là — une org créée hier peut avoir expiré, ou contenir des données de
 * démonstration Salesforce qu'il vaut mieux connaître avant d'en ajouter.
 *
 * Usage : `node scripts/terrain/sonder.mjs [--visible]`
 */
import { ouvrirCoffre } from './coffre.mjs';
import { api, ouvrirSession } from './session.mjs';

const COFFRE = 'C:\\Users\\marti\\.noe\\coffre\\salesforce-de.dpapi';
const VERSION_API = 'v62.0';

const coffre = ouvrirCoffre(COFFRE);
console.log(`org        ${coffre.url}`);
console.log(`compte     ${coffre.utilisateur}`);

let session;
try {
  session = await ouvrirSession(coffre, { visible: process.argv.includes('--visible') });
} catch (e) {
  console.error(`\nECHEC DE CONNEXION : ${e.message}`);
  process.exit(1);
}

try {
  console.log(`instance   ${session.instance}`);
  console.log(`session    ouverte\n`);

  const limites = await api(session, `/services/data/${VERSION_API}/limits`);
  if (!limites.ok) {
    console.error(`l API refuse le jeton de session : ${limites.statut}`);
    console.error(JSON.stringify(limites.corps).slice(0, 300));
    process.exit(1);
  }
  const appels = limites.corps?.DailyApiRequests;
  console.log(
    `API REST   ok — ${appels?.Remaining ?? '?'} / ${appels?.Max ?? '?'} appels restants`,
  );

  // Ce qui existe deja, objet par objet.
  for (const objet of ['Lead', 'Contact', 'Account', 'Opportunity']) {
    const r = await api(
      session,
      `/services/data/${VERSION_API}/query?q=${encodeURIComponent(`SELECT COUNT() FROM ${objet}`)}`,
    );
    console.log(
      `  ${objet.padEnd(12)} ${r.ok ? `${r.corps.totalSize} enregistrement(s)` : `refuse (${r.statut})`}`,
    );
  }

  // Les champs du Lead, pour choisir les `scope_fields` de `terrain.json`.
  const d = await api(session, `/services/data/${VERSION_API}/sobjects/Lead/describe`);
  if (d.ok) {
    const modifiables = d.corps.fields
      .filter((f) => f.updateable && !f.deprecatedAndHidden)
      .map((f) => f.name);
    console.log(`\nLead : ${modifiables.length} champs modifiables`);
    console.log(
      `  interessants : ${modifiables.filter((n) => /Status|Rating|Industry|Description|Company|Title/.test(n)).join(', ')}`,
    );
  }
} finally {
  await session.fermer();
}
