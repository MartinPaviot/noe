/**
 * La réconciliation (spec 003, tâche 8).
 *
 * L'invariant qu'on protège ici tient en une phrase : **chaque changement finit
 * dans exactement une colonne**. Le reste des tests explique pourquoi chaque
 * colonne est celle-là et pas une autre.
 */
import { describe, expect, it } from 'vitest';
import type { ApiChange } from './connector.js';
import { type ActionUi, FENETRE_JOINTURE_MS, reconcilier, tauxExplique } from './reconciliation.js';

const REF = { connector: 'crm', object: 'lead', id: 'L-1' };
const AUTRE = { connector: 'crm', object: 'lead', id: 'L-2' };
const SCOPE = ['statut', 'montant'];

const change = (at: string, fields: string[], actor: string | null = null): ApiChange => ({
  ref: REF,
  at,
  fields,
  actor,
});

const action = (seq: number, at: string, refId = 'L-1'): ActionUi => ({
  seq,
  at,
  refId,
  fields: ['statut'],
});

describe('chaque changement finit dans exactement une colonne (R4.2)', () => {
  it('les trois colonnes somment au total, toujours', () => {
    const changes = [
      change('2026-01-14T09:00:00.000Z', ['statut']),
      change('2026-01-14T09:05:00.000Z', ['champ_hors_scope']),
      change('2026-01-14T09:10:00.000Z', ['montant']),
      change('2026-01-14T09:20:00.000Z', ['statut'], 'collegue'),
    ];
    const b = reconcilier(changes, [action(1, '2026-01-14T09:00:10.000Z')], SCOPE, [], 'operateur');
    expect(b.explique + b.hors_perimetre + b.trous).toBe(changes.length);
    expect(b.lignes.length).toBe(changes.length);
  });

  it('aucun changement ne disparait, meme quand rien ne colle', () => {
    // Un changement perdu se lit comme un systeme qui n'a rien fait. C'est la
    // pire des trois erreurs possibles, parce qu'elle est invisible.
    const changes = [change('2026-01-14T09:00:00.000Z', ['statut'])];
    const b = reconcilier(changes, [], SCOPE, []);
    expect(b.lignes.length).toBe(1);
    expect(b.trous).toBe(1);
  });
});

describe('la jointure a trente secondes (R4.1)', () => {
  it('explique un changement colle a une action de la meme entite', () => {
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['statut'])],
      [action(7, '2026-01-14T09:00:20.000Z')],
      SCOPE,
      [],
    );
    expect(b.explique).toBe(1);
    expect(b.lignes[0]?.colonne).toEqual({ kind: 'explique', seqUi: 7 });
  });

  it('au-dela de trente secondes, ce n est plus la meme histoire', () => {
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['statut'])],
      [action(7, '2026-01-14T09:00:31.000Z')],
      SCOPE,
      [],
    );
    expect(b.trous).toBe(1);
    expect(FENETRE_JOINTURE_MS).toBe(30_000);
  });

  it('une action sur une AUTRE entite n explique rien', () => {
    // La jointure est « meme entite ET fenetre ». Relacher l'entite ferait
    // expliquer un changement par un geste fait ailleurs, sur un autre dossier.
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['statut'])],
      [{ seq: 7, at: '2026-01-14T09:00:05.000Z', refId: AUTRE.id, fields: ['statut'] }],
      SCOPE,
      [],
    );
    expect(b.trous).toBe(1);
  });

  it('prend l action la PLUS PROCHE, pas la premiere venue', () => {
    // Sinon le rapport attribue le changement a une action anterieure alors
    // qu'une action posterieure le colle mieux — et dit la mauvaise cause.
    const b = reconcilier(
      [change('2026-01-14T09:00:20.000Z', ['statut'])],
      [action(1, '2026-01-14T09:00:00.000Z'), action(2, '2026-01-14T09:00:19.000Z')],
      SCOPE,
      [],
    );
    expect(b.lignes[0]?.colonne).toEqual({ kind: 'explique', seqUi: 2 });
  });
});

