/**
 * Le port de fédération et son connecteur de banc (spec 003, tâche 1).
 *
 * Ce qui se vérifie ici, ce sont les **refus**. Un connecteur qui répond bien
 * quand tout va bien ne prouve rien : c'est quand le système distant est ambigu,
 * en colère, ou amnésique que le domaine doit tenir sans mentir.
 */
import { describe, expect, it } from 'vitest';
import type { ApiChange, EntityCandidate, HistoryPoint } from './connector.js';
import { FakeConnector, verdictAvant } from './fake-connector.js';

const contact = (): EntityCandidate => ({
  type: 'contact',
  keys: [{ kind: 'email_token', value: 'EMAIL_blvbgfywcnrhm' }],
});

const REF = { connector: 'fake', object: 'contact', id: 'FAKE-0001' };

describe('le port de federation', () => {
  it("n'expose aucune ecriture", async () => {
    // R « hors perimetre explicite » : la promotion appartient a une spec
    // ulterieure. Un port qui exposerait `write` « pour plus tard » laisserait un
    // adaptateur l'implementer, puis un appelant l'appeler.
    const c = new FakeConnector();
    const verbes = Object.getOwnPropertyNames(Object.getPrototypeOf(c));
    expect(verbes).not.toContain('write');
    expect(verbes).not.toContain('update');
    expect(verbes).not.toContain('create');
    expect(verbes.filter((v) => v !== 'constructor')).toEqual(
      expect.arrayContaining(['resolve', 'read', 'changes', 'history']),
    );
  });
});

