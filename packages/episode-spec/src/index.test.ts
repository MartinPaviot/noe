import { describe, expect, it } from 'vitest';
import { episodeAvecTrou, episodeValide } from './fixtures.js';
import { EPISODE_FORMAT_VERSION } from './index.js';
import { CONFIRMATION_API_VERIFIABLE, Episode, gradeOf, SCHEMA_V, ToolCall } from './schema.js';

describe('format d episode — version', () => {
  it('expose la version du format', () => {
    expect(EPISODE_FORMAT_VERSION).toBe('1.0.0');
    expect(SCHEMA_V).toBe(1);
  });
});

describe('schema — cas valides (R1.1, R1.2, R1.3)', () => {
  it('accepte un episode nominal', () => {
    expect(() => Episode.parse(episodeValide())).not.toThrow();
  });

  it('accepte un episode avec trou', () => {
    expect(() => Episode.parse(episodeAvecTrou())).not.toThrow();
  });

  it('accepte les trois genres d evenement', () => {
    const ep = Episode.parse(episodeAvecTrou());
    expect(ep.events.map((e) => e.kind)).toEqual(['ui_action', 'api_change', 'gap']);
  });

  it('accepte un supersedes', () => {
    const ep = episodeValide({ supersedes: '01JQ8Z9K2M3N4P5Q6R7S8T9V1X' });
    expect(() => Episode.parse(ep)).not.toThrow();
  });
});

describe('schema — invariant seq strictement croissant (R1.2)', () => {
  it('refuse deux seq egaux', () => {
    const ep = episodeValide();
    const casse = { ...ep, events: [ep.events[0]!, { ...ep.events[1]!, seq: 0 }] };
    const r = Episode.safeParse(casse);
    expect(r.success).toBe(false);
    if (!r.success) expect(JSON.stringify(r.error.issues)).toContain('strictement croissant');
  });

  it('refuse un seq qui decroit', () => {
    const ep = episodeValide();
    const casse = { ...ep, events: [{ ...ep.events[0]!, seq: 5 }, ep.events[1]!] };
    expect(Episode.safeParse(casse).success).toBe(false);
  });
});

describe('schema — invariant bornes temporelles', () => {
  it('refuse un ts anterieur a t0', () => {
    const ep = episodeValide();
    const casse = {
      ...ep,
      events: [{ ...ep.events[0]!, ts: '2026-07-01T00:00:00.000Z' }, ep.events[1]!],
    };
    const r = Episode.safeParse(casse);
    expect(r.success).toBe(false);
    if (!r.success) expect(JSON.stringify(r.error.issues)).toContain('hors des bornes');
  });

  it('refuse un ts posterieur a t1', () => {
    const ep = episodeValide();
    const casse = {
      ...ep,
      events: [ep.events[0]!, { ...ep.events[1]!, ts: '2026-09-01T00:00:00.000Z' }],
    };
    expect(Episode.safeParse(casse).success).toBe(false);
  });

  it('refuse t1 anterieur a t0', () => {
    const r = Episode.safeParse(episodeValide({ t1: '2026-07-01T00:00:00.000Z' }));
    expect(r.success).toBe(false);
  });
});

describe('schema — coherence du grade declare (R2.1)', () => {
  it('refuse un episode a trou declare A', () => {
    const ep = episodeAvecTrou();
    const r = Episode.safeParse({ ...ep, grade: 'A' });
    expect(r.success).toBe(false);
    if (!r.success) expect(JSON.stringify(r.error.issues)).toContain('les regles donnent');
  });

  it('refuse un episode nominal declare C', () => {
    expect(Episode.safeParse(episodeValide({ grade: 'C' })).success).toBe(false);
  });

  it('refuse un compte de trous incoherent', () => {
    const ep = episodeAvecTrou();
    const r = Episode.safeParse({
      ...ep,
      completeness: { explained: 1, out_of_scope: 0, gaps: 0 },
    });
    expect(r.success).toBe(false);
  });
});

describe('schema — rejets structurels', () => {
  it('refuse un id non-ULID', () => {
    expect(Episode.safeParse(episodeValide({ id: 'pas-un-ulid' })).success).toBe(false);
  });

  it('refuse un episode sans evenement', () => {
    expect(Episode.safeParse(episodeValide({ events: [] })).success).toBe(false);
  });

  it('refuse un scope_fields vide', () => {
    expect(Episode.safeParse(episodeValide({ scope_fields: [] })).success).toBe(false);
  });

  it('refuse un etat imbrique (FlatState est plat)', () => {
    const ep = episodeValide();
    const casse = {
      ...ep,
      entities: [{ ...ep.entities[0]!, state_after: { statut: { profond: 1 } } }],
    };
    expect(Episode.safeParse(casse).success).toBe(false);
  });
});

