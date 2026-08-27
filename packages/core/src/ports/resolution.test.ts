/**
 * La résolution des candidates (spec 003, tâche 5).
 *
 * Les cas qui comptent sont ceux où le système refuse : un candidat, zéro, deux,
 * et deux graphies d'une même adresse qui doivent converger.
 */
import { describe, expect, it } from 'vitest';
import type { EntityCandidate } from './connector.js';
import {
  type CandidatDistant,
  memeCle,
  normaliserIdentifiant,
  raison,
  resoudre,
} from './resolution.js';

const MAINTENANT = '2026-01-14T09:12:00.000Z';

const parEmail = (jeton: string): EntityCandidate => ({
  type: 'contact',
  keys: [{ kind: 'email_token', value: jeton }],
});

const distant = (id: string, jeton: string): CandidatDistant => ({
  ref: { connector: 'fake', object: 'contact', id },
  keys: [{ kind: 'email_token', value: jeton }],
});

describe('un candidat, zero, deux (R2.2)', () => {
  it('un seul candidat resout, et dit par quelle cle', () => {
    const r = resoudre(parEmail('EMAIL_aaa'), [distant('C-1', 'EMAIL_aaa')], MAINTENANT);
    expect(r.status).toBe('resolved');
    if (r.status !== 'resolved') return;
    expect(r.ref.id).toBe('C-1');
    expect(r.by).toBe('email_token');
    expect(r.at).toBe(MAINTENANT);
  });

  it('zero candidat rend not_found', () => {
    const r = resoudre(parEmail('EMAIL_aaa'), [distant('C-1', 'EMAIL_bbb')], MAINTENANT);
    expect(r.status).toBe('not_found');
  });

  it('deux candidats rendent ambiguous AVEC leur nombre', () => {
    const r = resoudre(
      parEmail('EMAIL_aaa'),
      [distant('C-1', 'EMAIL_aaa'), distant('C-2', 'EMAIL_aaa')],
      MAINTENANT,
    );
    expect(r.status).toBe('ambiguous');
    if (r.status === 'ambiguous') expect(r.count).toBe(2);
  });

  it('une ambiguite N EST PAS departagee par une cle plus faible', () => {
    // C'est le coeur de R2.2. Affiner avec `domain_name` ce que `email_token`
    // n'a pas su trancher, c'est exactement deviner — avec la cle la PLUS FAIBLE
    // des trois, celle ou deux personnes peuvent legitimement se ressembler.
    const candidate: EntityCandidate = {
      type: 'contact',
      keys: [
        { kind: 'email_token', value: 'EMAIL_aaa' },
        { kind: 'domain_name', domain: 'exemple.fr', name: 'Dupont' },
      ],
    };
    const distants: CandidatDistant[] = [
      {
        ref: { connector: 'fake', object: 'contact', id: 'C-1' },
        keys: [
          { kind: 'email_token', value: 'EMAIL_aaa' },
          { kind: 'domain_name', domain: 'exemple.fr', name: 'Dupont' },
        ],
      },
      {
        ref: { connector: 'fake', object: 'contact', id: 'C-2' },
        keys: [{ kind: 'email_token', value: 'EMAIL_aaa' }],
      },
    ];
    const r = resoudre(candidate, distants, MAINTENANT);
    expect(r.status).toBe('ambiguous');
  });
});

describe('la comparaison en jetons (R6.2)', () => {
  it('deux graphies d une meme adresse donnent le meme identifiant normalise', () => {
    // La valeur claire ne vit qu'en memoire ; ce sont les JETONS qui se
    // comparent. Encore faut-il que les deux cotes normalisent pareil — sinon
    // « Jean.Dupont@Exemple.FR » et « jean.dupont@exemple.fr » sont deux
    // personnes, et la jointure est perdue sans que personne ne le voie.
    const graphies = [
      'Jean.Dupont@Exemple.FR',
      'jean.dupont@exemple.fr',
      '  jean.dupont@exemple.fr  ',
      'JEAN.DUPONT@EXEMPLE.FR',
    ];
    const normalisees = new Set(graphies.map((g) => normaliserIdentifiant('email_token', g)));
    expect(normalisees.size).toBe(1);
  });

  it('resout malgre une difference de casse des deux cotes', () => {
    const r = resoudre(
      parEmail('  Jean.Dupont@Exemple.FR '),
      [distant('C-1', 'jean.dupont@exemple.fr')],
      MAINTENANT,
    );
    expect(r.status).toBe('resolved');
  });

  it('un identifiant systeme garde sa casse — elle peut etre significative', () => {
    // Un identifiant opaque n'appartient pas a notre vocabulaire. Le mettre en
    // minuscules « pour normaliser » ferait matcher deux enregistrements
    // differents sur les systemes qui distinguent la casse.
    expect(normaliserIdentifiant('system_id', ' 003Ab000001XyZ ')).toBe('003Ab000001XyZ');
    expect(
      memeCle({ kind: 'system_id', value: '003Ab' }, { kind: 'system_id', value: '003ab' }),
    ).toBe(false);
  });
});

