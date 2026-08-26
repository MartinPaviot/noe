#!/usr/bin/env node
/**
 * Miroir JSON de la bibliothèque de motifs PII, pour les consommateurs non-TS.
 *
 * `docs/decisions.md` posait la règle et la dette dans la même phrase : les
 * motifs vivent dans `episode-spec` sous forme de **chaînes** pour que
 * l'adaptateur Rust les consomme telles quelles — « avant la tâche 3, il faudra
 * générer un miroir JSON et un test de synchronisation, sans quoi la promesse
 * n'est qu'une intention ». C'est ce fichier.
 *
 * Le miroir n'est pas la source : `MOTIFS_PII` l'est. Le miroir en est une
 * projection vérifiée, et `--verifier` échoue si les deux ont divergé. Sans ce
 * mode, le fichier deviendrait en quelques semaines une copie périmée que
 * personne ne regarde — exactement la divergence que la décision voulait
 * empêcher.
 *
 * Les fichiers produits sont **exclus du formateur** dans `biome.json`. Biome
 * reformatait le JSON genere — il replie un tableau court sur une ligne — et le
 * verificateur le declarait alors perime a chaque `pnpm format`. Deux outils qui
 * se disputent le meme fichier finissent toujours par faire desactiver le plus
 * utile des deux.
 *
 * Sont aussi générés des **vecteurs de test partagés**. Comparer les chaînes de
 * motifs entre deux implémentations ne prouve presque rien : deux moteurs
 * d'expressions régulières peuvent lire la même chaîne différemment. Comparer
 * les SORTIES sur les mêmes entrées, si.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const RACINE = join(ICI, '..');
const DEST_MOTIFS = join(RACINE, 'packages', 'episode-spec', 'motifs.json');
const DEST_VECTEURS = join(RACINE, 'packages', 'episode-spec', 'vecteurs-redaction.json');
const DEST_CAUSES = join(RACINE, 'packages', 'episode-spec', 'causes-gap.json');
const DEST_GRADES = join(RACINE, 'packages', 'episode-spec', 'vecteurs-grade.json');

// `pathToFileURL` : sur Windows, un chemin absolu commence par une lettre de
// lecteur, que le chargeur ESM lit comme un schema d URL inconnu.
const { CAUSES_GAP, MOTIFS_PII, VERSION_MOTIFS, chercherPii, gradeOf, resoudreChevauchements } =
  await import(pathToFileURL(join(RACINE, 'packages', 'episode-spec', 'dist', 'index.js')).href);

/**
 * Les entrées sur lesquelles TOUTES les implémentations doivent s'accorder.
 *
 * Le jeu couvre trois choses : ce qui doit être détecté, ce qui ne doit PAS
 * l'être (les faux positifs coûtent des jointures perdues), et les formes qui
 * doivent produire le MÊME jeton après normalisation — c'est cette dernière
 * famille qui fait vivre le graphe d'entités.
 */
const ENTREES = [
  // --- ce qui doit etre detecte ---
  'Rappeler jean.dupont@exemple.fr avant vendredi',
  'Ligne directe 06 12 34 56 78',
  'Ligne directe 0612345678',
  'Ligne directe +33 6 12 34 56 78',
  'Numero belge +32 471 12 34 56',
  'Virement sur FR7630006000011234567890189',
  'Carte 4970 1234 5678 9012',
  'Deux a la fois : a@b.fr et 06.12.34.56.78',

  // --- ce qui ne doit PAS l etre ---
  'Reference interne 2026-08-26',
  'Version 1.2.3 du connecteur',
  'Montant 1 234,56 EUR',
  'Piste ouverte il y a 12 jours',
  'Code postal 75011 Paris',
  'SIRET 12345678900011',
  '',

  // --- limites ---
  'arobase sans domaine : jean@exemple',
  'presque un IBAN : FR76 mais trop court',
];

const motifs = {
  version: VERSION_MOTIFS,
  // Trie par type : le miroir doit avoir un ordre stable, sinon deux
  // generations successives produiraient un diff sans changement de fond.
  motifs: [...MOTIFS_PII]
    .map(({ type, source, drapeaux, priorite, note }) => ({
      type,
      source,
      drapeaux,
      priorite,
      note,
    }))
    .sort((a, b) => a.type.localeCompare(b.type)),
};

