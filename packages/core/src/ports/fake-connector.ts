/**
 * Le connecteur de banc, et ses quatre scénarios adverses (spec 003, §1).
 *
 * Même rôle que `FakeSource` et `FakeClock` en spec 002 : rendre R1 à R7
 * testables en CI, sans réseau, sans quota, sans org de démo. Ce qui compte
 * n'est pas qu'il imite Salesforce — c'est qu'il sache **mal se comporter à la
 * demande**, exactement là où le domaine doit tenir.
 *
 * Les quatre scénarios du design ne sont pas décoratifs. Chacun correspond à une
 * exigence qui, sans lui, ne serait vérifiée que le jour où un vrai système la
 * violera :
 *
 * 1. **Résolution ambiguë** — R2.2. Deux contacts portent le même courriel. Le
 *    système doit refuser de trancher, et dire combien il a trouvé.
 * 2. **429 en rafale** — R5.1. Le client commun doit temporiser, pas marteler.
 * 3. **Écriture avant lecture** — R3.3. Quelqu'un a modifié l'enregistrement
 *    avant notre première lecture : le `state_before` qu'on croit avoir est déjà
 *    faux, et seule l'histoire peut le rattraper.
 * 4. **Champ non historisé** — R3.3 encore, mais l'autre issue. Quand l'histoire
 *    ne sait pas, on écrit `unknown_before` avec sa raison. Salesforce ne suit
 *    que vingt champs par objet : ce cas sera fréquent, et ce n'est pas un bug.
 */
import type {
  ApiChange,
  ApiRef,
  EntityCandidate,
  FlatState,
  HistoryPoint,
  Outcome,
  ReadConnector,
  Resolution,
  TimeWindow,
} from './connector.js';
import { err, ok } from './connector.js';

export type ScenarioFake = {
  /** R2.2 : combien de candidats la résolution trouve. 1 = résolu. */
  readonly candidats?: number;
  /** R5.1 : combien de 429 consécutifs avant de répondre. */
  readonly rafale429?: number;
  /** R3.3 : l'enregistrement a été modifié AVANT notre première lecture. */
  readonly ecritureAvantLecture?: boolean;
  /** R3.3 : les champs dont le système ne garde aucune histoire. */
  readonly champsNonHistorises?: readonly string[];
  /** L'état que `read` rend. */
  readonly etat?: FlatState;
  /** Les changements que `changes` rend. */
  readonly changements?: readonly ApiChange[];
};

/**
 * Un connecteur qui obéit à un scénario.
 *
 * Il compte ses appels : R5.3 impose un budget par épisode, et un banc qui ne
 * saurait pas dire combien de fois il a été sollicité ne pourrait pas le
 * vérifier.
 */
export class FakeConnector implements ReadConnector {
  readonly id = 'fake';

  private appels = 0;
  private restant429: number;

  constructor(private readonly scenario: ScenarioFake = {}) {
    this.restant429 = scenario.rafale429 ?? 0;
  }

  /** Combien de fois le connecteur a été appelé, tous verbes confondus. */
  compteur(): number {
    return this.appels;
  }

  /**
   * Le 429 de la rafale, s'il en reste.
   *
   * Rendu comme `retryable_exhausted` avec le compte de tentatives : c'est au
   * client commun de retenter, pas au connecteur. Un connecteur qui retenterait
   * tout seul cacherait la rafale au client, et le budget d'appels de R5.3 ne
   * verrait rien passer.
   */
  private peutRepondre(): Outcome<null> | null {
    this.appels += 1;
    if (this.restant429 > 0) {
      this.restant429 -= 1;
      return err({
        kind: 'retryable_exhausted',
        attempts: 1,
        detail: '429 du banc',
      });
    }
    return null;
  }

  async resolve(candidate: EntityCandidate): Promise<Outcome<Resolution>> {
    const refus = this.peutRepondre();
    if (refus !== null) return refus as Outcome<Resolution>;

    const n = this.scenario.candidats ?? 1;
    if (n === 0) return ok({ status: 'not_found' });
    if (n >= 2) return ok({ status: 'ambiguous', count: n });

    const cle = candidate.keys[0];
    if (cle === undefined) {
      // Aucune clé forte : ce n'est pas « pas trouvé », c'est « rien à
      // chercher ». Les confondre ferait croire à une absence côté système.
      return ok({ status: 'not_found' });
    }
    return ok({
      status: 'resolved',
      ref: { connector: this.id, object: candidate.type, id: 'FAKE-0001' },
      by: cle.kind,
      at: '2026-01-14T09:12:00.000Z',
    });
  }

