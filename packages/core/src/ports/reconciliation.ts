/**
 * La réconciliation des deux plans (spec 003, R4).
 *
 * Deux récits du même travail : ce que l'opérateur a fait à l'écran, et ce que le
 * système a enregistré. Ils ne se recouvrent jamais parfaitement, et **c'est
 * l'écart qui est l'information**.
 *
 * La règle tient en une phrase : *chaque changement d'API finit dans exactement
 * une colonne*. Pas zéro — un changement perdu se lit comme un système qui n'a
 * rien fait. Pas deux — un changement compté deux fois gonfle le taux d'expliqué,
 * qui est LA métrique de santé du produit.
 *
 * ## La colonne qui compte
 *
 * `trou` porte une sous-cause, et c'est elle qu'il faut lire. Un changement sans
 * action UI **dans un trou déclaré** est attendu : on savait qu'on ne regardait
 * pas. Le même changement **hors de tout trou** est le vrai signal d'alarme : le
 * monde a bougé pendant qu'on croyait observer, et on n'a rien vu.
 */
import type { ApiChange } from './connector.js';

/** Une action de l'opérateur, réduite à ce dont la jointure a besoin. */
export type ActionUi = {
  readonly seq: number;
  readonly at: string;
  /** L'entité visée, telle que la résolution l'a établie. */
  readonly refId: string;
  /** Les champs que l'action touche, quand on les connaît. */
  readonly fields: readonly string[];
};

/** Une fenêtre de trou déjà déclarée par la capture (spec 002). */
export type Trou = { readonly from: string; readonly to: string };

/** R4.1 : la jointure se fait à trente secondes près, sur la même entité. */
export const FENETRE_JOINTURE_MS = 30_000;

/** Les trois colonnes de R4.2, et rien d'autre. */
export type Colonne =
  | { readonly kind: 'explique'; readonly seqUi: number }
  | { readonly kind: 'hors_perimetre'; readonly raison: string }
  | { readonly kind: 'trou'; readonly sousCause: 'dans_gap_declare' | 'hors_gap' };

export type LigneReconciliee = {
  readonly change: ApiChange;
  readonly colonne: Colonne;
};

export type Bilan = {
  readonly explique: number;
  readonly hors_perimetre: number;
  readonly trous: number;
  /** Le sous-total qui alarme : un changement non expliqué HORS de tout trou. */
  readonly trous_hors_gap: number;
  readonly lignes: readonly LigneReconciliee[];
};

const ms = (iso: string): number => Date.parse(iso);

function dansUnTrou(at: string, trous: readonly Trou[]): boolean {
  const t = ms(at);
  return trous.some((g) => t >= ms(g.from) && t <= ms(g.to));
}

/**
 * Réconcilie les deux plans.
 *
 * L'ordre des tests n'est pas indifférent, et il suit R4.2 :
 *
 * 1. **Un acteur qui n'est pas l'opérateur** sort d'abord. Un collègue, un
 *    automatisme, une intégration : le changement est réel, il n'est simplement
 *    pas de nous. Le compter comme un trou accuserait la capture d'avoir raté
 *    quelque chose qu'elle n'avait aucune raison de voir.
 * 2. **Un champ hors périmètre** ensuite. La tâche déclare ses `scope_fields` ;
 *    ce qui n'y est pas n'est pas de son ressort.
 * 3. **Une action UI proche** explique.
 * 4. **Sinon, un trou**, avec sa sous-cause.
 *
 * `actor: null` veut dire « le système ne l'expose pas », pas « l'opérateur ».
 * Supposer l'opérateur expliquerait des changements qu'il n'a pas faits, et
 * gonflerait la métrique de santé exactement là où elle doit alerter.
 */
export function reconcilier(
  changes: readonly ApiChange[],
  actions: readonly ActionUi[],
  scopeFields: readonly string[],
  trous: readonly Trou[],
  operateur: string | null = null,
): Bilan {
  const scope = new Set(scopeFields);
  const lignes: LigneReconciliee[] = [];

  for (const c of changes) {
    // 1. Un autre acteur, quand le système le dit.
    if (c.actor !== null && operateur !== null && c.actor !== operateur) {
      lignes.push({
        change: c,
        colonne: { kind: 'hors_perimetre', raison: `acteur ${c.actor}, pas l operateur` },
      });
      continue;
    }

    // 2. Les champs hors du périmètre déclaré de la tâche.
    const dansScope = c.fields.filter((f) => scope.has(f));
    if (dansScope.length === 0) {
      lignes.push({
        change: c,
        colonne: {
          kind: 'hors_perimetre',
          raison: `champs hors scope : ${c.fields.join(', ')}`,
        },
      });
      continue;
    }

    // 3. Une action UI sur la même entité, à moins de trente secondes.
    const t = ms(c.at);
    const proches = actions
      .filter((a) => a.refId === c.ref.id)
      .filter((a) => Math.abs(ms(a.at) - t) <= FENETRE_JOINTURE_MS);
    if (proches.length > 0) {
      // La PLUS PROCHE dans le temps. Prendre la première venue attribuerait le
      // changement à une action antérieure alors qu'une action postérieure le
      // colle mieux — et le rapport dirait la mauvaise cause.
      const meilleure = proches.reduce((a, b) =>
        Math.abs(ms(a.at) - t) <= Math.abs(ms(b.at) - t) ? a : b,
      );
      lignes.push({ change: c, colonne: { kind: 'explique', seqUi: meilleure.seq } });
      continue;
    }

    // 4. Un trou, et sa sous-cause.
    lignes.push({
      change: c,
      colonne: {
        kind: 'trou',
        sousCause: dansUnTrou(c.at, trous) ? 'dans_gap_declare' : 'hors_gap',
      },
    });
  }

  return {
    explique: lignes.filter((l) => l.colonne.kind === 'explique').length,
    hors_perimetre: lignes.filter((l) => l.colonne.kind === 'hors_perimetre').length,
    trous: lignes.filter((l) => l.colonne.kind === 'trou').length,
    trous_hors_gap: lignes.filter(
      (l) => l.colonne.kind === 'trou' && l.colonne.sousCause === 'hors_gap',
    ).length,
    lignes,
  };
}

/**
 * Le taux d'expliqué : LA métrique de santé (R4.3).
 *
 * Le hors-périmètre est **exclu du dénominateur**. Un changement qui n'est pas de
 * notre ressort ne peut ni être expliqué ni manquer ; l'y laisser ferait baisser
 * le taux quand un collègue travaille, et monter quand il part en vacances.
 *
 * Rend `null` quand il n'y a rien à mesurer. Zéro pour cent sur zéro changement
 * serait un chiffre faux, et il descendrait la moyenne du jour.
 */
export function tauxExplique(b: Bilan): number | null {
  const denominateur = b.explique + b.trous;
  if (denominateur === 0) return null;
  return b.explique / denominateur;
}
