/**
 * Le client commun : ce qui rend une API distante en colère inoffensive
 * (spec 003, R5).
 *
 * **Une API en colère ne doit ni faire crasher le process, ni le faire mentir.**
 * Les deux moitiés comptent, et la seconde est la plus traître : un connecteur
 * qui avale une erreur et rend un état vide produit un `state_before` faux qui a
 * l'air juste, et le juge conclut à un désaccord qui n'existe pas.
 *
 * Tout passe par ici. Un adaptateur qui appellerait `fetch` directement
 * échapperait au budget de R5.3, et une tempête de requêtes ne se voit qu'après
 * coup — quand le quota est déjà brûlé.
 */
import type { ConnectorError, Outcome } from './connector.js';
import { err } from './connector.js';

/** R5.1 : cinq tentatives, pas six. Le plafond est dans l'exigence. */
export const TENTATIVES_MAX = 5;
/** R5.3 : le budget par défaut, surchargeable par `terrain.json`. */
export const BUDGET_PAR_EPISODE = 30;

/** Le délai de base du backoff exponentiel, en millisecondes. */
export const DELAI_BASE_MS = 250;
/** Au-delà, on n'attend plus davantage : un backoff illimité bloque la clôture. */
export const DELAI_MAX_MS = 8_000;

/** Ce qu'une tentative a donné, du point de vue du client. */
export type Reponse<T> =
  | { readonly kind: 'ok'; readonly value: T }
  /** 429 ou 5xx : ça vaut la peine de réessayer. */
  | { readonly kind: 'retryable'; readonly retryAfterMs?: number; readonly detail: string }
  /** 401 : le jeton a expiré, on rafraîchit puis on rejoue **une** fois. */
  | { readonly kind: 'unauthorized'; readonly detail: string }
  /** 403 : les droits manquent. Réessayer ne changera rien. */
  | { readonly kind: 'permission'; readonly detail: string }
  /** 404 : l'enregistrement n'existe pas. */
  | { readonly kind: 'not_found'; readonly detail: string };

/**
 * Le délai avant la tentative `n`, avec **jitter**.
 *
 * Le jitter n'est pas une coquetterie. Sans lui, tous les clients qui prennent
 * un 429 au même instant reviennent au même instant : la deuxième vague est
 * aussi serrée que la première, et l'API reste en colère. Le hasard les étale.
 *
 * `alea` est injecté pour que le banc soit déterministe — un test qui vérifierait
 * un délai tiré au hasard vérifierait le hasard.
 */
export function delaiMs(tentative: number, alea: () => number = Math.random): number {
  const exponentiel = Math.min(DELAI_BASE_MS * 2 ** Math.max(0, tentative - 1), DELAI_MAX_MS);
  // Jitter « full » : entre la moitié et la totalité. Garder un plancher évite
  // qu'un tirage malheureux ne rappelle immédiatement.
  return Math.floor(exponentiel * (0.5 + 0.5 * alea()));
}

/**
 * Le compteur d'appels d'un épisode (R5.3).
 *
 * Il est passé au client, pas tenu par lui : un budget par client serait un
 * budget par connecteur, et deux connecteurs se partageraient le double. Le
 * budget appartient à l'**épisode**.
 */
export class Budget {
  private consommes = 0;

  constructor(private readonly plafond: number = BUDGET_PAR_EPISODE) {}

  reste(): number {
    return Math.max(0, this.plafond - this.consommes);
  }

  consommes_(): number {
    return this.consommes;
  }

  plafond_(): number {
    return this.plafond;
  }

  /** Rend `false` quand le budget est épuisé. Ne lève jamais. */
  prendre(): boolean {
    if (this.consommes >= this.plafond) return false;
    this.consommes += 1;
    return true;
  }
}

export type OptionsClient = {
  readonly budget: Budget;
  /** Rafraîchit le jeton. Rendu `false` = échec définitif → `reauth_required`. */
  readonly rafraichir?: () => Promise<boolean>;
  /** Injecté pour les bancs : sinon un test de backoff dure huit secondes. */
  readonly dormir?: (ms: number) => Promise<void>;
  readonly alea?: () => number;
  readonly tentativesMax?: number;
};

const dormirVrai = (ms: number): Promise<void> =>
  new Promise((f) => {
    setTimeout(f, ms);
  });

