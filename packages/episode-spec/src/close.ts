import { ulid } from 'ulid';
import { type Episode, gradeOf } from './schema.js';

/**
 * Un épisode clôturé. Le type interdit l'écriture, `Object.freeze` l'interdit à
 * l'exécution. Les deux, parce qu'un `as any` suffirait à contourner le premier.
 */
export type EpisodeClos = Readonly<Episode>;

/** Gèle en profondeur. Le gel de surface laisserait `events[0].seq` mutable. */
function gelerProfond<T>(valeur: T): T {
  if (valeur === null || typeof valeur !== 'object') return valeur;
  for (const v of Object.values(valeur as Record<string, unknown>)) gelerProfond(v);
  return Object.freeze(valeur);
}

/**
 * Clôture un épisode : recalcule son grade, l'annote, et le gèle définitivement.
 *
 * INVARIANT IV — un épisode clôturé n'est jamais modifié. Une correction produit
 * un nouvel épisode qui référence l'ancien (voir `remplacer`).
 */
export function cloturer(ep: Episode): EpisodeClos {
  const verdict = gradeOf(ep);
  return gelerProfond({ ...ep, grade: verdict.grade, grade_reason: verdict.reason });
}

/**
 * Produit un épisode corrigé qui **remplace** un épisode clôturé, sans jamais le
 * modifier. Le nouvel épisode porte un `id` neuf et un `supersedes` vers l'ancien.
 *
 * C'est le seul chemin légitime pour « corriger » un épisode.
 */
export function remplacer(
  ancien: EpisodeClos,
  corrections: Partial<Omit<Episode, 'id' | 'supersedes' | 'schema_v'>>,
): EpisodeClos {
  const candidat: Episode = {
    ...ancien,
    ...corrections,
    id: ulid(),
    supersedes: ancien.id,
  };
  return cloturer(candidat);
}

/** Vrai si l'épisode a bien été gelé. Utilisé par les tests et les gardes-fous. */
export function estClos(ep: Episode): boolean {
  return Object.isFrozen(ep);
}