describe('l ordre des cles', () => {
  it('l identifiant systeme tranche avant le courriel', () => {
    // Le systeme lui-meme l'a emis : il ne peut pas designer deux
    // enregistrements. Le courriel, si — une adresse partagee, un alias.
    const candidate: EntityCandidate = {
      type: 'contact',
      keys: [
        { kind: 'system_id', value: 'C-9' },
        { kind: 'email_token', value: 'EMAIL_aaa' },
      ],
    };
    const distants: CandidatDistant[] = [
      {
        ref: { connector: 'fake', object: 'contact', id: 'C-9' },
        keys: [{ kind: 'system_id', value: 'C-9' }],
      },
      {
        ref: { connector: 'fake', object: 'contact', id: 'C-1' },
        keys: [{ kind: 'email_token', value: 'EMAIL_aaa' }],
      },
    ];
    const r = resoudre(candidate, distants, MAINTENANT);
    expect(r.status === 'resolved' && r.by).toBe('system_id');
    expect(r.status === 'resolved' && r.ref.id).toBe('C-9');
  });

  it('une cle absente ne bloque pas la suivante', () => {
    const candidate: EntityCandidate = {
      type: 'contact',
      keys: [{ kind: 'email_token', value: 'EMAIL_aaa' }],
    };
    const r = resoudre(candidate, [distant('C-1', 'EMAIL_aaa')], MAINTENANT);
    expect(r.status === 'resolved' && r.by).toBe('email_token');
  });

  it('le couple domaine + nom resout quand il est seul et exact', () => {
    const candidate: EntityCandidate = {
      type: 'compte',
      keys: [{ kind: 'domain_name', domain: 'Exemple.FR', name: 'Dupont SA' }],
    };
    const distants: CandidatDistant[] = [
      {
        ref: { connector: 'fake', object: 'compte', id: 'A-1' },
        keys: [{ kind: 'domain_name', domain: 'exemple.fr', name: 'dupont sa' }],
      },
    ];
    expect(resoudre(candidate, distants, MAINTENANT).status).toBe('resolved');
  });

  it('un nom approchant ne resout RIEN', () => {
    // Pas de distance d'edition, pas de score, pas de « meilleur candidat ».
    // Une resolution floue qui se trompe attribue le travail d'un operateur au
    // dossier de quelqu'un d'autre.
    const candidate: EntityCandidate = {
      type: 'compte',
      keys: [{ kind: 'domain_name', domain: 'exemple.fr', name: 'Dupont SA' }],
    };
    const distants: CandidatDistant[] = [
      {
        ref: { connector: 'fake', object: 'compte', id: 'A-1' },
        keys: [{ kind: 'domain_name', domain: 'exemple.fr', name: 'Dupond SA' }],
      },
    ];
    expect(resoudre(candidate, distants, MAINTENANT).status).toBe('not_found');
  });
});

describe('la raison rendue a l episode (R2.2)', () => {
  it('distingue not_found et ambiguous', () => {
    // Les deux n'appellent pas le meme geste : « il n'existe pas » et « il y en a
    // trop » se corrigent differemment, et « non resolu » tout court laisse
    // chercher au mauvais endroit.
    expect(raison({ status: 'not_found' })).toBe('not_found');
    expect(raison({ status: 'ambiguous', count: 3 })).toBe('ambiguous:3');
  });

  it('une resolution dit par quelle cle et quand (R2.3)', () => {
    const texte = raison({
      status: 'resolved',
      ref: { connector: 'fake', object: 'contact', id: 'C-1' },
      by: 'email_token',
      at: MAINTENANT,
    });
    expect(texte).toContain('email_token');
    expect(texte).toContain(MAINTENANT);
  });
});
