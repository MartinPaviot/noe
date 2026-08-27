#!/usr/bin/env node
/**
 * Miroir JSON des règles de résolution (spec 003, R2.1 et R2.2).
 *
 * Deux règles vivent en double, en TypeScript et en Rust, parce que la
 * résolution se décide des deux côtés : `resolution.ts` pour le harness,
 * `salesforce.rs` pour le capteur.
 *
 * 1. **L'ordre de force des clés.** Identifiant système, puis courriel, puis
 *    domaine + nom. Il est écrit deux fois, à la main, et rien ne le vérifiait.
 *    Inverser deux entrées ne casserait aucun test : ça déciderait seulement
 *    qu'une ambiguïté de courriel se laisse trancher par un nom, ce que R2.2
 *    interdit — et le corpus aurait l'air juste.
 * 2. **La normalisation des identifiants.** « Les mêmes règles des deux côtés »
 *    est écrit en commentaire dans quatre fichiers. Un commentaire ne vérifie
 *    rien : deux graphies d'une adresse qui ne convergeraient que d'un côté
 *    donneraient deux jetons pour une personne, et la jointure serait perdue
 *    sans que personne ne le voie.
 *
 * Le miroir n'est pas la source : `resolution.ts` l'est. `--verifier` échoue si
 * les deux ont divergé.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const RACINE = join(ICI, '..');
const DEST = join(RACINE, 'packages', 'core', 'vecteurs-resolution.json');

const verifier = process.argv.includes('--verifier');

const { normaliserIdentifiant, resoudre } = await import(
  pathToFileURL(join(RACINE, 'packages', 'core', 'dist', 'ports', 'resolution.js')).href
);

/**
 * Les valeurs sur lesquelles la normalisation doit s'accorder.
 *
 * Le jeu couvre ce qui varie vraiment dans la vie d'une adresse lue à l'écran :
 * la casse, les blancs de bordure d'un copier-coller, et l'accent qu'un nom
 * porte mais qu'une adresse ne porte jamais.
 */
const IDENTIFIANTS = [
  ['email_token', 'Jean.Dupont@Exemple.FR'],
  ['email_token', '  jean.dupont@exemple.fr  '],
  ['email_token', 'JEAN.DUPONT@EXEMPLE.FR'],
  ['email_token', 'a@b.fr'],
  ['email_token', ''],
  ['domain_name', '  Exemple.FR '],
  ['domain_name', 'exemple.fr'],
  // Un identifiant système garde sa casse : elle porte le suffixe de contrôle,
  // donc de l'information. Le mettre en minuscules désignerait un autre
  // enregistrement — ou aucun.
  ['system_id', '0035g00000LmT4EAAV'],
  ['system_id', '  0035g00000LmT4EAAV  '],
  ['system_id', '0035G00000LMT4EAAV'],
];

const REF = { connector: 'salesforce', object: 'Contact', id: '003AAA' };
const AUTRE = { connector: 'salesforce', object: 'Lead', id: '00QBBB' };
const QUAND = '2026-01-01T00:00:00.000Z';

/**
 * Les scénarios de résolution que les deux implémentations doivent trancher
 * pareil.
 *
 * Le troisième est le plus important : **une ambiguïté n'est jamais départagée
 * par une clé plus faible.** Affiner avec `domain_name` ce que le courriel n'a
 * pas tranché, c'est exactement deviner.
 */
const SCENARIOS = [
  {
    nom: 'un seul candidat par courriel',
    candidate: { id: 'c1', keys: [{ kind: 'email_token', value: 'jean@ex.com' }] },
    distants: [{ ref: REF, keys: [{ kind: 'email_token', value: 'jean@ex.com' }] }],
  },
  {
    nom: 'aucun candidat',
    candidate: { id: 'c2', keys: [{ kind: 'email_token', value: 'absent@ex.com' }] },
    distants: [{ ref: REF, keys: [{ kind: 'email_token', value: 'jean@ex.com' }] }],
  },
  {
    nom: 'deux candidats par courriel, un seul par domaine + nom',
    candidate: {
      id: 'c3',
      keys: [
        { kind: 'email_token', value: 'jean@ex.com' },
        { kind: 'domain_name', domain: 'ex.com', name: 'Jean Dupont' },
      ],
    },
    distants: [
      {
        ref: REF,
        keys: [
          { kind: 'email_token', value: 'jean@ex.com' },
          { kind: 'domain_name', domain: 'ex.com', name: 'Jean Dupont' },
        ],
      },
      { ref: AUTRE, keys: [{ kind: 'email_token', value: 'jean@ex.com' }] },
    ],
  },
  {
    nom: 'l identifiant systeme passe avant le courriel ambigu',
    candidate: {
      id: 'c4',
      keys: [
        { kind: 'email_token', value: 'jean@ex.com' },
        { kind: 'system_id', value: '003AAA' },
      ],
    },
    distants: [
      { ref: REF, keys: [{ kind: 'system_id', value: '003AAA' }] },
      { ref: AUTRE, keys: [{ kind: 'email_token', value: 'jean@ex.com' }] },
    ],
  },
  {
    nom: 'deux graphies d une adresse designent la meme personne',
    candidate: { id: 'c5', keys: [{ kind: 'email_token', value: 'Jean.Dupont@Exemple.FR' }] },
    distants: [{ ref: REF, keys: [{ kind: 'email_token', value: 'jean.dupont@exemple.fr' }] }],
  },
  {
    nom: 'aucune cle exploitable',
    candidate: { id: 'c6', keys: [] },
    distants: [{ ref: REF, keys: [{ kind: 'email_token', value: 'jean@ex.com' }] }],
  },
];

const contenu = {
  note: 'Genere par scripts/generer-resolution.mjs. Ne pas editer a la main.',
  // L'ordre de force, sorti du code et non recopie : c'est lui qu'on garde.
  priorite: ['system_id', 'email_token', 'domain_name'],
  normalisation: IDENTIFIANTS.map(([kind, valeur]) => ({
    kind,
    valeur,
    attendu: normaliserIdentifiant(kind, valeur),
  })),
  resolutions: SCENARIOS.map((s) => {
    const r = resoudre(s.candidate, s.distants, QUAND);
    return {
      nom: s.nom,
      statut: r.status,
      par: r.status === 'resolved' ? r.by : null,
      id: r.status === 'resolved' ? r.ref.id : null,
      compte: r.status === 'ambiguous' ? r.count : null,
    };
  }),
};

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

const attendu = rendu(contenu);
if (verifier) {
  const identique = lire(DEST) === attendu;
  console.log(`  vecteurs-resolution.json  ${identique ? 'a jour' : 'PERIME'}`);
  if (!identique) {
    console.error('\nMiroir perime. Relancer sans --verifier et committer le resultat.');
    process.exit(1);
  }
} else {
  writeFileSync(DEST, attendu, 'utf8');
  console.log('  vecteurs-resolution.json  ecrit');
}

console.log(
  `\n${contenu.normalisation.length} vecteurs de normalisation · ` +
    `${contenu.resolutions.length} scenarios de resolution`,
);