describe('scenario 1 — la resolution ambigue (R2.2)', () => {
  it('un seul candidat resout, et dit par quelle cle', async () => {
    const r = await new FakeConnector({ candidats: 1 }).resolve(contact());
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.value.status).toBe('resolved');
    if (r.value.status !== 'resolved') return;
    // R2.3 : la cle qui a tranche, et quand. Sans elle, une resolution fausse
    // est indiagnosticable — on ne sait meme pas ce qui l'a decidee.
    expect(r.value.by).toBe('email_token');
    expect(r.value.at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it('zero candidat rend not_found, pas une devinette', async () => {
    const r = await new FakeConnector({ candidats: 0 }).resolve(contact());
    expect(r.ok && r.value.status).toBe('not_found');
  });

  it('deux candidats rendent ambiguous AVEC leur nombre', async () => {
    // « Non resolu » tout court ne se corrige pas : `not_found` et `ambiguous:2`
    // appellent deux gestes differents, et l'episode doit laisser la prise.
    const r = await new FakeConnector({ candidats: 2 }).resolve(contact());
    expect(r.ok).toBe(true);
    if (!r.ok || r.value.status !== 'ambiguous') {
      expect.fail(`attendu ambiguous, recu ${JSON.stringify(r)}`);
      return;
    }
    expect(r.value.count).toBe(2);
  });

  it('une candidate sans cle forte ne devine pas non plus', async () => {
    // « Rien a chercher » n'est pas « absent du systeme ». Les confondre ferait
    // croire a une absence cote CRM, et on chercherait le probleme au mauvais
    // endroit.
    const r = await new FakeConnector().resolve({ type: 'contact', keys: [] });
    expect(r.ok && r.value.status).toBe('not_found');
  });
});

describe('scenario 2 — la rafale de 429 (R5.1)', () => {
  it('rend une erreur retryable tant que la rafale dure', async () => {
    const c = new FakeConnector({ rafale429: 3 });
    for (let i = 0; i < 3; i += 1) {
      const r = await c.resolve(contact());
      expect(r.ok, `appel ${i + 1}`).toBe(false);
      if (!r.ok) expect(r.error.kind).toBe('retryable_exhausted');
    }
    // La rafale finie, le connecteur repond.
    const apres = await c.resolve(contact());
    expect(apres.ok).toBe(true);
  });

  it('ne retente PAS tout seul — sinon le budget ne verrait rien passer', async () => {
    // R5.3 compte les appels par episode. Un connecteur qui retenterait en
    // interne cacherait la rafale au client commun, et le budget serait faux.
    const c = new FakeConnector({ rafale429: 2 });
    await c.resolve(contact());
    expect(c.compteur()).toBe(1);
  });
});

describe('scenario 3 — l ecriture avant la lecture (R3.3)', () => {
  it('l historique revele une ecriture anterieure, et on reconstitue', () => {
    const histoire: HistoryPoint[] = [
      { at: '2026-01-14T09:00:00.000Z', field: 'statut', from: 'nouveau', to: 'qualifie' },
    ];
    const v = verdictAvant('statut', new Set(['statut']), histoire, '2026-01-14T09:12:00.000Z');
    expect(v.kind).toBe('reconstitue');
    if (v.kind === 'reconstitue') expect(v.valeur).toBe('nouveau');
  });

  it('prend la PLUS ANCIENNE ecriture anterieure, pas la derniere', () => {
    // La plus ancienne porte la valeur d'origine. Prendre la plus recente
    // rendrait une valeur intermediaire, et le diff avec `state_after`
    // raconterait une moitie de l'histoire — en ayant l'air complet.
    const histoire: HistoryPoint[] = [
      { at: '2026-01-14T09:05:00.000Z', field: 'statut', from: 'contacte', to: 'qualifie' },
      { at: '2026-01-14T09:00:00.000Z', field: 'statut', from: 'nouveau', to: 'contacte' },
    ];
    const v = verdictAvant('statut', new Set(['statut']), histoire, '2026-01-14T09:12:00.000Z');
    expect(v.kind === 'reconstitue' && v.valeur).toBe('nouveau');
  });

  it('sans ecriture anterieure, la lecture directe fait foi', () => {
    const histoire: HistoryPoint[] = [
      // Posterieure a la premiere lecture : elle ne dit rien de l'avant.
      { at: '2026-01-14T09:30:00.000Z', field: 'statut', from: 'qualifie', to: 'gagne' },
    ];
    const v = verdictAvant('statut', new Set(['statut']), histoire, '2026-01-14T09:12:00.000Z');
    expect(v.kind).toBe('lecture_directe');
  });

  it('le banc rend bien une ecriture anterieure quand on la demande', async () => {
    const c = new FakeConnector({ ecritureAvantLecture: true });
    const r = await c.history(REF, 'statut', {
      from: '2026-01-14T09:00:00.000Z',
      to: '2026-01-14T09:12:00.000Z',
    });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value.length).toBe(1);
  });
});

describe('scenario 4 — le champ non historise (R3.3)', () => {
  it('rend unknown_before AVEC sa raison, jamais un silence', () => {
    // « Jamais silencieusement compte » est le mot de l'exigence. Un champ exclu
    // sans raison est indistinguable d'un champ oublie.
    const v = verdictAvant('description', new Set(['statut']), [], '2026-01-14T09:12:00.000Z');
    expect(v.kind).toBe('inconnu');
    if (v.kind === 'inconnu') {
      expect(v.raison).toContain('non historise');
      expect(v.raison.length).toBeGreaterThan(20);
    }
  });

  it('un historique vide sur un champ SUIVI ne vaut pas inconnu', () => {
    // Le piege du scenario : « aucun changement » et « je ne sais pas » se
    // ressemblent, et menent a des conclusions opposees. Seule la liste des
    // champs suivis les distingue.
    const v = verdictAvant('statut', new Set(['statut']), [], '2026-01-14T09:12:00.000Z');
    expect(v.kind).toBe('lecture_directe');
  });

  it('le banc rend une liste vide pour un champ non suivi, comme le vrai systeme', async () => {
    const c = new FakeConnector({ champsNonHistorises: ['description'] });
    const r = await c.history(REF, 'description', {
      from: '2026-01-14T09:00:00.000Z',
      to: '2026-01-14T09:12:00.000Z',
    });
    // Une liste vide, PAS une erreur : c'est ce que fait Salesforce, et c'est ce
    // qui rend le cas piegeux.
    expect(r.ok && r.value).toEqual([]);
  });
});

describe('la lecture reste dans son perimetre (R3.1)', () => {
  it('ne rend que les champs demandes', async () => {
    const c = new FakeConnector({ etat: { statut: 'qualifie', montant: 4200, secret: 'x' } });
    const r = await c.read(REF, ['statut']);
    expect(r.ok && r.value).toEqual({ statut: 'qualifie' });
  });

  it('un champ absent du systeme ne s invente pas', async () => {
    const c = new FakeConnector({ etat: { statut: 'qualifie' } });
    const r = await c.read(REF, ['statut', 'inexistant']);
    expect(r.ok && Object.keys(r.value)).toEqual(['statut']);
  });
});

describe('le delta (R4)', () => {
  it('ne rend que les changements posterieurs a l instant demande', async () => {
    const changements: ApiChange[] = [
      { ref: REF, at: '2026-01-14T09:00:00.000Z', fields: ['statut'], actor: null },
      { ref: REF, at: '2026-01-14T09:20:00.000Z', fields: ['montant'], actor: 'operateur' },
    ];
    const c = new FakeConnector({ changements });
    const r = await c.changes(REF, '2026-01-14T09:10:00.000Z');
    expect(r.ok && r.value.map((x) => x.fields[0])).toEqual(['montant']);
  });

  it('un acteur inconnu vaut null, jamais « l operateur »', async () => {
    // R4.2 range un changement d'un autre acteur hors perimetre. Supposer
    // l'operateur par defaut expliquerait des changements qu'il n'a pas faits —
    // et gonflerait le taux d'explique, qui est LA metrique de sante.
    const changements: ApiChange[] = [
      { ref: REF, at: '2026-01-14T09:20:00.000Z', fields: ['statut'], actor: null },
    ];
    const r = await new FakeConnector({ changements }).changes(REF, '2026-01-14T09:00:00.000Z');
    expect(r.ok && r.value[0]?.actor).toBeNull();
  });
});
