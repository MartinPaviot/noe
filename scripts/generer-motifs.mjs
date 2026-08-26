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

// `pathToFileURL` : sur Windows, un chemin absolu commence par une lettre de
// lecteur, que le chargeur ESM lit comme un schema d URL inconnu.
const { MOTIFS_PII, VERSION_MOTIFS, chercherPii } = await import(
  pathToFileURL(join(RACINE, 'packages', 'episode-spec', 'dist', 'index.js')).href
);

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
    .map(({ type, source, drapeaux, note }) => ({ type, source, drapeaux, note }))
    .sort((a, b) => a.type.localeCompare(b.type)),
};

const vecteurs = {
  version: VERSION_MOTIFS,
  note: 'Sorties de reference. Toute implementation doit rendre exactement ceci.',
  cas: ENTREES.map((entree) => ({
    entree,
    // Uniquement le type et la position : l'extrait tronqué de `chercherPii`
    // n'a pas à voyager dans un fichier versionné, et le type + l'index
    // suffisent à prouver que deux moteurs voient la même chose au même endroit.
    occurrences: chercherPii(entree).map(({ type, index }) => ({ type, index })),
  })),
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

for (const [nom, chemin, contenu] of [
  ['motifs.json', DEST_MOTIFS, motifs],
  ['vecteurs-redaction.json', DEST_VECTEURS, vecteurs],
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