/**
 * Exécute une requête avec toute la politique de R5.
 *
 * L'appelant fournit une fonction qui tente **une fois** et classe sa réponse.
 * Le client s'occupe du reste : budget, backoff, `Retry-After`, refresh sur 401,
 * et la classification finale de R5.2.
 */
export async function appeler<T>(
  tenter: () => Promise<Reponse<T>>,
  options: OptionsClient,
): Promise<Outcome<T>> {
  const dormir = options.dormir ?? dormirVrai;
  const alea = options.alea ?? Math.random;
  const max = options.tentativesMax ?? TENTATIVES_MAX;

  let rafraichissementTente = false;

  for (let tentative = 1; tentative <= max; tentative += 1) {
    // Le budget se prend AVANT l'appel, y compris pour une reprise : une
    // tentative coûte un appel au quota distant, qu'elle réussisse ou non.
    // Le compter après ne protégerait de rien.
    if (!options.budget.prendre()) {
      return err({ kind: 'budget_exhausted', budget: options.budget.plafond_() });
    }

    const r = await tenter();
    switch (r.kind) {
      case 'ok':
        return { ok: true, value: r.value };

      case 'permission':
        // Réessayer ne changera pas les droits. R5.2 : hors périmètre, avec raison.
        return err({ kind: 'permission', detail: r.detail });

      case 'not_found':
        return err({ kind: 'not_found', detail: r.detail });

      case 'unauthorized': {
        // **Un seul rafraîchissement.** Deux 401 de suite après un refresh réussi
        // veulent dire autre chose qu'un jeton expiré, et boucler dessus brûlerait
        // le budget sur un problème que le refresh ne résout pas.
        if (rafraichissementTente || options.rafraichir === undefined) {
          return err({
            kind: 'permission',
            detail: `401 persistant apres rafraichissement : ${r.detail}`,
          });
        }
        rafraichissementTente = true;
        const ok = await options.rafraichir();
        if (!ok) {
          return err({ kind: 'permission', detail: `reauth_required : ${r.detail}` });
        }
        // On rejoue tout de suite, sans attendre : le jeton est neuf.
        continue;
      }

      case 'retryable': {
        if (tentative >= max) {
          return err({
            kind: 'retryable_exhausted',
            attempts: tentative,
            detail: r.detail,
          });
        }
        // `Retry-After` gagne sur notre calcul quand le serveur en donne un : il
        // sait quand il sera prêt, nous devinons. L'ignorer est la façon la plus
        // courante de transformer une limitation en bannissement.
        const attente = r.retryAfterMs ?? delaiMs(tentative, alea);
        await dormir(Math.min(attente, DELAI_MAX_MS));
        continue;
      }
    }
  }

  return err({
    kind: 'retryable_exhausted',
    attempts: max,
    detail: 'tentatives epuisees',
  });
}

/**
 * Ce qu'une erreur devient dans l'épisode (R5.2).
 *
 * La classification est ici, en un seul endroit, parce qu'elle décide de ce que
 * l'épisode raconte. Éparpillée dans les adaptateurs, elle finirait par diverger
 * — et deux erreurs identiques donneraient deux verdicts différents selon le
 * connecteur qui les a rencontrées.
 */
export type Consequence =
  /** Un trou déclaré, avec sa cause. */
  | { readonly kind: 'trou'; readonly cause: string }
  /** Hors périmètre, avec sa raison. */
  | { readonly kind: 'hors_perimetre'; readonly raison: string }
  /** La résolution a échoué : l'entité reste non résolue. */
  | { readonly kind: 'non_resolue'; readonly raison: string };

export function consequence(e: ConnectorError): Consequence {
  switch (e.kind) {
    case 'retryable_exhausted':
      return { kind: 'trou', cause: `api_indisponible apres ${e.attempts} tentatives` };
    case 'budget_exhausted':
      // R5.3 : dépassement → arrêt des lectures + trou déclaré. Jamais une
      // tempête de requêtes, jamais un silence non plus.
      return { kind: 'trou', cause: `budget d appels epuise (${e.budget})` };
    case 'permission':
      return { kind: 'hors_perimetre', raison: `droits insuffisants : ${e.detail}` };
    case 'not_found':
      return { kind: 'non_resolue', raison: e.detail };
  }
}
