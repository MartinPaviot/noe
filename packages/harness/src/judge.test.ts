import type { Episode, ToolCall } from '@noe/episode-spec';
import { describe, expect, it } from 'vitest';
import { equivalent, juger, normalize } from './judge.js';

// ---------------------------------------------------------------------------
// R4.1 — normalisation
// ---------------------------------------------------------------------------

describe('normalize — null, absent et vide sont equivalents (R4.1)', () => {
  it('ramene null, undefined et la chaine vide a null', () => {
    for (const v of [null, undefined, '', '   ', '\n', '\r\n']) {
      expect(normalize(v)).toBeNull();
    }
  });

  it('donc null equivaut a absent equivaut a vide', () => {
    expect(equivalent(null, '')).toBe(true);
    expect(equivalent(undefined, '   ')).toBe(true);
  });
});

describe('normalize — espaces et fins de ligne', () => {
  it('rogne les espaces de bord', () => {
    expect(normalize('  qualifie  ')).toBe('qualifie');
  });

  it('unifie CRLF en LF', () => {
    expect(normalize('ligne1\r\nligne2')).toBe('ligne1\nligne2');
    expect(equivalent('a\r\nb', 'a\nb')).toBe(true);
  });

  it('ne touche pas aux espaces interieurs', () => {
    expect(normalize('envoyer  le devis')).toBe('envoyer  le devis');
  });
});

