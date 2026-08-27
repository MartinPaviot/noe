/**
 * Le client commun sous mauvais traitement (spec 003, tâche 3).
 *
 * Chaque test met l'API dans un état de colère précis et vérifie que le client
 * ne fait ni l'une ni l'autre des deux fautes : crasher, ou mentir.
 */
import { describe, expect, it, vi } from 'vitest';
import {
  appeler,
  BUDGET_PAR_EPISODE,
  Budget,
  consequence,
  DELAI_MAX_MS,
  delaiMs,
  type Reponse,
  TENTATIVES_MAX,
} from './client.js';

/** Un dormeur qui ne dort pas : sinon un test de backoff dure huit secondes. */
const dormirFaux = () => {
  const attentes: number[] = [];
  return {
    attentes,
    dormir: async (ms: number) => {
      attentes.push(ms);
    },
  };
};

const alea = () => 0.5;

describe('le backoff (R5.1)', () => {
  it('croit exponentiellement et se plafonne', () => {
    const suite = [1, 2, 3, 4, 5, 6, 7, 8].map((n) => delaiMs(n, () => 1));
    for (let i = 1; i < suite.length; i += 1) {
      expect(suite[i]!).toBeGreaterThanOrEqual(suite[i - 1]!);
    }
    expect(suite.at(-1)).toBeLessThanOrEqual(DELAI_MAX_MS);
  });

  it('porte un jitter — sans lui, la deuxieme vague est aussi serree que la premiere', () => {
    // Tous les clients qui prennent un 429 au meme instant reviennent au meme
    // instant. Le hasard les etale ; c'est la seule raison d'etre du jitter.
    const bas = delaiMs(3, () => 0);
    const haut = delaiMs(3, () => 1);
    expect(bas).toBeLessThan(haut);
    expect(bas).toBeGreaterThan(0);
  });

  it('respecte Retry-After quand le serveur en donne un', async () => {
    // Le serveur SAIT quand il sera pret ; nous devinons. Ignorer son en-tete est
    // la facon la plus courante de transformer une limitation en bannissement.
    const { attentes, dormir } = dormirFaux();
    let n = 0;
    const tenter = async (): Promise<Reponse<string>> => {
      n += 1;
      return n === 1
        ? { kind: 'retryable', retryAfterMs: 4321, detail: '429' }
        : { kind: 'ok', value: 'enfin' };
    };
    const r = await appeler(tenter, { budget: new Budget(), dormir, alea });
    expect(r.ok && r.value).toBe('enfin');
    expect(attentes).toEqual([4321]);
  });
});

describe('la rafale de 429 (R5.1)', () => {
  it('retente jusqu au plafond, puis classe retryable_exhausted', async () => {
    const { attentes, dormir } = dormirFaux();
    const tenter = async (): Promise<Reponse<string>> => ({
      kind: 'retryable',
      detail: '429 en rafale',
    });
    const r = await appeler(tenter, { budget: new Budget(100), dormir, alea });
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error.kind).toBe('retryable_exhausted');
      if (r.error.kind === 'retryable_exhausted') expect(r.error.attempts).toBe(TENTATIVES_MAX);
    }
    // Cinq tentatives, donc quatre attentes : on ne dort pas apres la derniere.
    expect(attentes.length).toBe(TENTATIVES_MAX - 1);
  });

  it('rend la main des que l API repond', async () => {
    const { dormir } = dormirFaux();
    let n = 0;
    const tenter = async (): Promise<Reponse<number>> => {
      n += 1;
      return n < 3 ? { kind: 'retryable', detail: '503' } : { kind: 'ok', value: n };
    };
    const r = await appeler(tenter, { budget: new Budget(), dormir, alea });
    expect(r.ok && r.value).toBe(3);
  });
});

describe('le 401 et le rafraichissement (R5.1)', () => {
  it('rafraichit puis rejoue, sans attendre', async () => {
    const { attentes, dormir } = dormirFaux();
    let n = 0;
    const tenter = async (): Promise<Reponse<string>> => {
      n += 1;
      return n === 1
        ? { kind: 'unauthorized', detail: 'jeton expire' }
        : { kind: 'ok', value: 'ok' };
    };
    const rafraichir = vi.fn(async () => true);
    const r = await appeler(tenter, { budget: new Budget(), dormir, alea, rafraichir });
    expect(r.ok && r.value).toBe('ok');
    expect(rafraichir).toHaveBeenCalledTimes(1);
    // Le jeton est neuf : rien ne justifie d'attendre.
    expect(attentes).toEqual([]);
  });

  it('ne rafraichit QU UNE FOIS — boucler brulerait le budget', async () => {
    // Deux 401 de suite apres un refresh reussi veulent dire autre chose qu'un
    // jeton expire. Reboucler dessus depenserait le quota sur un probleme que le
    // refresh ne resout pas.
    const { dormir } = dormirFaux();
    const tenter = async (): Promise<Reponse<string>> => ({
      kind: 'unauthorized',
      detail: 'toujours 401',
    });
    const rafraichir = vi.fn(async () => true);
    const budget = new Budget(50);
    const r = await appeler(tenter, { budget, dormir, alea, rafraichir });
    expect(rafraichir).toHaveBeenCalledTimes(1);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe('permission');
    expect(budget.consommes_()).toBe(2);
  });

  it('un refresh definitivement echoue devient reauth_required', async () => {
    // R1.3 : l'etat passe a `reauth_required`, visible au tray, sans crash ni
    // perte d'episode. La consequence est un hors-perimetre, pas une exception.
    const { dormir } = dormirFaux();
    const tenter = async (): Promise<Reponse<string>> => ({
      kind: 'unauthorized',
      detail: 'refresh mort',
    });
    const r = await appeler(tenter, {
      budget: new Budget(),
      dormir,
      alea,
      rafraichir: async () => false,
    });
    expect(r.ok).toBe(false);
    if (!r.ok && r.error.kind === 'permission') {
      expect(r.error.detail).toContain('reauth_required');
    } else {
      expect.fail(`attendu permission/reauth, recu ${JSON.stringify(r)}`);
    }
  });
});

