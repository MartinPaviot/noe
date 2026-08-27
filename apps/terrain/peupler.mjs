/**
 * Peuple l'org de démo, plante les canaris, et écrit `terrain.json`.
 *
 * Deuxième étape de la tâche 0, après `sonder.mjs`. Ce script n'invente rien :
 * il exécute le plan de `plan.mjs`, qui est pur et vérifié par dix tests. Tout ce
 * qui se décide est là-bas ; ici il n'y a que des appels et des constats.
 *
 * ## Rejouable
 *
 * Chaque enregistrement porte une **clé de dédoublonnage** — un nom de société,
 * une adresse — et le script la cherche avant de créer. Un second passage ne
 * crée rien : c'est ce qui permet de le relancer après une coupure sans se
 * demander ce qui a déjà été fait.
 *
 * ## Ce qu'il ne peut pas faire, et qu'il dit
 *
 * **L'historique des champs ne s'active pas par l'API REST.** Il se règle dans
 * Setup, objet par objet, vingt champs au maximum. Le script le VÉRIFIE — il
 * provoque un changement, relit `LeadHistory`, et regarde si la ligne est là —
 * puis nomme ce qui manque. Il ne fait pas semblant : rien dans l'API de
 * description ne dit si un champ est suivi, donc le seul contrôle honnête est
 * l'expérience.
 *
 * Un historique manquant n'est pas un détail. Une liste vide ressemble à « rien
 * n'a changé » alors qu'elle veut dire « je ne sais pas », et les deux mènent à
 * des conclusions opposées : la première autorise un `state_before` reconstitué,
 * la seconde impose `unknown_before`.
 *
 * Usage : `node peupler.mjs [--verifier-seulement] [--visible]`
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { ouvrirCoffre } from './coffre.mjs';
import { plan } from './plan.mjs';
import { api, ouvrirSession } from './session.mjs';

const COFFRE = join(homedir(), '.noe', 'coffre', 'salesforce-de.dpapi');
const TERRAIN = join(homedir(), '.noe', 'terrain.json');
const VERSION_API = 'v62.0';

const verifierSeulement = process.argv.includes('--verifier-seulement');

/** Échappe une valeur SOQL. Mêmes règles que l'adaptateur Rust. */
function echapper(valeur) {
  return String(valeur).replace(/\\/g, '\\\\').replace(/'/g, "\\'");
}

/** L'identifiant d'un enregistrement déjà présent, s'il y en a un. */
async function dejaLa(session, { objet, cle }) {
  const soql = `SELECT Id FROM ${objet} WHERE ${cle.champ} = '${echapper(cle.valeur)}' LIMIT 2`;
  const r = await api(session, `/services/data/${VERSION_API}/query?q=${encodeURIComponent(soql)}`);
  if (!r.ok) throw new Error(`recherche ${objet} refusee : ${r.statut} ${JSON.stringify(r.corps)}`);
  const trouves = r.corps?.records ?? [];
  // Deux enregistrements pour une clé de dédoublonnage : le terrain est déjà
  // sale, et créer un troisième n'arrangerait rien. On s'arrête et on le dit.
  if (trouves.length > 1) {
    throw new Error(`${objet} ${cle.valeur} existe en double — terrain a nettoyer a la main`);
  }
  return trouves[0]?.Id ?? null;
}

/** Crée un enregistrement et rend son identifiant. */
async function creer(session, { objet, champs }) {
  const r = await api(session, `/services/data/${VERSION_API}/sobjects/${objet}`, {
    method: 'POST',
    body: JSON.stringify(champs),
  });
  if (!r.ok) throw new Error(`creation ${objet} refusee : ${r.statut} ${JSON.stringify(r.corps)}`);
  return r.corps.id;
}

/**
 * Vérifie que l'historique d'un champ est bien suivi, en le provoquant.
 *
 * Rien dans `/describe` ne dit si un champ est suivi. Le seul contrôle honnête
 * est donc l'expérience : on change la valeur, on relit l'historique, et on
 * regarde. On remet ensuite la valeur d'origine — un terrain qu'on sonde ne doit
 * pas rester modifié par la sonde.
 */
async function historiqueSuivi(session, idPiste, champ, valeurActuelle, valeurTest) {
  const chemin = `/services/data/${VERSION_API}/sobjects/Lead/${idPiste}`;
  const ecrire = (v) =>
    api(session, chemin, { method: 'PATCH', body: JSON.stringify({ [champ]: v }) });

  const aller = await ecrire(valeurTest);
  if (!aller.ok) return { suivi: false, cause: `ecriture refusee : ${aller.statut}` };

  const soql =
    `SELECT Field FROM LeadHistory WHERE LeadId = '${echapper(idPiste)}' ` +
    `AND Field = '${echapper(champ)}' LIMIT 1`;
  const lu = await api(
    session,
    `/services/data/${VERSION_API}/query?q=${encodeURIComponent(soql)}`,
  );

  await ecrire(valeurActuelle);

  if (!lu.ok) return { suivi: false, cause: `historique illisible : ${lu.statut}` };
  return { suivi: (lu.corps?.records ?? []).length > 0, cause: 'historique vide' };
}

// ---------------------------------------------------------------------------

const p = plan();
const coffre = ouvrirCoffre(COFFRE);
console.log(`org        ${coffre.url}`);
console.log(`compte     ${coffre.utilisateur}`);
console.log(`plan       ${p.enregistrements.length} enregistrements, ${p.canaris.length} canaris`);

const session = await ouvrirSession(coffre, { visible: process.argv.includes('--visible') });
console.log(`instance   ${session.instance}\n`);

const parObjet = {};
let crees = 0;
let deja = 0;

/** Note l'identifiant d'un enregistrement, par objet. */
function retenir(objet, id) {
  if (parObjet[objet] === undefined) parObjet[objet] = [];
  parObjet[objet].push(id);
}

for (const enregistrement of p.enregistrements) {
  const existant = await dejaLa(session, enregistrement);
  if (existant !== null) {
    deja += 1;
    retenir(enregistrement.objet, existant);
    continue;
  }
  if (verifierSeulement) {
    console.log(`MANQUE     ${enregistrement.objet} ${enregistrement.cle.valeur}`);
    continue;
  }
  const id = await creer(session, enregistrement);
  crees += 1;
  retenir(enregistrement.objet, id);
  console.log(`cree       ${enregistrement.objet} ${enregistrement.cle.valeur} → ${id}`);
}

console.log(`\npeuplement ${crees} crees, ${deja} deja presents`);

// -- L'historique, verifie et jamais suppose --------------------------------

const piste = parObjet['Lead']?.[0];
const manquants = [];
if (piste === undefined) {
  console.log('historique non verifie : aucune piste sous la main');
} else {
  const original = p.enregistrements.find((e) => e.objet === 'Lead')?.champs ?? {};
  for (const champ of p.historique_requis.Lead) {
    const actuelle = original[champ] ?? null;
    const test = champ === 'Rating' ? 'Warm' : `${actuelle ?? ''} (sonde)`.trim();
    const { suivi, cause } = await historiqueSuivi(session, piste, champ, actuelle, test);
    console.log(`historique ${champ.padEnd(12)} ${suivi ? 'suivi' : `NON SUIVI (${cause})`}`);
    if (!suivi) manquants.push(champ);
  }
}

if (manquants.length > 0) {
  console.log(
    `\nA FAIRE DANS SETUP — activer le suivi d historique sur Lead pour : ${manquants.join(', ')}.` +
      `\nSans lui, LeadHistory rend une liste VIDE, qui ressemble a « rien n a change »` +
      `\nalors qu elle veut dire « je ne sais pas ». Les deux menent a des conclusions opposees.`,
  );
}

// -- terrain.json ------------------------------------------------------------

if (!verifierSeulement) {
  mkdirSync(dirname(TERRAIN), { recursive: true });
  writeFileSync(TERRAIN, `${JSON.stringify(p.terrain, null, 2)}\n`, 'utf8');
  console.log(`\nterrain    ${TERRAIN} ecrit`);
  console.log('           il ne porte AUCUN secret : les jetons vivent sous DPAPI.');
}

console.log(
  manquants.length === 0
    ? '\nTerrain pret.'
    : `\nTerrain incomplet : ${manquants.length} champ(s) sans historique.`,
);
process.exit(manquants.length === 0 ? 0 : 1);
