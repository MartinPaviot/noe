/**
 * @noe/harness — le socle de preuve.
 *
 * Rejeu à froid d'un corpus d'épisodes et jugement mécanique. Aucun appel réseau,
 * aucun modèle : le juge compare des états normalisés, et rien d'autre.
 */

export {
  type ChampJuge,
  CLASSES,
  type Classe,
  diffObserve,
  equivalent,
  juger,
  normalize,
  type Valeur,
  type VerdictEpisode,
} from './judge.js';

export {
  type Cible,
  contexteDe,
  type Policy,
  politiqueNulle,
  politiqueParfaite,
  type ReplayContext,
} from './policy.js';

export {
  type Agregat,
  agreger,
  chargerCorpus,
  codeSortie,
  type EpisodeIllisible,
  EXIT_ERREUR,
  EXIT_OK,
  EXIT_VERDICT_NON_CONFORME,
  type RapportRejeu,
  rejouer,
} from './replay.js';

export { rapportJson, rapportTexte, stringifyStable } from './report.js';
