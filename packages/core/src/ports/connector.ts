/**
 * Le port de fédération : **lecture seule, structurellement** (spec 003, §1).
 *
 * Il n'y a pas de `write` dans ce type, et ce n'est pas un oubli. La spec 003 est
 * en lecture seule, et une interface qui exposerait l'écriture « pour plus tard »
 * laisserait un adaptateur l'implémenter, puis un appelant l'appeler. Ici le
 * compilateur l'interdit : la promotion et l'exécution appartiennent à une spec
 * ultérieure, qui étendra le port explicitement et par décision.
 *
 * Le domaine ne connaît que ce port. Le choix du CRM vit dans `terrain.json`
 * (R1.1) — jamais dans le code hors de son adaptateur, sinon changer de terrain
 * demanderait de retoucher le domaine, et le domaine n'a pas d'opinion sur
 * Salesforce.
 */

/** Une entité candidate, telle que la capture de la spec 002 la propose. */
export type EntityCandidate = {
  /** Le type d'entité visé : `contact`, `lead`, `opportunity`… */
  readonly type: string;
  /**
   * Les clés fortes disponibles, déjà **pseudonymisées** quand elles portent une
   * identité (R6.2). La valeur claire ne vit qu'en mémoire, jamais ici.
   */
  readonly keys: readonly StrongKey[];
};

/**
 * Une clé de résolution. **Fortes uniquement** — R2.1.
 *
 * Pas de « nom approchant », pas de distance d'édition, pas de score. Une
 * résolution floue qui se trompe attribue le travail d'un opérateur au dossier de
 * quelqu'un d'autre, et rien en aval ne peut le rattraper : le graphe d'entités
 * est faux et il a l'air juste.
 */
export type StrongKey =
  /** Identifiant du système distant, exact. */
  | { readonly kind: 'system_id'; readonly value: string }
  /** Adresse de courriel, comparée en **jeton HMAC des deux côtés** (R6.2). */
  | { readonly kind: 'email_token'; readonly value: string }
  /** Domaine + nom exact, tous deux normalisés. */
  | { readonly kind: 'domain_name'; readonly domain: string; readonly name: string };

/** La référence d'un enregistrement chez le système distant. */
export type ApiRef = {
  readonly connector: string;
  readonly object: string;
  readonly id: string;
};

/**
 * Le verdict de résolution. **Jamais de devinette** (R2.2).
 *
 * L'échec porte sa raison : `not_found` et `ambiguous:2` ne se corrigent pas de la
 * même façon, et un épisode qui dirait seulement « non résolu » ne laisserait
 * aucune prise pour comprendre pourquoi.
 */
export type Resolution =
  | {
      readonly status: 'resolved';
      readonly ref: ApiRef;
      /** R2.3 : la clé qui a tranché, et quand. */
      readonly by: StrongKey['kind'];
      readonly at: string;
    }
  | { readonly status: 'not_found' }
  | { readonly status: 'ambiguous'; readonly count: number };

/** Un état plat : champ → valeur scalaire. Le juge de la spec 001 lit ceci. */
export type FlatState = Readonly<Record<string, string | number | boolean | null>>;

/**
 * Ce que le juge doit RETIRER de son verdict, champ par champ (§7).
 *
 * Parallèle à `FlatState` plutôt qu'imbriqué dedans : le juge lit l'état, les
 * exclusions lisent la méta. Annoter les valeurs elles-mêmes aurait rendu
 * `FlatState` non plat, et le diff cesserait d'être trivial.
 */
export type StateMeta = Readonly<
  Record<
    string,
    {
      /** R3.3 : la valeur vient de l'historique, pas d'une lecture directe. */
      readonly reconstituted?: boolean;
      /** R3.3 : on n'a pas su dire ce que valait ce champ avant. */
      readonly unknown_before?: boolean;
      /** Pourquoi. Obligatoire quand `unknown_before` : jamais d'exclusion muette. */
      readonly reason?: string;
    }
  >
>;

/** Un changement observé côté API (R4). */
export type ApiChange = {
  readonly ref: ApiRef;
  readonly at: string;
  readonly fields: readonly string[];
  /**
   * L'acteur, **quand le système l'expose**. `null` veut dire « inconnu » et non
   * « l'opérateur » : R4.2 range un changement d'un autre acteur hors périmètre,
   * et supposer l'opérateur par défaut expliquerait des changements qu'il n'a pas
   * faits.
   */
  readonly actor: string | null;
};

/** Une fenêtre de temps, bornes ISO incluses. */
export type TimeWindow = { readonly from: string; readonly to: string };

/** Un point d'historique : ce que valait un champ, et quand il a changé. */
export type HistoryPoint = {
  readonly at: string;
  readonly field: string;
  readonly from: string | number | boolean | null;
  readonly to: string | number | boolean | null;
};

/** Les erreurs qu'un connecteur rend, classées (R5.2). */
export type ConnectorError =
  /** Les tentatives sont épuisées : ça devient un trou, avec sa cause. */
  | { readonly kind: 'retryable_exhausted'; readonly attempts: number; readonly detail: string }
  /** Droits insuffisants : hors périmètre, avec raison. */
  | { readonly kind: 'permission'; readonly detail: string }
  /** L'enregistrement n'existe pas : la résolution a échoué. */
  | { readonly kind: 'not_found'; readonly detail: string }
  /** Le budget d'appels de l'épisode est épuisé (R5.3). */
  | { readonly kind: 'budget_exhausted'; readonly budget: number };

/**
 * Le résultat d'une opération de connecteur.
 *
 * Un `Result` et non une exception : R5.2 dit qu'un connecteur **ne doit jamais
 * faire crasher le process ni bloquer la clôture d'un épisode**. Une exception
 * remonte jusqu'à quelqu'un qui l'attrape mal ; un type de retour force
 * l'appelant à décider quoi en faire, et le compilateur vérifie qu'il l'a fait.
 */
export type Outcome<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: ConnectorError };

export const ok = <T>(value: T): Outcome<T> => ({ ok: true, value });
export const err = <T>(error: ConnectorError): Outcome<T> => ({ ok: false, error });

/**
 * Le port. Quatre verbes, aucun n'écrit.
 */
export interface ReadConnector {
  readonly id: string;

  /** R2 — clés fortes seulement, jamais de devinette. */
  resolve(candidate: EntityCandidate): Promise<Outcome<Resolution>>;

  /** R3 — l'état normalisé. Non persisté tel quel : la redaction passe après. */
  read(ref: ApiRef, fields: readonly string[]): Promise<Outcome<FlatState>>;

  /** R4 — le delta depuis un instant donné. */
  changes(ref: ApiRef, since: string): Promise<Outcome<readonly ApiChange[]>>;

  /** R3.3 — la corroboration par l'historique, quand le système en a un. */
  history(
    ref: ApiRef,
    field: string,
    window: TimeWindow,
  ): Promise<Outcome<readonly HistoryPoint[]>>;
}
