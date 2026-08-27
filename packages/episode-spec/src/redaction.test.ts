import { describe, expect, it } from 'vitest';
import { episodeValide } from './fixtures.js';
import {
  chercherCompact,
  chercherPii,
  MOTIFS_PII,
  normaliserBlancs,
  resoudreChevauchements,
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
  // La version est ÉPINGLÉE, à dessein : la bumper doit être une décision, pas
  // un effet de bord. Ce test a fait son travail — il a rougi quand la v2 a
  // corrigé le trou téléphonique, et a forcé l'entrée dans `decisions.md`.
  it('est versionnee — un corpus juge sous une version anterieure reste interpretable', () => {
    expect(VERSION_MOTIFS).toBe(4);
  });

  // Les trois graphies trouvees par revue adverse (D29). Chacune traversait la
  // redaction en clair ; chacune est une forme d'affichage courante, pas une
  // curiosite de laboratoire.
  it.each([
    ['parenthese de conduite, mobile', 'Mobile +33 (0)6 12 34 56 78'],
    ['parenthese de conduite, fixe', 'Standard +33 (0)1 42 68 53 00'],
    ['parenthese sans separateurs', 'Direct +33 (0)612345678'],
    ['indicatif 0033', 'Depuis l etranger 0033 6 12 34 56 78'],
    ['espace insecable', 'Ligne 06\u00a012\u00a034\u00a056\u00a078'],
    ['espace insecable etroite', 'Ligne 06\u202f12\u202f34\u202f56\u202f78'],
    ['espace de largeur nulle', 'Ligne 06\u200b12\u200b34\u200b56\u200b78'],
  ])('detecte un numero francais ecrit avec %s', (_nom, texte) => {
    expect(chercherPii(texte).map((o) => o.type)).toContain('TEL_FR');
  });

  it('ne fabrique pas de numero en collant deux groupes separes', () => {
    // La normalisation remplace le caractere de largeur nulle par une espace,
    // pas par rien : sinon `06<ZWSP>12` se lirait `0612` et on inventerait une
    // graphie que personne n'a ecrite.
    expect(normaliserBlancs('06\u200b12')).toBe('06 12');
  });

  it('les graphies d un meme numero convergent apres compactage', () => {
    const compactes = [
      '+33 (0)6 12 34 56 78',
      '+33 6 12 34 56 78',
      '06 12 34 56 78',
      '0033 6 12 34 56 78',
      '06\u00a012\u00a034\u00a056\u00a078',
    ].map((t) => chercherCompact(t).map((o) => o.type));
    for (const c of compactes) expect(c).toContain('TEL_FR_COMPACT');
  });

  // Le filet doit rester muet sur ce qui n'est pas un numero, sinon il declasse
  // des episodes honnetes sans recours.
  it.each([
    ['un horodatage en millisecondes', '1767225600000'],
    ['une date ISO', '2026-01-14T09:12:03.000Z'],
    ['un montant', 'Montant 1 234,56 EUR'],
    ['un code postal', 'Code postal 75011 Paris'],
    ['un SIRET', 'SIRET 12345678900011'],
    ['un ULID', '01JQA1B2C3D4E5F6G7H8J9K0M1'],
    ['un jeton de redaction', 'TEL_FR_1a2b3c4d'],
  ])('le filet ne mord pas sur %s', (_nom, texte) => {
    expect(chercherCompact(texte)).toEqual([]);
  });

  it('le filet voit ce que la bibliotheque raterait', () => {
    // Une graphie qu'aucun motif ne connait : le tiret cadratin comme
    // separateur. La bibliotheque ne la voit pas ; le filet, si — et c'est
    // exactement sa raison d'etre. Le jour ou il parle seul, c'est la
    // bibliotheque qu'il faut corriger.
    const exotique = '06\u201412\u201434\u201456\u201478';
    expect(chercherPii(exotique).map((o) => o.type)).not.toContain('TEL_FR');
    expect(chercherCompact(exotique).map((o) => o.type)).toContain('TEL_FR_COMPACT');
  });

  it('validerRedaction refuse un episode que le filet seul denonce', () => {
    const episode = { events: [{ target: { name: '06\u201412\u201434\u201456\u201478' } }] };
    const v = validerRedaction(episode);
    expect(v.valide).toBe(false);
    expect(v.occurrences.map((o) => o.type)).toContain('TEL_FR_COMPACT');
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

/**
 * Les formes d'un meme numero francais.
 *
 * Le trou de la v1 vivait exactement entre deux motifs : TEL_FR refusait un
 * separateur apres l'indicatif, TEL_INTL excluait l'indicatif francais. Un
 * numero ecrit « +33 6 12 34 56 78 » n'etait donc reclame par personne. Ce
 * tableau enumere les formes qu'un humain ecrit reellement, parce que c'est la
 * seule facon de ne pas re-creuser le meme trou ailleurs.
 */
describe('TEL_FR couvre les formes reellement ecrites (v2)', () => {
  const formes = [
    '+33 6 12 34 56 78',
    '+33612345678',
    '+33-6-12-34-56-78',
    '+33.6.12.34.56.78',
    '06 12 34 56 78',
    '0612345678',
    '06.12.34.56.78',
    '06-12-34-56-78',
    '+33 1 45 67 89 01',
  ];

  for (const forme of formes) {
    it(`detecte « ${forme} »`, () => {
      const trouvees = chercherPii(`Ligne directe ${forme} merci`);
      expect(trouvees.map((o) => o.type)).toContain('TEL_FR');
    });
  }

  const jamais = [
    'Reference interne 2026-08-26',
    'Version 1.2.3 du connecteur',
    'Montant 1 234,56 EUR',
    'Code postal 75011 Paris',
    'Piste ouverte il y a 12 jours',
  ];

  for (const texte of jamais) {
    it(`ne se declenche pas sur « ${texte} »`, () => {
      expect(chercherPii(texte).map((o) => o.type)).not.toContain('TEL_FR');
    });
  }
});

/**
 * L'arbitrage des chevauchements decide quel JETON un texte produira.
 *
 * Ce n'est pas un detail d'implementation : deux jetons differents pour une meme
 * entite, c'est une jointure perdue dans le graphe. La regle doit donc etre
 * deterministe, et identique dans les trois moteurs.
 */
describe('arbitrage des chevauchements', () => {
  it('un IBAN l emporte sur le motif telephonique qu il contient', () => {
    const brutes = chercherPii('Virement sur FR7630006000011234567890189');
    expect(brutes.map((o) => o.type)).toEqual(expect.arrayContaining(['IBAN', 'TEL_FR']));

    const retenues = resoudreChevauchements(brutes);
    expect(retenues.map((o) => o.type)).toEqual(['IBAN']);
  });

  it('un numero francais rend TEL_FR, jamais TEL_INTL', () => {
    // La v2 l obtenait par une anticipation negative que Rust ne sait pas lire.
    // La v3 l obtient par la priorite, qui se lit partout.
    for (const forme of ['+33 6 12 34 56 78', '+33612345678']) {
      const retenues = resoudreChevauchements(chercherPii(`tel ${forme}`));
      expect(retenues.map((o) => o.type)).toEqual(['TEL_FR']);
    }
  });

  it('un numero etranger reste TEL_INTL', () => {
    const retenues = resoudreChevauchements(chercherPii('Numero belge +32 471 12 34 56'));
    expect(retenues.map((o) => o.type)).toEqual(['TEL_INTL']);
  });

  it('deux PII disjointes sont toutes deux retenues', () => {
    const retenues = resoudreChevauchements(
      chercherPii('Deux a la fois : a@b.fr et 06.12.34.56.78'),
    );
    expect(retenues.map((o) => o.type)).toEqual(['EMAIL', 'TEL_FR']);
  });

  it('les retenues ne se chevauchent jamais', () => {
    const retenues = resoudreChevauchements(
      chercherPii('FR7630006000011234567890189 puis a@b.fr puis 4970 1234 5678 9012'),
    );
    for (let i = 1; i < retenues.length; i++) {
      const precedente = retenues[i - 1];
      const courante = retenues[i];
      expect(precedente).toBeDefined();
      expect(courante).toBeDefined();
      expect(courante?.index).toBeGreaterThanOrEqual(precedente?.fin ?? 0);
    }
  });

  it('est stable : deux appels rendent le meme resultat', () => {
    const brutes = chercherPii('FR7630006000011234567890189 et 06 12 34 56 78');
    expect(resoudreChevauchements(brutes)).toEqual(resoudreChevauchements(brutes));
  });
});

describe('aucun motif n utilise une syntaxe hors du sous-ensemble commun', () => {
  // Le moteur de Rust ne connait ni anticipation ni retrospection. Une
  // bibliotheque destinee a trois moteurs doit tenir dans leur intersection —
  // sinon la promesse « les motifs sont lus tels quels » est fausse, et on ne
  // s en apercoit qu en compilant l adaptateur natif.
  it('pas d anticipation ni de retrospection', () => {
    for (const m of MOTIFS_PII) {
      expect(m.source, `${m.type} contient une anticipation`).not.toMatch(/\(\?[=!]/);
      expect(m.source, `${m.type} contient une retrospection`).not.toMatch(/\(\?<[=!]/);
    }
  });

  it('chaque motif porte une priorite distincte', () => {
    const priorites = MOTIFS_PII.map((m) => m.priorite);
    expect(new Set(priorites).size, 'deux motifs a egalite rendraient l arbitrage ambigu').toBe(
      priorites.length,
    );
  });
});