describe('grades — regles mecaniques (R2.1) et motif (R2.3)', () => {
  it('A quand aucun trou et toutes entites resolues', () => {
    const v = gradeOf(episodeValide());
    expect(v.grade).toBe('A');
    expect(v.reason).toContain('sans trou');
  });

  it('B avec exactement un trou, et le dit', () => {
    const v = gradeOf(episodeAvecTrou());
    expect(v.grade).toBe('B');
    expect(v.reason).toContain('1 trou de capture');
  });

  it('B avec exactement une entite non resolue', () => {
    const ep = episodeValide();
    const v = gradeOf({
      ...ep,
      entities: [{ ...ep.entities[0]!, state_after: undefined }],
    });
    expect(v.grade).toBe('B');
    expect(v.reason).toContain('1 entite non resolue');
  });

  it('C avec un trou ET une entite non resolue', () => {
    const ep = episodeAvecTrou();
    const v = gradeOf({
      ...ep,
      entities: [{ ...ep.entities[0]!, state_before: undefined }],
    });
    expect(v.grade).toBe('C');
    expect(v.reason).toContain('declasse en C');
  });

  it('C si la redaction n est pas validee, quoi qu il arrive par ailleurs', () => {
    const ep = episodeValide();
    const v = gradeOf({
      ...ep,
      entities: [{ ...ep.entities[0]!, key: { type: 'contact', value_pseudo: '   ' } }],
    });
    expect(v.grade).toBe('C');
    expect(v.reason).toContain('redaction non validee');
  });

  it('chaque motif est non vide — « pourquoi pas A » est toujours repondable', () => {
    for (const ep of [episodeValide(), episodeAvecTrou()]) {
      expect(gradeOf(ep).reason.length).toBeGreaterThan(10);
    }
  });
});

describe('ToolCall', () => {
  it('accepte une ecriture de champs', () => {
    const r = ToolCall.safeParse({
      connector: 'crm',
      object: 'contact',
      object_id: 'c_001',
      action: 'update_fields',
      fields: { statut: 'qualifie', score: 3, actif: true, note: null },
    });
    expect(r.success).toBe(true);
  });

  it('refuse une action inconnue — une seule action en v1', () => {
    const r = ToolCall.safeParse({
      connector: 'crm',
      object: 'contact',
      object_id: 'c_001',
      action: 'delete',
      fields: {},
    });
    expect(r.success).toBe(false);
  });
});

describe('INVARIANT 7 — la confirmation API (spec 003, R7.1)', () => {
  /**
   * L'invariant est allume par la spec 003. Ces tests le rendent MORDANT.
   *
   * Le basculer sans test, c'est changer une constante et croire que quelque
   * chose s'est passe : rien n'aurait rougi, et rien ne rougirait le jour ou
   * quelqu'un la remettrait a `false`.
   */
  const parfait = () => ({
    events: [{ kind: 'ui_action' as const }],
    entities: [
      {
        key: { type: 'contact', value_pseudo: 'EMAIL_aaa' },
        api_refs: [{ connector: 'crm', object: 'lead', id: 'L-1' }],
        state_before: { statut: 'nouveau' },
        state_after: { statut: 'qualifie' },
      },
    ],
  });

  it('est allume — la spec 003 fournit le connecteur qui le rend atteignable', () => {
    expect(CONFIRMATION_API_VERIFIABLE).toBe(true);
  });

  it('un episode complet AVEC api_refs est A', () => {
    expect(gradeOf(parfait()).grade).toBe('A');
  });

  it('le meme episode avec des api_refs vides tombe en B, et le DIT', () => {
    // Un A sans `api_refs`, c'est un episode qui affirme avoir tout explique sans
    // avoir rien verifie. La raison doit nommer la cause, sinon « pourquoi ce
    // n'est pas un A » reste sans reponse — et R2.3 exige qu'il y en ait une.
    const sansRefs = {
      ...parfait(),
      entities: parfait().entities.map((e) => ({ ...e, api_refs: [] })),
    };
    const v = gradeOf(sansRefs);
    expect(v.grade).toBe('B');
    expect(v.reason).toContain('bornes non confirmees');
  });
});
