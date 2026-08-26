import { z } from 'zod';
import { resumerOccurrences, validerRedaction } from './redaction.js';

/** Version du format d'épisode. Tout épisode capturé porte ce numéro. */
export const SCHEMA_V = 1;

// ---------------------------------------------------------------------------
// Briques élémentaires
// ---------------------------------------------------------------------------

/** Cible d'une action d'interface. `region` est optionnel : tout capteur ne la connaît pas. */
export const Target = z.object({
  role: z.string().min(1),
  name: z.string().min(1),
  region: z.string().optional(),
});

/**
 * Un trou de capture. Événement de première classe : on ne rebouche jamais en
 * silence, on enregistre ce qu'on a manqué et pourquoi.
 */
/**
 * Les causes de trou, comme **donnée** et non comme littéral inline.
 *
 * Le capteur est en Rust et porte le même enum ; s'ils divergent, le capteur
 * écrit une cause que le harness refuse de parser, et l'épisode part en
 * quarantaine sans que personne comprenne pourquoi. La liste est donc exportée
 * pour être projetée dans un miroir vérifié, exactement comme les motifs PII.
 */
export const CAUSES_GAP = [
  'crash',
  'kill',
  'sleep',
  'seq_break',
  'manual',
  'pause',
  'timeout',
] as const;

export const Gap = z.object({
  // « pause » et « timeout » ajoutes par la spec 002 (voir docs/decisions.md) :
  // une pause operateur et une cloture automatique a 60 min sont des trous de
  // capture au meme titre qu un crash — declares, jamais silencieux.
  cause: z.enum(CAUSES_GAP),
  from_seq: z.number().int().nonnegative(),
  to_seq: z.number().int().nonnegative(),
});

const baseEvent = {
  schema_v: z.literal(SCHEMA_V),
  seq: z.number().int().nonnegative(),
  ts: z.iso.datetime(),
};

export const Event = z.discriminatedUnion('kind', [
  z.object({
    ...baseEvent,
    kind: z.literal('ui_action'),
    source: z.literal('ui'),
    action: z.enum(['invoke', 'input', 'toggle', 'navigate', 'copy', 'paste', 'submit']),
    target: Target,
    /** Déjà pseudonymisé à la capture. Le socle ne pseudonymise pas, il vérifie. */
    payload: z.string().optional(),
  }),
  z.object({
    ...baseEvent,
    kind: z.literal('api_change'),
    source: z.literal('api'),
    connector: z.string().min(1),
    object: z.string().min(1),
    object_id: z.string().min(1),
    fields_changed: z.array(z.string()),
  }),
  z.object({
    ...baseEvent,
    kind: z.literal('gap'),
    source: z.literal('system'),
    gap: Gap,
  }),
]);

/** État plat : champ → valeur scalaire. Pas d'imbrication, pour que le diff soit trivial. */
export const FlatState = z.record(
  z.string(),
  z.union([z.string(), z.number(), z.boolean(), z.null()]),
);

export const Entity = z.object({
  key: z.object({ type: z.string().min(1), value_pseudo: z.string().min(1) }),
  first_seen_seq: z.number().int().nonnegative(),
  api_refs: z.array(
    z.object({ connector: z.string().min(1), object: z.string().min(1), id: z.string().min(1) }),
  ),
  state_before: FlatState.optional(),
  state_after: FlatState.optional(),
});

export const Completeness = z.object({
  explained: z.number().int().nonnegative(),
  out_of_scope: z.number().int().nonnegative(),
  gaps: z.number().int().nonnegative(),
});

export const Grade = z.enum(['A', 'B', 'C']);

// ---------------------------------------------------------------------------
// Grades — R2.1, règles mécaniques
// ---------------------------------------------------------------------------

export type GradeVerdict = { grade: 'A' | 'B' | 'C'; reason: string };

type EntreeGrade = {
  events: readonly { kind: string }[];
  entities: readonly {
    key: { type?: string; value_pseudo: string };
    api_refs?: readonly unknown[];
    state_before?: unknown;
    state_after?: unknown;
  }[];
};

/**
 * INVARIANT 7 du prompt maître : le grade A exige « bornes confirmées API ».
 *
 * **Garde en vigueur jusqu'à la spec 003.** La condition n'est pas vérifiable
 * tant qu'aucun connecteur ne lit d'état : l'appliquer aujourd'hui produirait des
 * C partout, l'omettre laisserait un invariant non tenu. Elle est donc déclarée
 * dans le code, et neutralisée par ce drapeau — qui dit exactement où on en est.
 *
 * Passer à `true` avec la fédération (spec 003) déclenche le regrade du corpus.
 * Voir `docs/decisions.md`, D5.
 */
export const CONFIRMATION_API_VERIFIABLE = false;

/**
 * Attribue le grade selon R2.1, et surtout **dit pourquoi**.
 * « Pourquoi ce n'est pas un A » doit toujours être répondable (R2.3).
 */
