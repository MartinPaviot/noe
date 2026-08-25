/**
 * @noe/connectors — adaptateurs vers les systemes de verite.
 * Vide en session 0. Chaque connecteur est en lecture bornee et journalise ses trous.
 */

/** Identifiants des systemes de verite envisages. Aucun n'est implemente. */
export const SYSTEMES_ENVISAGES = ['salesforce', 'google-workspace'] as const;

export type SystemeDeVerite = (typeof SYSTEMES_ENVISAGES)[number];