const vecteurs = {
  version: VERSION_MOTIFS,
  note: 'Sorties de reference. Toute implementation doit rendre exactement ceci.',
  cas: ENTREES.map((entree) => {
    // Les index de TypeScript comptent en unites UTF-16, ceux de Rust en
    // octets. Sur de l'ASCII les deux coincident ; ailleurs ils ne seraient pas
    // comparables, et le test inter-implementations comparerait des pommes et
    // des poires sans le dire.
    // Test sur les points de code plutot que par expression reguliere : une
    // classe de caracteres de controle dans une regex est precisement ce que
    // le lint interdit, et il a raison — elle se relit mal et se corrige de
    // travers, comme la premiere version de cette ligne l'a montre.
    if ([...entree].some((c) => (c.codePointAt(0) ?? 0) > 0x7f)) {
      throw new Error(`vecteur non-ASCII, les index ne seraient pas comparables : ${entree}`);
    }
    const brutes = chercherPii(entree);
    return {
      entree,
      // Uniquement le type et les bornes : l'extrait tronqué de `chercherPii`
      // n'a pas à voyager dans un fichier versionné, et le type + la position
      // suffisent à prouver que deux moteurs voient la même chose au même
      // endroit.
      occurrences: brutes.map(({ type, index, fin }) => ({ type, index, fin })),
      // Ce qui sera REELLEMENT remplace apres arbitrage des chevauchements.
      // C'est cette liste-la qui determine les jetons, donc les jointures.
      retenues: resoudreChevauchements(brutes).map(({ type, index, fin }) => ({
        type,
        index,
        fin,
      })),
    };
  }),
};

const lire = (chemin) => {
  try {
    return readFileSync(chemin, 'utf8');
  } catch {
    return null;
  }
};

const rendu = (o) => `${JSON.stringify(o, null, 2)}\n`;

const verifier = process.argv.includes('--verifier');
let ecarts = 0;

// Les causes de trou : meme dispositif que les motifs, meme raison. Le capteur
// Rust porte le meme enum ; s'ils divergent, il ecrit une cause que le harness
// refuse de parser, et l'episode part en quarantaine sans explication.
const causes = { causes: [...CAUSES_GAP].sort() };

/**
 * Vecteurs de grade : le SEUIL, figé.
 *
 * Le seuil de `gradeOf` — au plus UN défaut pour rester en B, deux ou plus font
 * tomber en C — a été mal miroité côté Rust au premier essai. Le harness a
 * refusé l'épisode, ce qui est le bon comportement, mais la divergence ne
 * s'était vue qu'en produisant un épisode réel. Ces vecteurs la font voir en CI.
 */
const CAS_GRADE = [
  { gaps: 0, entites: [] },
  { gaps: 0, entites: [{ resolue: true }] },
  { gaps: 0, entites: [{ resolue: false }] },
  { gaps: 1, entites: [] },
  { gaps: 1, entites: [{ resolue: true }] },
  { gaps: 1, entites: [{ resolue: false }] },
  { gaps: 2, entites: [] },
  { gaps: 0, entites: [{ resolue: false }, { resolue: false }] },
  { gaps: 3, entites: [{ resolue: false }, { resolue: true }] },
  { gaps: 0, entites: [{ resolue: true, pseudo: '   ' }] },
];

const episodeSynthetique = (cas) => ({
  events: [
    ...Array.from({ length: cas.gaps }, (_, i) => ({ kind: 'gap', seq: i + 1 })),
    { kind: 'ui_action', seq: 900 },
  ],
  entities: cas.entites.map((e, i) => ({
    key: { type: 'capture', value_pseudo: e.pseudo ?? `CIBLE_${i}0000000` },
    first_seen_seq: 1,
    api_refs: [],
    ...(e.resolue ? { state_before: {}, state_after: {} } : {}),
  })),
});

const grades = {
  note: 'Le seuil de gradeOf, fige. Toute implementation doit rendre exactement ceci.',
  cas: CAS_GRADE.map((cas) => {
    const v = gradeOf(episodeSynthetique(cas));
    return {
      gaps: cas.gaps,
      entites: cas.entites.map((e) => ({ resolue: e.resolue === true, pseudo: e.pseudo ?? null })),
      grade: v.grade,
      reason: v.reason,
    };
  }),
};

for (const [nom, chemin, contenu] of [
  ['motifs.json', DEST_MOTIFS, motifs],
  ['vecteurs-redaction.json', DEST_VECTEURS, vecteurs],
  ['causes-gap.json', DEST_CAUSES, causes],
  ['vecteurs-grade.json', DEST_GRADES, grades],
]) {
  const attendu = rendu(contenu);
  if (verifier) {
    const actuel = lire(chemin);
    const identique = actuel === attendu;
    console.log(`  ${nom.padEnd(26)} ${identique ? 'a jour' : 'PERIME'}`);
    if (!identique) ecarts++;
  } else {
    writeFileSync(chemin, attendu, 'utf8');
    console.log(`  ${nom.padEnd(26)} ecrit`);
  }
}

const detectees = vecteurs.cas.filter((c) => c.occurrences.length > 0).length;
console.log(
  `\n${motifs.motifs.length} motifs (v${VERSION_MOTIFS}) · ` +
    `${vecteurs.cas.length} vecteurs, dont ${detectees} avec detection`,
);

if (verifier && ecarts > 0) {
  console.error(
    `\n${ecarts} fichier(s) perimes. Relancer sans --verifier et committer le resultat.`,
  );
  process.exit(1);
}
