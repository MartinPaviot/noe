/**
 * @noe/core — domaine pur. Aucun I/O, aucun reseau, aucune dependance runtime.
 * Vide par construction : la session 0 ne livre aucune logique metier.
 */

/** Version du domaine, incrementee quand un invariant change. */
export const DOMAIN_VERSION = '0.0.0' as const;

/**
 * Le client commun de la spec 003 : backoff, budget, classification.
 *
 * Tout passe par lui. Un adaptateur qui appellerait le réseau directement
 * échapperait au budget de R5.3, et une tempête de requêtes ne se voit qu'après
 * coup — quand le quota est déjà brûlé.
 */
export type { Consequence, OptionsClient, Reponse } from './ports/client.js';
export {
  appeler,
  BUDGET_PAR_EPISODE,
  Budget,
  consequence,
  DELAI_BASE_MS,
  DELAI_MAX_MS,
  delaiMs,
  TENTATIVES_MAX,
} from './ports/client.js';
/**
 * Le port de fédération de la spec 003 : **lecture seule, structurellement**.
 *
 * Le domaine ne connaît que ce port. Le choix du CRM vit dans `terrain.json`,
 * jamais dans le code hors de son adaptateur.
 */
export type {
  ApiChange,
  ApiRef,
  ConnectorError,
  EntityCandidate,
  FlatState,
  HistoryPoint,
  Outcome,
  ReadConnector,
  Resolution,
  StateMeta,
  StrongKey,
  TimeWindow,
} from './ports/connector.js';
export { err, ok } from './ports/connector.js';
export type { ChampsSuivis, ScenarioFake, VerdictAvant } from './ports/fake-connector.js';
export { FakeConnector, verdictAvant } from './ports/fake-connector.js';
/**
 * La réconciliation : chaque changement d'API dans exactement une colonne.
 */
export type { ActionUi, Bilan, Colonne, LigneReconciliee, Trou } from './ports/reconciliation.js';
export {
  FENETRE_JOINTURE_MS,
  reconcilier,
  tauxExplique,
} from './ports/reconciliation.js';
/**
 * La résolution des entités : clés fortes seulement, jamais de devinette.
 */
export type { CandidatDistant } from './ports/resolution.js';
export { memeCle, normaliserIdentifiant, raison, resoudre } from './ports/resolution.js';
