/**
 * @noe/core — domaine pur. Aucun I/O, aucun reseau, aucune dependance runtime.
 * Vide par construction : la session 0 ne livre aucune logique metier.
 */

/** Version du domaine, incrementee quand un invariant change. */
export const DOMAIN_VERSION = '0.0.0' as const;
