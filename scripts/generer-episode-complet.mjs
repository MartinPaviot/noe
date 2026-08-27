#!/usr/bin/env node
/**
 * Un épisode qui exerce **tous** les champs du schéma, pour le miroir Rust.
 *
 * Le corpus doré vérifie déjà beaucoup : le type Rust le lit, n'en perd aucun
 * champ, et le juge Rust y recalcule les mêmes grades. Mais un contrôle ne vaut
 * que par ce que ses vecteurs exercent, et le corpus doré n'a **jamais** porté
 * `resolved` ni `state_meta` — les deux champs d'entité ajoutés le 2026-08-27.
 * Une divergence sur eux serait passée sans qu'aucun test ne rougisse.
 *
 * Ce fichier est donc l'épisode « tout allumé » : chaque champ optionnel
 * présent, chaque variante d'événement représentée. Il n'est pas un épisode
 * plausible et n'essaie pas de l'être — il n'a qu'un travail, montrer chaque
 * champ au moins une fois.
 *
 * ## Le générateur REFUSE de produire un épisode incomplet
 *
 * C'est le cœur du dispositif. Les clés déclarées par les schémas Zod sont
 * énumérées, et si l'épisode n'en couvre pas une, le script échoue. Le jour où
 * quelqu'un ajoute un champ au schéma, c'est ici que ça s'arrête — et pas six
 * mois plus tard, sur un épisode réel que le harness refuse.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const RACINE = join(ICI, '..');
const DEST = join(RACINE, 'packages', 'episode-spec', 'episode-complet.json');

const verifier = process.argv.includes('--verifier');

const { Entity, Episode, Event, SCHEMA_V } = await import(
  pathToFileURL(join(RACINE, 'packages', 'episode-spec', 'dist', 'index.js')).href
);

const T0 = '2026-01-01T09:00:00.000Z';
const T1 = '2026-01-01T09:05:00.000Z';

const episode = {
  schema_v: SCHEMA_V,
  id: '01JQ0000000000000000000000',
  task_slug: 'miroir-complet',
  t0: T0,
  t1: T1,
  events: [
    {
      schema_v: SCHEMA_V,
      seq: 1,
      ts: T0,
      kind: 'ui_action',
      source: 'ui',
      action: 'input',
      target: { role: 'textbox', name: 'CIBLE_aaaaaaaaaaaaa', region: 'formulaire' },
      payload: 'PII_bbbbbbbbbbbbb',
    },
    {
      schema_v: SCHEMA_V,
      seq: 2,
      ts: '2026-01-01T09:01:00.000Z',
      kind: 'api_change',
      source: 'api',
      connector: 'salesforce',
      object: 'Lead',
      object_id: '0035g00000LmT4EAAV',
      fields_changed: ['Status'],
    },
    {
      schema_v: SCHEMA_V,
      seq: 3,
      ts: '2026-01-01T09:02:00.000Z',
      kind: 'gap',
      source: 'system',
      gap: { cause: 'seq_break', from_seq: 2, to_seq: 3 },
    },
    {
      schema_v: SCHEMA_V,
      seq: 4,
      ts: '2026-01-01T09:03:00.000Z',
      kind: 'degraded',
      source: 'system',
      degraded: { what: 'snapshots', from: 'nominal', to: 'suspendus' },
    },
    {
      schema_v: SCHEMA_V,
      seq: 5,
      ts: T1,
      kind: 'ui_action',
      source: 'ui',
      action: 'submit',
      target: { role: 'button', name: 'Enregistrer' },
    },
  ],
  entities: [
    {
      key: { type: 'salesforce', value_pseudo: 'CIBLE_aaaaaaaaaaaaa' },
      first_seen_seq: 1,
      api_refs: [{ connector: 'salesforce', object: 'Lead', id: '0035g00000LmT4EAAV' }],
      state_before: { Status: 'Open - Not Contacted', Rating: null },
      state_after: { Status: 'Working - Contacted', Rating: 'Hot' },
      resolved: { by: 'system_id', at: T0 },
      state_meta: {
        Description: { unknown_before: true, reason: 'champ non historise par le systeme' },
        Rating: { reconstituted: true, reason: 'reconstitue depuis l historique' },
      },
    },
  ],
  // Le schema VERIFIE lui-meme que le grade declare est celui que les regles
  // donnent. Cet episode porte un trou, donc il est B — et vouloir l'ecrire A
  // pour faire joli s'est fait refuser tout de suite, ce qui est le bon signe.
  grade: 'B',
  grade_reason: 'declasse en B : 1 trou de capture',
  scope_fields: ['Status', 'Rating'],
  completeness: { explained: 2, out_of_scope: 0, gaps: 1 },
  // INVARIANT IV : un episode cloture n'est jamais modifie ; une correction
  // produit un episode neuf qui reference l'ancien. Le miroir Rust ne portait
  // pas ce champ, donc une lecture-ecriture l'aurait efface en silence.
  supersedes: '01JQ1111111111111111111111',
};

/** Les clés qu'un schéma Zod objet déclare. */
function clesDeclarees(schema) {
  const forme = schema._def?.shape ?? schema.shape;
  const resolu = typeof forme === 'function' ? forme() : forme;
  return Object.keys(resolu ?? {});
}