describe('le hors-perimetre (R4.2)', () => {
  it('un champ hors scope sort, avec sa raison', () => {
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['notes_internes'])],
      [action(1, '2026-01-14T09:00:01.000Z')],
      SCOPE,
      [],
    );
    expect(b.hors_perimetre).toBe(1);
    const c = b.lignes[0]?.colonne;
    expect(c?.kind === 'hors_perimetre' && c.raison).toContain('notes_internes');
  });

  it('un autre acteur sort AVANT le test de perimetre', () => {
    // Un collegue, un automatisme, une integration : le changement est reel, il
    // n'est pas de nous. Le compter comme un trou accuserait la capture d'avoir
    // rate quelque chose qu'elle n'avait aucune raison de voir.
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['statut'], 'collegue')],
      [],
      SCOPE,
      [],
      'operateur',
    );
    expect(b.hors_perimetre).toBe(1);
    const c = b.lignes[0]?.colonne;
    expect(c?.kind === 'hors_perimetre' && c.raison).toContain('collegue');
  });

  it('un acteur INCONNU ne vaut pas « l operateur »', () => {
    // `actor: null` veut dire « le systeme ne l'expose pas ». Supposer
    // l'operateur expliquerait des changements qu'il n'a pas faits, et
    // gonflerait la metrique de sante exactement la ou elle doit alerter.
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['statut'], null)],
      [],
      SCOPE,
      [],
      'operateur',
    );
    expect(b.hors_perimetre).toBe(0);
    expect(b.trous).toBe(1);
  });

  it('un changement mixte, dont UN champ est dans le scope, reste jugeable', () => {
    const b = reconcilier(
      [change('2026-01-14T09:00:00.000Z', ['statut', 'notes_internes'])],
      [action(1, '2026-01-14T09:00:02.000Z')],
      SCOPE,
      [],
    );
    expect(b.explique).toBe(1);
  });
});

describe('les trous et leur sous-cause (R4.2)', () => {
  it('dans un trou declare : attendu, on savait qu on ne regardait pas', () => {
    const b = reconcilier([change('2026-01-14T09:05:00.000Z', ['statut'])], [], SCOPE, [
      { from: '2026-01-14T09:00:00.000Z', to: '2026-01-14T09:10:00.000Z' },
    ]);
    expect(b.lignes[0]?.colonne).toEqual({ kind: 'trou', sousCause: 'dans_gap_declare' });
    expect(b.trous_hors_gap).toBe(0);
  });

  it('hors de tout trou : LE signal d alarme', () => {
    // Le monde a bouge pendant qu'on croyait observer, et on n'a rien vu. C'est
    // le seul cas ou la capture a vraiment failli.
    const b = reconcilier([change('2026-01-14T09:30:00.000Z', ['statut'])], [], SCOPE, [
      { from: '2026-01-14T09:00:00.000Z', to: '2026-01-14T09:10:00.000Z' },
    ]);
    expect(b.lignes[0]?.colonne).toEqual({ kind: 'trou', sousCause: 'hors_gap' });
    expect(b.trous_hors_gap).toBe(1);
  });

  it('les bornes du trou sont incluses', () => {
    const trou = { from: '2026-01-14T09:00:00.000Z', to: '2026-01-14T09:10:00.000Z' };
    for (const at of [trou.from, trou.to]) {
      const b = reconcilier([change(at, ['statut'])], [], SCOPE, [trou]);
      expect(b.trous_hors_gap, at).toBe(0);
    }
  });
});

describe('le taux d explique (R4.3)', () => {
  it('exclut le hors-perimetre du denominateur', () => {
    // Sinon le taux baisse quand un collegue travaille, et monte quand il part
    // en vacances. Ce serait une mesure de l'activite des autres, pas de la
    // qualite de notre capture.
    const b = reconcilier(
      [
        change('2026-01-14T09:00:00.000Z', ['statut']),
        change('2026-01-14T09:05:00.000Z', ['notes_internes']),
        change('2026-01-14T09:10:00.000Z', ['notes_internes']),
      ],
      [action(1, '2026-01-14T09:00:05.000Z')],
      SCOPE,
      [],
    );
    expect(b.explique).toBe(1);
    expect(b.hors_perimetre).toBe(2);
    expect(tauxExplique(b)).toBe(1);
  });

  it('rend null quand il n y a rien a mesurer', () => {
    // Zero pour cent sur zero changement serait un chiffre faux, et il
    // descendrait la moyenne du jour.
    expect(tauxExplique(reconcilier([], [], SCOPE, []))).toBeNull();
  });

  it('compte les trous au denominateur', () => {
    const b = reconcilier(
      [
        change('2026-01-14T09:00:00.000Z', ['statut']),
        change('2026-01-14T09:30:00.000Z', ['statut']),
      ],
      [action(1, '2026-01-14T09:00:05.000Z')],
      SCOPE,
      [],
    );
    expect(tauxExplique(b)).toBe(0.5);
  });
});