export function gradeOf(ep: EntreeGrade): GradeVerdict {
  const gaps = ep.events.filter((e) => e.kind === 'gap').length;
  const nonResolues = ep.entities.filter(
    (e) => e.state_before === undefined || e.state_after === undefined,
  ).length;

  // Deux conditions distinctes, souvent confondues.
  //
  // La premiere est structurelle : une cle d entite porte `value_pseudo`, jamais
  // la valeur reelle. Une cle vide signale que la pseudonymisation n a pas tourne.
  const clesPseudonymisees = ep.entities.every((e) => e.key.value_pseudo.trim().length > 0);
  if (!clesPseudonymisees) {
    return { grade: 'C', reason: 'redaction non validee : une cle d entite est vide' };
  }

  // La seconde est celle que la spec 002 (R4.6) definit mecaniquement : zero
  // motif PII dans l episode entierement serialise.
  const redaction = validerRedaction(ep);
  if (!redaction.valide) {
    return {
      grade: 'C',
      reason: `redaction non validee : ${resumerOccurrences(redaction.occurrences)} dans l episode serialise`,
    };
  }
  // INVARIANT 7 — bornes confirmees API. Neutralise jusqu a la spec 003 (D5).
  const bornesConfirmees =
    !CONFIRMATION_API_VERIFIABLE ||
    ep.entities.every((e) => Array.isArray(e.api_refs) && e.api_refs.length > 0);

  if (gaps === 0 && nonResolues === 0 && bornesConfirmees) {
    return { grade: 'A', reason: 'sequence sans trou, toutes entites resolues, redaction validee' };
  }
  if (gaps === 0 && nonResolues === 0) {
    return { grade: 'B', reason: 'declasse en B : bornes non confirmees par API' };
  }
  const defauts = gaps + nonResolues;
  if (defauts <= 1) {
    const quoi = gaps === 1 ? '1 trou de capture' : '1 entite non resolue';
    return { grade: 'B', reason: `declasse en B : ${quoi}` };
  }
  return {
    grade: 'C',
    reason: `declasse en C : ${gaps} trou(s) et ${nonResolues} entite(s) non resolue(s)`,
  };
}

// ---------------------------------------------------------------------------
// Épisode
// ---------------------------------------------------------------------------

const EpisodeBase = z.object({
  schema_v: z.literal(SCHEMA_V),
  id: z.ulid(),
  task_slug: z.string().min(1),
  t0: z.iso.datetime(),
  t1: z.iso.datetime(),
  events: z.array(Event).min(1),
  entities: z.array(Entity),
  grade: Grade,
  grade_reason: z.string().min(1),
  /** Les champs que la tâche est censée toucher. Hors de cette liste : hors périmètre. */
  scope_fields: z.array(z.string()).min(1),
  completeness: Completeness,
  supersedes: z.ulid().optional(),
});

export const Episode = EpisodeBase.superRefine((ep, ctx) => {
  // seq strictement croissant (R1.2)
  for (let i = 1; i < ep.events.length; i++) {
    const prec = ep.events[i - 1];
    const cour = ep.events[i];
    if (prec !== undefined && cour !== undefined && cour.seq <= prec.seq) {
      ctx.addIssue({
        code: 'custom',
        path: ['events', i, 'seq'],
        message: `seq doit etre strictement croissant : ${cour.seq} apres ${prec.seq}`,
      });
    }
  }

  // bornes temporelles : t0 <= ts <= t1
  const t0 = Date.parse(ep.t0);
  const t1 = Date.parse(ep.t1);
  if (t1 < t0) {
    ctx.addIssue({ code: 'custom', path: ['t1'], message: 't1 est anterieur a t0' });
  }
  for (const [i, ev] of ep.events.entries()) {
    const ts = Date.parse(ev.ts);
    if (ts < t0 || ts > t1) {
      ctx.addIssue({
        code: 'custom',
        path: ['events', i, 'ts'],
        message: `ts hors des bornes [t0, t1] : ${ev.ts}`,
      });
    }
  }

  // cohérence du grade déclaré avec le grade recalculé (R2.1)
  const calcule = gradeOf(ep);
  if (calcule.grade !== ep.grade) {
    ctx.addIssue({
      code: 'custom',
      path: ['grade'],
      message: `grade declare « ${ep.grade} » mais les regles donnent « ${calcule.grade} » (${calcule.reason})`,
    });
  }

  // cohérence du nombre de trous annoncé
  const gaps = ep.events.filter((e) => e.kind === 'gap').length;
  if (ep.completeness.gaps !== gaps) {
    ctx.addIssue({
      code: 'custom',
      path: ['completeness', 'gaps'],
      message: `completeness.gaps = ${ep.completeness.gaps} mais ${gaps} evenement(s) gap presents`,
    });
  }
});

/** Une écriture proposée par une politique. Une seule action en v1, volontairement. */
export const ToolCall = z.object({
  connector: z.string().min(1),
  object: z.string().min(1),
  object_id: z.string().min(1),
  action: z.literal('update_fields'),
  fields: FlatState,
});

// ---------------------------------------------------------------------------
// Types inférés — le schéma est la source unique
// ---------------------------------------------------------------------------

export type Target = z.infer<typeof Target>;
export type Gap = z.infer<typeof Gap>;
export type Event = z.infer<typeof Event>;
export type FlatState = z.infer<typeof FlatState>;
export type Entity = z.infer<typeof Entity>;
export type Completeness = z.infer<typeof Completeness>;
export type Grade = z.infer<typeof Grade>;
export type Episode = z.infer<typeof Episode>;
export type ToolCall = z.infer<typeof ToolCall>;