/**
 * Refuse un épisode qui n'exerce pas exactement ce que le schéma déclare.
 *
 * **Dans les deux sens, et le second n'est pas décoratif.** Les objets Zod ne
 * sont pas stricts par défaut : une clé en trop passe la validation sans un mot.
 * J'ai inventé un `degraded.reason` qui n'existe nulle part, Zod l'a accepté, et
 * c'est le miroir Rust qui l'a signalé — en le comptant comme un champ perdu, ce
 * qu'il était, mais pour la mauvaise raison. Un vecteur qui contient ce que
 * personne ne déclare fait chercher un défaut là où il n'y en a pas.
 */
function verifierCouverture() {
  const manques = [];
  const surplus = [];

  const comparer = (prefixe, objet, schema) => {
    const vues = new Set(Object.keys(objet));
    const declarees = new Set(clesDeclarees(schema));
    for (const cle of declarees) if (!vues.has(cle)) manques.push(`${prefixe}.${cle}`);
    for (const cle of vues) if (!declarees.has(cle)) surplus.push(`${prefixe}.${cle}`);
    return vues.size;
  };

  const clesEpisode = comparer('episode', episode, Episode._def?.schema ?? Episode);
  const clesEntite = comparer('entity', episode.entities[0], Entity);

  // Chaque variante d'événement doit être représentée au moins une fois — c'est
  // ce qui a manqué pour `api_change`, absent du miroir Rust pendant deux specs —
  // et chaque événement écrit ici doit correspondre exactement à sa variante.
  const genresVus = new Set(episode.events.map((e) => e.kind));
  const variantes = new Map((Event._def.options ?? []).map((v) => [v.shape.kind.value, v]));
  for (const genre of variantes.keys()) {
    if (!genresVus.has(genre)) manques.push(`event.kind=${genre}`);
  }
  // Les MANQUES se comptent par genre et pas par evenement : un champ optionnel
  // qu'un seul evenement porte est exerce, et l'exiger de tous ferait ecrire des
  // evenements absurdes pour satisfaire le controle. Les SURPLUS, eux, se
  // comptent evenement par evenement : une cle de trop est de trop partout.
  const vuesParGenre = new Map();
  for (const [i, ev] of episode.events.entries()) {
    const variante = variantes.get(ev.kind);
    if (variante === undefined) {
      surplus.push(`events[${i}].kind=${ev.kind}`);
      continue;
    }
    const declarees = new Set(clesDeclarees(variante));
    const cumul = vuesParGenre.get(ev.kind) ?? new Set();
    for (const cle of Object.keys(ev)) {
      cumul.add(cle);
      if (!declarees.has(cle)) surplus.push(`events[${i}].${cle}`);
    }
    vuesParGenre.set(ev.kind, cumul);
  }
  for (const [genre, variante] of variantes) {
    const vues = vuesParGenre.get(genre);
    if (vues === undefined) continue;
    for (const cle of clesDeclarees(variante)) {
      if (!vues.has(cle)) manques.push(`event(${genre}).${cle}`);
    }
  }

  if (manques.length > 0 || surplus.length > 0) {
    if (manques.length > 0) {
      console.error(
        `\nL episode complet n exerce pas tout ce que le schema declare :\n  ${manques.join('\n  ')}`,
      );
    }
    if (surplus.length > 0) {
      console.error(
        `\nL episode complet porte des cles que le schema NE declare PAS :\n  ${surplus.join('\n  ')}` +
          '\n  (Zod les accepte en silence — ses objets ne sont pas stricts.)',
      );
    }
    console.error('\nCorriger ici, puis relancer.');
    process.exit(1);
  }
  return { clesEpisode, clesEntite, genres: genresVus.size };
}

const couverture = verifierCouverture();

// Et il doit rester un épisode VALIDE : un miroir que le schéma refuserait
// n'apprendrait rien au Rust, sinon à accepter ce que personne n'accepte.
const verdict = Episode.safeParse(episode);
if (!verdict.success) {
  console.error(`\nL episode complet ne passe pas son propre schema :\n${verdict.error}`);
  process.exit(1);
}

function rendu(o) {
  return `${JSON.stringify(o, null, 2)}\n`;
}

function lire(chemin) {
  try {
    return readFileSync(chemin, 'utf8');
  } catch {
    return null;
  }
}

const attendu = rendu(episode);
if (verifier) {
  const identique = lire(DEST) === attendu;
  console.log(`  episode-complet.json      ${identique ? 'a jour' : 'PERIME'}`);
  if (!identique) {
    console.error('\nMiroir perime. Relancer sans --verifier et committer le resultat.');
    process.exit(1);
  }
} else {
  writeFileSync(DEST, attendu, 'utf8');
  console.log('  episode-complet.json      ecrit');
}

console.log(
  `\n${couverture.clesEpisode} cles d episode · ${couverture.clesEntite} cles d entite · ` +
    `${couverture.genres} genres d evenement`,
);