describe('le budget d appels (R5.3)', () => {
  it('par defaut, trente appels', () => {
    expect(BUDGET_PAR_EPISODE).toBe(30);
    expect(new Budget().reste()).toBe(30);
  });

  it('compte AUSSI les tentatives qui echouent', async () => {
    // Une tentative coute un appel au quota distant, qu'elle reussisse ou non.
    // Ne compter que les succes ne protegerait de rien : c'est precisement quand
    // ca echoue qu'on martele.
    const { dormir } = dormirFaux();
    const budget = new Budget(10);
    await appeler(async () => ({ kind: 'retryable', detail: '429' }), {
      budget,
      dormir,
      alea,
    });
    expect(budget.consommes_()).toBe(TENTATIVES_MAX);
  });

  it('epuise, il arrete tout et le DIT', async () => {
    const { dormir } = dormirFaux();
    const budget = new Budget(2);
    const r = await appeler(async () => ({ kind: 'retryable', detail: '429' }), {
      budget,
      dormir,
      alea,
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe('budget_exhausted');
    expect(budget.reste()).toBe(0);
  });

  it('un budget epuise ne leve jamais — la cloture ne doit pas dependre du reseau', async () => {
    const budget = new Budget(0);
    const r = await appeler(async () => ({ kind: 'ok', value: 1 }), { budget });
    expect(r.ok).toBe(false);
  });
});

describe('la classification des erreurs (R5.2)', () => {
  it('range chaque erreur dans exactement une consequence', () => {
    expect(consequence({ kind: 'retryable_exhausted', attempts: 5, detail: 'x' }).kind).toBe(
      'trou',
    );
    expect(consequence({ kind: 'budget_exhausted', budget: 30 }).kind).toBe('trou');
    expect(consequence({ kind: 'permission', detail: 'x' }).kind).toBe('hors_perimetre');
    expect(consequence({ kind: 'not_found', detail: 'x' }).kind).toBe('non_resolue');
  });

  it('chaque consequence porte sa raison — jamais un classement muet', () => {
    // Un trou sans cause est indistinguable d'un trou qu'on n'a pas compris, et
    // la regle 4 du projet dit qu'un trou est un evenement de premiere classe.
    const cas = [
      consequence({ kind: 'retryable_exhausted', attempts: 5, detail: 'x' }),
      consequence({ kind: 'budget_exhausted', budget: 30 }),
      consequence({ kind: 'permission', detail: 'droits' }),
      consequence({ kind: 'not_found', detail: 'absent' }),
    ];
    for (const c of cas) {
      const texte = 'cause' in c ? c.cause : c.raison;
      expect(texte.length, JSON.stringify(c)).toBeGreaterThan(3);
    }
  });
});

describe('ce que le client ne fait jamais', () => {
  it('ne leve pas, meme quand tout va mal', async () => {
    // R5.2 : « un connecteur NE DOIT JAMAIS faire crasher le process ni bloquer
    // la cloture d'un episode ». Un Result et non une exception, parce qu'une
    // exception remonte jusqu'a quelqu'un qui l'attrape mal.
    const { dormir } = dormirFaux();
    const tenter = async (): Promise<Reponse<never>> => {
      throw new Error('le reseau a explose');
    };
    await expect(appeler(tenter, { budget: new Budget(), dormir, alea })).rejects.toThrow(
      'le reseau a explose',
    );
    // Note : une exception JETEE par l'appelant lui appartient. Le client ne
    // l'avale pas — l'avaler transformerait un defaut de code en trou de capture,
    // et on chercherait le probleme dans le mauvais systeme.
  });

  it('ne dort jamais plus que le plafond, meme si le serveur le demande', async () => {
    // Un `Retry-After` de dix minutes bloquerait la cloture de l'episode. On
    // respecte l'en-tete, dans la limite de ce qu'un episode peut attendre.
    const { attentes, dormir } = dormirFaux();
    let n = 0;
    await appeler(
      async (): Promise<Reponse<string>> => {
        n += 1;
        return n === 1
          ? { kind: 'retryable', retryAfterMs: 600_000, detail: '429' }
          : { kind: 'ok', value: 'ok' };
      },
      { budget: new Budget(), dormir, alea },
    );
    expect(attentes[0]).toBe(DELAI_MAX_MS);
  });
});
