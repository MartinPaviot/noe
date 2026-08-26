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

/**
 * Un episode tel que la CAPTURE de la spec 002 le produit : des evenements
 * d'interface, des entites reperees mais **non resolues** (aucun connecteur ne
 * les a encore rapprochees d'un etat API).
 *
 * Grade B attendu, et rien a juger — c'est le cas normal avant la spec 003.
 */
export function episodeCapture(): Episode {
  return {
    schema_v: 1,
    id: ULID_B,
    task_slug: 'maj-crm-post-echange',
    t0: '2026-08-06T10:00:00.000Z',
    t1: '2026-08-06T10:04:00.000Z',
    events: [
      {
        schema_v: 1,
        kind: 'ui_action',
        seq: 0,
        ts: '2026-08-06T10:00:30.000Z',
        source: 'ui',
        action: 'navigate',
        target: { role: 'link', name: 'Contacts', region: 'Navigation principale' },
      },
      {
        schema_v: 1,
        kind: 'ui_action',
        seq: 1,
        ts: '2026-08-06T10:02:00.000Z',
        source: 'ui',
        action: 'input',
        target: { role: 'textbox', name: 'Prochaine action', region: 'Fiche contact' },
        payload: 'relancer EMAIL_7f3a9c21',
      },
      {
        schema_v: 1,
        kind: 'ui_action',
        seq: 2,
        ts: '2026-08-06T10:03:00.000Z',
        source: 'ui',
        action: 'submit',
        target: { role: 'button', name: 'Enregistrer', region: 'Fiche contact' },
      },
    ],
    entities: [
      {
        // Entite CANDIDATE : reperee par motif sur, sans api_refs ni etats.
        key: { type: 'contact', value_pseudo: 'EMAIL_7f3a9c21' },
        first_seen_seq: 1,
        api_refs: [],
      },
    ],
    grade: 'B',
    grade_reason: 'declasse en B : 1 entite non resolue',
    scope_fields: ['statut', 'prochaine_action', 'date_relance', 'notes'],
    completeness: { explained: 0, out_of_scope: 0, gaps: 0 },
  };
}
