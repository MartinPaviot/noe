#!/usr/bin/env node
/**
 * Miroir JSON du client commun (spec 003, R5.1), pour le consommateur Rust.
 *
 * Le client de reprise existe en deux exemplaires — `packages/core/src/ports/
 * client.ts` et `apps/desktop/src-tauri/src/client.rs` — parce que la reprise
 * doit avoir lieu là où l'appel part, et les appels partent des deux côtés.
 * **Deux clients qui reprendraient différemment produiraient deux corpus
 * incomparables**, et l'écart ne se verrait qu'au moment où on essaierait de les
 * additionner.
 *
 * Le 2026-08-27, le miroir Rust a été écrit en recopiant les constantes **à la
 * main**. C'est exactement la dette que `generer-motifs.mjs` avait été écrit
 * pour éteindre ailleurs, et la laisser vivre ici aurait été la reprendre en
 * connaissance de cause.
 *
 * ## Pourquoi des vecteurs et pas seulement des constantes
 *
 * Comparer `TENTATIVES_MAX = 5` des deux côtés ne prouve presque rien : les
 * constantes sont la partie qu'on relit. Ce qui diverge en silence, c'est
 * l'**arithmétique** — un `Math.floor` contre une troncature de cast, un
 * `2 ** n` en flottant contre un décalage entier, un plafond appliqué avant ou
 * après le jitter. Comparer les SORTIES sur les mêmes entrées attrape ça ; la
 * leçon vient de la bibliothèque de motifs, où deux moteurs d'expressions
 * régulières lisaient la même chaîne différemment.
 *
 * Le miroir n'est pas la source : `client.ts` l'est. `--verifier` échoue si les
 * deux ont divergé. Sans ce mode, le fichier deviendrait en quelques semaines
 * une copie périmée que personne ne regarde.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const RACINE = join(ICI, '..');
const DEST = join(RACINE, 'packages', 'core', 'vecteurs-client.json');

const verifier = process.argv.includes('--verifier');

// On importe la BUILD, comme le fait le generateur de motifs : lire le source
// TypeScript demanderait un chargeur, et un miroir produit par un outil que la
// CI n'a pas serait un miroir qu'on ne verifie jamais.
//
// `pathToFileURL` : sur Windows, un chemin absolu commence par une lettre de
// lecteur, que le chargeur ESM lit comme un schema d'URL inconnu.
const { BUDGET_PAR_EPISODE, DELAI_BASE_MS, DELAI_MAX_MS, TENTATIVES_MAX, delaiMs } = await import(
  pathToFileURL(join(RACINE, 'packages', 'core', 'dist', 'ports', 'client.js')).href
);

/**
 * La grille sur laquelle les deux implémentations doivent s'accorder.
 *
 * Les tirages de jitter sont **choisis, pas tirés** : un vecteur qui embarquerait
 * du hasard vérifierait le hasard. Les bornes `0` et `1` comptent autant que le
 * milieu — c'est là que le plancher de demi-jitter et le plafond se voient.
 *
 * Les tentatives vont **au-delà du maximum** : le calcul doit rester borné même
 * si quelqu'un relève `TENTATIVES_MAX` un jour, et c'est précisément le cas où
 * un `2 ** n` déborde d'un côté et sature de l'autre.
 */
const TENTATIVES = [1, 2, 3, 4, 5, 6, 8, 12, 20, 40];
const TIRAGES = [0, 0.25, 0.5, 0.75, 0.999999, 1];

const contenu = {
  note: 'Genere par scripts/generer-client.mjs. Ne pas editer a la main.',
  constantes: {
    tentatives_max: TENTATIVES_MAX,
    delai_base_ms: DELAI_BASE_MS,
    delai_max_ms: DELAI_MAX_MS,
    budget_par_episode: BUDGET_PAR_EPISODE,
  },
  delais: TENTATIVES.flatMap((tentative) =>
    TIRAGES.map((alea) => ({
      tentative,
      alea,
      attendu_ms: delaiMs(tentative, () => alea),
    })),
  ),
};

/** Le rendu : indenté, terminé par une ligne vide, stable d'une fois sur l'autre. */
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
  console.log(`  vecteurs-client.json      ${identique ? 'a jour' : 'PERIME'}`);
  if (!identique) {
    console.error('\nMiroir perime. Relancer sans --verifier et committer le resultat.');
    process.exit(1);
  }
} else {
  writeFileSync(DEST, attendu, 'utf8');
  console.log('  vecteurs-client.json      ecrit');
}

console.log(
  `\n${contenu.delais.length} vecteurs de delai · ` +
    `${TENTATIVES.length} tentatives x ${TIRAGES.length} tirages`,
);
