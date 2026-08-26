import { describe, expect, it } from 'vitest';
import { episodeValide } from './fixtures.js';
import {
  chercherPii,
  MOTIFS_PII,
  resumerOccurrences,
  VERSION_MOTIFS,
  validerRedaction,
} from './redaction.js';
import { Episode, gradeOf } from './schema.js';

/** Sous noUncheckedIndexedAccess, un index peut etre undefined. On le refuse ici. */
function requis<T>(v: T | undefined, quoi: string): T {
  if (v === undefined) throw new Error(`fixture invalide : ${quoi} manquant`);
  return v;
}

describe('bibliotheque de motifs', () => {
  it('est versionnee — un corpus juge sous v1 reste interpretable', () => {
    expect(VERSION_MOTIFS).toBe(1);
  });

  it('couvre les quatre familles exigees par R4.1', () => {
    expect(MOTIFS_PII.map((m) => m.type)).toEqual(
      expect.arrayContaining(['EMAIL', 'TEL_FR', 'IBAN', 'CARTE']),
    );
  });

  it('chaque motif compile', () => {
    for (const m of MOTIFS_PII) {
      expect(() => new RegExp(m.source, m.drapeaux)).not.toThrow();
    }
  });
});

describe('detection — vrais positifs', () => {
  it('detecte une adresse de courriel', () => {
    expect(chercherPii('ecrire a jean.dupont@example.com svp').map((o) => o.type)).toContain(
      'EMAIL',
    );
  });

  it('detecte un numero francais, quelle que soit la graphie', () => {
    for (const tel of ['0612345678', '06 12 34 56 78', '06.12.34.56.78', '+33612345678']) {
      expect(chercherPii(`tel ${tel}`).map((o) => o.type)).toContain('TEL_FR');
    }
  });

  it('detecte un IBAN', () => {
    expect(chercherPii('RIB FR7630006000011234567890189').map((o) => o.type)).toContain('IBAN');
  });

  it('detecte un numero de carte', () => {
    for (const c of ['4539148803436467', '4539 1488 0343 6467', '4539-1488-0343-6467']) {
      expect(chercherPii(c).map((o) => o.type)).toContain('CARTE');
    }
  });
});

describe('detection — faux positifs a eviter', () => {
  it('ne prend pas une date ISO pour un telephone', () => {
    expect(chercherPii('2026-08-15T00:00:00.000Z')).toEqual([]);
  });

  it('ne prend pas un ULID pour un IBAN', () => {
    expect(chercherPii('01JQA1B2C3D4E5F6G7H8J9K0M1')).toEqual([]);
  });

  it('ne prend pas une empreinte sha256 pour une PII', () => {
    expect(chercherPii('sha256:896242eff9')).toEqual([]);
  });

  it('laisse passer un token pseudonymise — c est le resultat attendu de la redaction', () => {
    expect(chercherPii('EMAIL_7f3a9c21 TEL_FR_4b81e0d2 IBAN_e1c07a45')).toEqual([]);
  });

  it('laisse passer du texte metier ordinaire', () => {
    expect(chercherPii('objection budget, rappeler apres arbitrage')).toEqual([]);
  });
});

describe('extraits — signaler sans recopier', () => {
  it('tronque la valeur trouvee', () => {
    const o = chercherPii('jean.dupont@example.com')[0];
    expect(o).toBeDefined();
    expect(o?.extrait).toContain('…');
    expect(o?.extrait).not.toContain('dupont');
  });

  it('resume par type et par nombre', () => {
    const r = resumerOccurrences(chercherPii('a@b.com et c@d.com et FR7630006000011234567890189'));
    expect(r).toContain('EMAIL');
    expect(r).toContain('IBAN');
  });
});

describe('validation d un episode entier (R4.6)', () => {
  it('valide un episode propre', () => {
    expect(validerRedaction(episodeValide()).valide).toBe(true);
  });

  it('refuse un episode dont un payload porte une PII', () => {
    const ep = episodeValide();
    const pollue = {
      ...ep,
      events: [
        { ...requis(ep.events[0], 'events[0]'), payload: 'contact: jean@example.com' },
        ...ep.events.slice(1),
      ],
    };
    expect(validerRedaction(pollue).valide).toBe(false);
  });

  it('refuse une PII cachee dans un nom accessible (R4.5)', () => {
    // Le vecteur du monde reel : « Email de Jean Dupont — jean@… » comme titre.
    const ep = episodeValide();
    const premier = requis(ep.events[0], 'events[0]');
    const pollue = {
      ...ep,
      events: [
        { ...premier, target: { role: 'link', name: 'Email de jean@example.com' } },
        ...ep.events.slice(1),
      ],
    };
    expect(validerRedaction(pollue).valide).toBe(false);
  });

  it('refuse une PII cachee dans un etat d entite', () => {
    const ep = episodeValide();
    const pollue = {
      ...ep,
      entities: [
        {
          ...requis(ep.entities[0], 'entities[0]'),
          state_after: { statut: 'FR7630006000011234567890189' },
        },
      ],
    };
    expect(validerRedaction(pollue).valide).toBe(false);
  });
});

describe('effet sur le grade (R4.6 branche sur la spec 001)', () => {
  it('un episode pollue tombe en C, avec le motif', () => {
    const ep = episodeValide();
    const pollue = {
      ...ep,
      events: [
        { ...requis(ep.events[0], 'events[0]'), payload: 'jean@example.com' },
        ...ep.events.slice(1),
      ],
    };
    const v = gradeOf(pollue);
    expect(v.grade).toBe('C');
    expect(v.reason).toContain('redaction non validee');
    expect(v.reason).toContain('EMAIL');
  });

  it('le schema refuse donc de parser un episode pollue declare A', () => {
    const ep = episodeValide();
    const pollue = {
      ...ep,
      events: [
        { ...requis(ep.events[0], 'events[0]'), payload: 'jean@example.com' },
        ...ep.events.slice(1),
      ],
    };
    expect(Episode.safeParse(pollue).success).toBe(false);
  });
});
