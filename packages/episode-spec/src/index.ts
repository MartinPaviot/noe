/**
 * @noe/episode-spec — le format d'épisode de Noe.
 *
 * Sous licence MIT, volontairement distincte de l'AGPL du reste du dépôt : ce
 * format doit pouvoir être lu, validé et réimplémenté par n'importe qui, y
 * compris dans un produit propriétaire. C'est un format, pas un moteur.
 */

export { cloturer, type EpisodeClos, estClos, remplacer } from './close.js';

export { load, MigrationError, versionsMigrables } from './migrate.js';
export {
  Completeness,
  Entity,
  Episode,
  Event,
  FlatState,
  Gap,
  Grade,
  type GradeVerdict,
  gradeOf,
  SCHEMA_V,
  Target,
  ToolCall,
} from './schema.js';

/** Version du format. Conservée pour compatibilité avec la session 0. */
export const EPISODE_FORMAT_VERSION = '1.0.0' as const;
