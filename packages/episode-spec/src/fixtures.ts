import type { Episode } from './schema.js';

/**
 * Fabriques d'épisodes pour les tests. Exportées depuis le package : le harness
 * en a besoin autant que `episode-spec`, et les dupliquer les ferait diverger.
 */

export const ULID_A = '01JQ8Z9K2M3N4P5Q6R7S8T9V0W';
export const ULID_B = '01JQ8Z9K2M3N4P5Q6R7S8T9V1X';

/** Un épisode grade A, minimal mais complet et valide. */
export function episodeValide(surcharge: Partial<Episode> = {}): Episode {
  const base: Episode = {
    schema_v: 1,
    id: ULID_A,
    task_slug: 'maj-crm-post-echange',
    t0: '2026-08-01T09:00:00.000Z',
    t1: '2026-08-01T09:05:00.000Z',
    events: [
      {
        schema_v: 1,
        kind: 'ui_action',
        seq: 0,
        ts: '2026-08-01T09:00:10.000Z',
        source: 'ui',
        action: 'navigate',
        target: { role: 'link', name: 'Contacts' },
      },
      {
        schema_v: 1,
        kind: 'api_change',
        seq: 1,
        ts: '2026-08-01T09:04:00.000Z',
        source: 'api',
        connector: 'crm',
        object: 'contact',
        object_id: 'c_001',
        fields_changed: ['statut'],
      },
    ],
    entities: [
      {
        key: { type: 'contact', value_pseudo: 'PSEUDO_CONTACT_001' },
        first_seen_seq: 0,
        api_refs: [{ connector: 'crm', object: 'contact', id: 'c_001' }],
        state_before: { statut: 'nouveau' },
        state_after: { statut: 'qualifie' },
      },
    ],
    grade: 'A',
    grade_reason: 'sequence sans trou, toutes entites resolues, redaction validee',
    scope_fields: ['statut'],
    completeness: { explained: 1, out_of_scope: 0, gaps: 0 },
  };
  return { ...base, ...surcharge };
}

/** Un épisode grade B : un trou de capture, et un seul. */
export function episodeAvecTrou(): Episode {
  const ep = episodeValide();
  return {
    ...ep,
    events: [
      ...ep.events,
      {
        schema_v: 1,
        kind: 'gap',
        seq: 2,
        ts: '2026-08-01T09:04:30.000Z',
        source: 'system',
        gap: { cause: 'sleep', from_seq: 1, to_seq: 2 },
      },
    ],
    grade: 'B',
    grade_reason: 'declasse en B : 1 trou de capture',
    completeness: { explained: 1, out_of_scope: 0, gaps: 1 },
  };
}