  async read(_ref: ApiRef, fields: readonly string[]): Promise<Outcome<FlatState>> {
    const refus = this.peutRepondre();
    if (refus !== null) return refus as Outcome<FlatState>;

    const complet = this.scenario.etat ?? { statut: 'qualifie', montant: 4200 };
    // On ne rend QUE les champs demandés : R3.1 restreint la lecture au périmètre
    // de la tâche, et un connecteur qui rendrait tout ferait entrer dans
    // l'épisode des champs que personne n'a demandés.
    const restreint: Record<string, string | number | boolean | null> = {};
    for (const f of fields) {
      if (f in complet) restreint[f] = complet[f] ?? null;
    }
    return ok(restreint);
  }

  async changes(ref: ApiRef, since: string): Promise<Outcome<readonly ApiChange[]>> {
    const refus = this.peutRepondre();
    if (refus !== null) return refus as Outcome<readonly ApiChange[]>;
    const tous = this.scenario.changements ?? [];
    return ok(tous.filter((c) => c.at >= since && c.ref.id === ref.id));
  }

  async history(
    ref: ApiRef,
    field: string,
    window: TimeWindow,
  ): Promise<Outcome<readonly HistoryPoint[]>> {
    const refus = this.peutRepondre();
    if (refus !== null) return refus as Outcome<readonly HistoryPoint[]>;

    // Scénario 4 : le champ n'est pas suivi. Le système rend une liste VIDE, pas
    // une erreur — c'est exactement ce que fait Salesforce, et c'est ce qui rend
    // le cas piégeux : « aucun changement » et « je ne sais pas » se ressemblent.
    // C'est à l'appelant de les distinguer, et il ne peut le faire que s'il sait
    // quels champs sont suivis.
    if ((this.scenario.champsNonHistorises ?? []).includes(field)) {
      return ok([]);
    }

    // Scénario 3 : une écriture antérieure à notre première lecture.
    if (this.scenario.ecritureAvantLecture === true) {
      return ok([
        {
          at: window.from,
          field,
          from: 'nouveau',
          to: 'qualifie',
        },
      ]);
    }
    return ok([]);
  }
}

/**
 * Les champs qu'un système déclare suivre.
 *
 * Sans cette liste, « l'historique est vide » est indistinguable de « le champ
 * n'a pas changé », et les deux mènent à des conclusions opposées : la seconde
 * autorise un `state_before` reconstitué, la première impose `unknown_before`.
 */
export type ChampsSuivis = ReadonlySet<string>;

/**
 * R3.3 — que faire d'un champ dont on soupçonne une écriture antérieure.
 *
 * Trois issues, et il faut les trois. Une seule manquante et on retomberait dans
 * l'erreur que l'exigence nomme : un champ « silencieusement compté ».
 */
export type VerdictAvant =
  /** L'histoire a parlé : on connaît la valeur d'avant. */
  | { readonly kind: 'reconstitue'; readonly valeur: string | number | boolean | null }
  /** Pas d'écriture antérieure : la lecture directe fait foi. */
  | { readonly kind: 'lecture_directe' }
  /** L'histoire ne sait pas. Le champ sort du verdict, avec sa raison. */
  | { readonly kind: 'inconnu'; readonly raison: string };

export function verdictAvant(
  champ: string,
  suivis: ChampsSuivis,
  histoire: readonly HistoryPoint[],
  premiereLecture: string,
): VerdictAvant {
  if (!suivis.has(champ)) {
    return {
      kind: 'inconnu',
      raison: `champ non historise par le systeme : impossible de dire ce qu il valait avant ${premiereLecture}`,
    };
  }
  const anterieures = histoire.filter((h) => h.field === champ && h.at < premiereLecture);
  if (anterieures.length === 0) return { kind: 'lecture_directe' };

  // La PLUS ANCIENNE antérieure porte la valeur d'origine : c'est son `from` qui
  // dit ce que valait le champ avant que quiconque n'y touche. Prendre la plus
  // récente rendrait la valeur intermédiaire, et le diff avec `state_after`
  // raconterait une moitié de l'histoire.
  const plusAncienne = [...anterieures].sort((a, b) => a.at.localeCompare(b.at))[0];
  return { kind: 'reconstitue', valeur: plusAncienne?.from ?? null };
}
