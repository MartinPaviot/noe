import { describe, expect, it } from 'vitest';
import { episodeValide } from './fixtures.js';
import { load, MigrationError, versionsMigrables } from './migrate.js';
import { SCHEMA_V } from './schema.js';

/** Un épisode en schema_v 0 : ni `scope_fields`, ni `grade_reason`. */
function episodeV0(): Record<string, unknown> {
  return {
    schema_v: 0,
    id: '01JQ8Z9K2M3N4P5Q6R7S8T9V0W',
    task_slug: 'maj-crm-post-echange',
    t0: '2026-08-01T09:00:00.000Z',
    t1: '2026-08-01T09:05:00.000Z',
    events: [
      {
        schema_v: 0,
        kind: 'api_change',
        seq: 0,
        ts: '2026-08-01T09:04:00.000Z',
        source: 'api',
        connector: 'crm',
        object: 'contact',
        object_id: 'c_001',
        fields_changed: ['statut', 'prochaine_action'],
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
    completeness: { explained: 1, out_of_scope: 0, gaps: 0 },
  };
}

describe('load — version courante', () => {
  it('charge un episode deja en version courante', () => {
    const ep = load(episodeValide());
    expect(ep.schema_v).toBe(SCHEMA_V);
  });

  it('valide au passage : un episode invalide leve', () => {
    expect(() => load(episodeValide({ scope_fields: [] }))).toThrow();
  });
});

describe('load — migration 0 vers 1 (R1.5)', () => {
  it('migre et parse vert', () => {
    const ep = load(episodeV0());
    expect(ep.schema_v).toBe(1);
  });

  it('reconstruit scope_fields depuis les champs reellement touches', () => {
    const ep = load(episodeV0());
    expect(ep.scope_fields).toEqual(['prochaine_action', 'statut']);
  });

  it('reconstruit grade_reason, absent en v0', () => {
    const ep = load(episodeV0());
    expect(ep.grade).toBe('A');
    expect(ep.grade_reason).toContain('sans trou');
  });

  it('propage la version aux evenements', () => {
    const ep = load(episodeV0());
    expect(ep.events.every((e) => e.schema_v === 1)).toBe(true);
  });
});

describe('load — echecs explicites, jamais de lecture partielle (R1.5)', () => {
  it('refuse une version sans migrateur', () => {
    const orphelin = { ...episodeV0(), schema_v: -3 };
    expect(() => load(orphelin)).toThrow(MigrationError);
    try {
      load(orphelin);
    } catch (e) {
      expect((e as MigrationError).message).toContain('aucun migrateur enregistre');
      expect((e as MigrationError).trouvee).toBe(-3);
      expect((e as MigrationError).attendue).toBe(SCHEMA_V);
    }
  });

  it('refuse une version plus recente que le format supporte', () => {
    expect(() => load({ ...episodeV0(), schema_v: SCHEMA_V + 1 })).toThrow(
      /plus recent que le format supporte/,
    );
  });

  it('refuse un schema_v absent', () => {
    const { schema_v: _, ...sansVersion } = episodeV0();
    expect(() => load(sansVersion)).toThrow(/schema_v absent/);
  });

  it('refuse un schema_v non entier', () => {
    expect(() => load({ ...episodeV0(), schema_v: '1' })).toThrow(/non entier/);
  });

  it('refuse une entree qui n est pas un objet', () => {
    expect(() => load('pas un objet')).toThrow(MigrationError);
    expect(() => load(null)).toThrow(MigrationError);
  });
});

describe('registre des migrateurs', () => {
  it('annonce les versions migrables', () => {
    expect(versionsMigrables()).toContain(0);
  });
});