describe('normalize — nombres compares en valeur, pas en chaine (R4.1)', () => {
  it('convertit une chaine numerique', () => {
    expect(normalize('42')).toBe(42);
    expect(normalize('-3.5')).toBe(-3.5);
    expect(normalize(' 7 ')).toBe(7);
  });

  it('rend « 42 » equivalent a 42', () => {
    expect(equivalent('42', 42)).toBe(true);
    expect(equivalent('42.0', 42)).toBe(true);
  });

  it('distingue deux nombres differents', () => {
    expect(equivalent('42', 43)).toBe(false);
  });

  it('ecarte les nombres non finis', () => {
    expect(normalize(Number.NaN)).toBeNull();
    expect(normalize(Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe('normalize — dates en ISO-8601 UTC (R4.1)', () => {
  it('accepte trois formats et les ramene au meme instant', () => {
    const attendu = '2026-08-15T00:00:00.000Z';
    expect(normalize('2026-08-15T00:00:00.000Z')).toBe(attendu);
    expect(normalize('2026-08-15T00:00:00Z')).toBe(attendu);
    expect(normalize('2026-08-15')).toBe(attendu);
  });

  it('rend equivalents deux ecritures du meme instant', () => {
    expect(equivalent('2026-08-15T00:00:00Z', '2026-08-15')).toBe(true);
  });

  it('distingue deux dates differentes', () => {
    expect(equivalent('2026-08-15', '2026-08-16')).toBe(false);
  });

  it('ne prend pas un nombre pour une date — l ordre des regles compte', () => {
    // Date.parse('2026') vaut un instant valide dans certains moteurs.
    // Le test numerique passe d abord, sinon une annee deviendrait un horodatage.
    expect(normalize('2026')).toBe(2026);
  });

  it('laisse tel quel un texte qui ne ressemble pas a une date', () => {
    expect(normalize('qualifie')).toBe('qualifie');
    expect(normalize('objection budget')).toBe('objection budget');
  });
});

describe('normalize — booleens', () => {
  it('passe les booleens sans les toucher', () => {
    expect(normalize(true)).toBe(true);
    expect(normalize(false)).toBe(false);
  });

  it('ne confond pas false et null', () => {
    expect(equivalent(false, null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// R4.2 / R4.3 — classement et verdict
// ---------------------------------------------------------------------------

function episode(surcharge: Partial<Episode> = {}): Episode {
  return {
    schema_v: 1,
    id: '01JQA1B2C3D4E5F6G7H8J9K0M1',
    task_slug: 't',
    t0: '2026-08-01T09:00:00.000Z',
    t1: '2026-08-01T09:10:00.000Z',
    events: [
      {
        schema_v: 1,
        kind: 'api_change',
        seq: 0,
        ts: '2026-08-01T09:01:00.000Z',
        source: 'api',
        connector: 'crm',
        object: 'contact',
        object_id: 'c1',
        fields_changed: ['statut'],
      },
    ],
    entities: [
      {
        key: { type: 'contact', value_pseudo: 'P1' },
        first_seen_seq: 0,
        api_refs: [{ connector: 'crm', object: 'contact', id: 'c1' }],
        state_before: { statut: 'nouveau', notes: '' },
        state_after: { statut: 'qualifie', notes: '' },
      },
    ],
    grade: 'A',
    grade_reason: 'sequence sans trou, toutes entites resolues, redaction validee',
    scope_fields: ['statut', 'notes'],
    completeness: { explained: 1, out_of_scope: 0, gaps: 0 },
    ...surcharge,
  };
}

const appel = (fields: Record<string, string | number | boolean | null>): ToolCall => ({
  connector: 'crm',
  object: 'contact',
  object_id: 'c1',
  action: 'update_fields',
  fields,
});

describe('classement des champs (R4.2)', () => {
  it('accord quand propose et observe coincident', () => {
    const v = juger(episode(), [appel({ statut: 'qualifie' })]);
    expect(v.champs.map((c) => c.classe)).toEqual(['accord']);
    expect(v.verdict).toBe('accord');
  });

  it('desaccord quand les deux different', () => {
    const v = juger(episode(), [appel({ statut: 'perdu' })]);
    expect(v.totaux.desaccord).toBe(1);
    expect(v.verdict).toBe('desaccord');
  });

  it('manque quand observe sans etre propose', () => {
    const v = juger(episode(), []);
    expect(v.totaux.manque).toBe(1);
    expect(v.verdict).toBe('desaccord');
  });

  it('excedent quand propose sans etre observe', () => {
    const v = juger(episode(), [appel({ statut: 'qualifie', notes: 'inventee' })]);
    expect(v.totaux.excedent).toBe(1);
    expect(v.verdict).toBe('desaccord');
  });

  it('accord malgre une ecriture equivalente mais differemment redigee', () => {
    const ep = episode({
      entities: [
        {
          key: { type: 'contact', value_pseudo: 'P1' },
          first_seen_seq: 0,
          api_refs: [{ connector: 'crm', object: 'contact', id: 'c1' }],
          state_before: { statut: 'nouveau', notes: '' },
          state_after: { statut: 'qualifie', notes: 'a\r\nb' },
        },
      ],
    });
    const v = juger(ep, [appel({ statut: '  qualifie  ', notes: 'a\nb' })]);
    expect(v.verdict).toBe('accord');
  });
});

describe('hors perimetre — ne pese jamais sur le verdict (R4.2)', () => {
  it('classe le champ hors scope et garde le verdict accord', () => {
    const ep = episode({
      scope_fields: ['statut'],
      entities: [
        {
          key: { type: 'contact', value_pseudo: 'P1' },
          first_seen_seq: 0,
          api_refs: [{ connector: 'crm', object: 'contact', id: 'c1' }],
          state_before: { statut: 'nouveau', derniere_connexion: '2026-01-01T00:00:00.000Z' },
          state_after: { statut: 'qualifie', derniere_connexion: '2026-08-01T00:00:00.000Z' },
        },
      ],
    });
    const v = juger(ep, [appel({ statut: 'qualifie' })]);
    expect(v.totaux.hors_perimetre).toBe(1);
    expect(v.totaux.manque).toBe(0);
    expect(v.verdict).toBe('accord');
  });
});

describe('verdict (R4.3) et exclusion des grades non-A (R2.2)', () => {
  it('accord ssi zero desaccord, zero manque, zero excedent', () => {
    const v = juger(episode(), [appel({ statut: 'qualifie' })]);
    expect(v.totaux.desaccord + v.totaux.manque + v.totaux.excedent).toBe(0);
    expect(v.verdict).toBe('accord');
  });

  it('un grade A compte dans les statistiques', () => {
    expect(juger(episode(), []).compte_dans_stats).toBe(true);
  });

  it('un grade B est lisible mais exclu', () => {
    const ep = episode({
      events: [
        ...episode().events,
        {
          schema_v: 1,
          kind: 'gap',
          seq: 1,
          ts: '2026-08-01T09:02:00.000Z',
          source: 'system',
          gap: { cause: 'sleep', from_seq: 0, to_seq: 1 },
        },
      ],
      grade: 'B',
      grade_reason: 'declasse en B : 1 trou de capture',
      completeness: { explained: 1, out_of_scope: 0, gaps: 1 },
    });
    const v = juger(ep, [appel({ statut: 'qualifie' })]);
    expect(v.compte_dans_stats).toBe(false);
    expect(v.verdict).toBe('accord'); // reste jugeable, simplement pas compte
  });
});

describe('entite non resolue', () => {
  it('ne fabrique aucun diff quand un etat manque', () => {
    const ep = episode({
      entities: [
        {
          key: { type: 'contact', value_pseudo: 'P1' },
          first_seen_seq: 0,
          api_refs: [{ connector: 'crm', object: 'contact', id: 'c1' }],
          state_before: { statut: 'nouveau' },
        },
      ],
      grade: 'B',
      grade_reason: 'declasse en B : 1 entite non resolue',
    });
    expect(juger(ep, []).champs).toHaveLength(0);
  });
});

describe('les champs retires du verdict (spec 003, §7)', () => {
  /**
   * Un champ dont on ignore la valeur d'AVANT ne peut pas etre juge.
   *
   * Le garder produirait un faux desaccord : `undefined → "qualifie"` se lit
   * comme un changement, alors qu'on ne sait simplement pas ce qu'il y avait.
   * Le juge accuserait la politique de ne pas avoir propose une ecriture qu'elle
   * n'avait aucune raison de proposer.
   */
  const episodeAvecInconnu = (): Episode =>
    episode({
      grade: 'A',
      // `description` DOIT etre dans le perimetre, sinon le champ serait deja
      // exclu pour une autre raison et le test ne prouverait rien. C'est le
      // piege : une regle qu'on croit verifier alors qu'une autre s'applique
      // avant.
      scope_fields: ['statut', 'description'],
      entities: [
        {
          key: { type: 'contact', value_pseudo: 'EMAIL_aaa' },
          first_seen_seq: 1,
          api_refs: [{ connector: 'crm', object: 'lead', id: 'L-1' }],
          state_before: { statut: 'nouveau' },
          state_after: { statut: 'qualifie', description: 'ecrite ailleurs' },
          state_meta: {
            description: {
              unknown_before: true,
              reason: 'champ non historise par le systeme',
            },
          },
        },
      ],
    });

  it('un champ unknown_before ne compte NI en accord NI en desaccord', () => {
    const ep = episodeAvecInconnu();
    const v = juger(ep, [
      {
        connector: 'crm',
        object: 'lead',
        object_id: 'L-1',
        action: 'update_fields',
        fields: { statut: 'qualifie' },
      },
    ]);
    expect(v.champs.map((c) => c.champ)).not.toContain('description');
    expect(v.verdict).toBe('accord');
  });

  it('sans le retrait, le meme episode serait en desaccord', () => {
    // Le test qui prouve que la regle SERT a quelque chose. Sans elle, le champ
    // apparait comme « manque » et le verdict bascule.
    const ep = episodeAvecInconnu();
    const sansMeta: Episode = {
      ...ep,
      entities: ep.entities.map((e) => ({ ...e, state_meta: undefined })),
    };
    const v = juger(sansMeta, [
      {
        connector: 'crm',
        object: 'lead',
        object_id: 'L-1',
        action: 'update_fields',
        fields: { statut: 'qualifie' },
      },
    ]);
    expect(v.verdict).toBe('desaccord');
    expect(v.champs.find((c) => c.champ === 'description')?.classe).toBe('manque');
  });

  it('le retrait laisse une TRACE, avec sa raison', () => {
    // Un retrait sans trace serait un verdict truque : le rapport dirait
    // « accord » sans dire sur quoi il a renonce a se prononcer — un verdict
    // d'autant plus flatteur qu'il regarde moins.
    const v = juger(episodeAvecInconnu(), []);
    expect(v.exclusions).toEqual([
      { champ: 'description', raison: 'champ non historise par le systeme' },
    ]);
  });

  it('un unknown_before sans raison le DIT plutot que de se taire', () => {
    const ep = episodeAvecInconnu();
    const sansRaison: Episode = {
      ...ep,
      entities: ep.entities.map((e) => ({
        ...e,
        state_meta: { description: { unknown_before: true } },
      })),
    };
    expect(juger(sansRaison, []).exclusions[0]?.raison).toContain('sans raison');
  });

  it('un champ RECONSTITUE reste juge — l historique a parle', () => {
    // `reconstituted` n'est pas `unknown_before`. Le premier dit « je sais, par
    // l'histoire » ; le second dit « je ne sais pas ». Les confondre retirerait
    // du verdict des champs parfaitement connus.
    const ep = episode({
      grade: 'A',
      scope_fields: ['statut'],
      entities: [
        {
          key: { type: 'contact', value_pseudo: 'EMAIL_aaa' },
          first_seen_seq: 1,
          api_refs: [{ connector: 'crm', object: 'lead', id: 'L-1' }],
          state_before: { statut: 'nouveau' },
          state_after: { statut: 'qualifie' },
          state_meta: { statut: { reconstituted: true } },
        },
      ],
    });
    const v = juger(ep, []);
    expect(v.champs.map((c) => c.champ)).toContain('statut');
    expect(v.exclusions).toEqual([]);
  });
});

describe('la normalisation des dates ne depend pas de la machine (R4.1)', () => {
  it('un horodatage sans fuseau est lu en UTC', () => {
    // `Date.parse('2026-08-15T00:00:00')` rend un instant en heure LOCALE. Le
    // meme corpus, juge sur deux postes, donnait deux valeurs normalisees
    // differentes — et l'empreinte qui en decoule aussi.
    //
    // Ce cas ne mord que sur une machine hors UTC : sur le runner d'integration,
    // qui est en UTC, il passe sans rien prouver. Les deux suivants, eux, mordent
    // partout — c'est pour ca qu'ils sont la.
    expect(normalize('2026-08-15T00:00:00')).toBe('2026-08-15T00:00:00.000Z');
    expect(normalize('2026-08-15T14:30:00')).toBe(normalize('2026-08-15T14:30:00Z'));
    expect(normalize('2026-08-15 14:30:00')).toBe(normalize('2026-08-15T14:30:00Z'));
  });

  it('un fuseau explicite est respecte', () => {
    expect(normalize('2026-08-15T02:00:00+02:00')).toBe('2026-08-15T00:00:00.000Z');
    expect(normalize('2026-08-14T20:00:00-04:00')).toBe('2026-08-15T00:00:00.000Z');
  });

  it('une date seule reste UTC, comme la norme le veut', () => {
    expect(normalize('2026-08-15')).toBe('2026-08-15T00:00:00.000Z');
  });

  it('une date ambigue reste une chaine', () => {
    // `15/08/2026` et `08/15/2026` sont LE MEME JOUR ecrit deux fois, et
    // `Date.parse` n'en lit qu'une : l'autre rend NaN. Deux ecritures du meme
    // jour ne pouvaient donc jamais etre equivalentes — mieux vaut n'en
    // convertir aucune que d'en convertir une seule.
    expect(normalize('15/08/2026')).toBe('15/08/2026');
    expect(normalize('08/15/2026')).toBe('08/15/2026');
    expect(equivalent('15/08/2026', '08/15/2026')).toBe(false);
  });

  it('un libelle de mois reste une chaine', () => {
    // Sa lecture depend de l'implementation ET du fuseau.
    expect(normalize('August 15, 2026')).toBe('August 15, 2026');
  });

  it('ce qui ne ressemble pas a une date n est pas converti', () => {
    expect(normalize('2026')).toBe(2026);
    expect(normalize('version 1.2.3')).toBe('version 1.2.3');
    expect(normalize('75011')).toBe(75011);
  });
});
