import type { Episode, Event, FlatState, ToolCall } from '@noe/episode-spec';
import { diffObserve } from './judge.js';

/**
 * Une cible d'écriture possible, telle que la politique la voit.
 */
export type Cible = {
  readonly connector: string;
  readonly object: string;
  readonly object_id: string;
};

/**
 * Ce qu'une politique reçoit pour décider.
 *
 * **Aucun I/O n'est possible depuis ce type** : ni client, ni `fetch`, ni handle,
 * ni chemin de fichier. L'absence d'appel réseau pendant le rejeu (R3.1) est
 * garantie par la signature, pas par la discipline du développeur.
 *
 * `state_after` est volontairement absent : une politique ne doit jamais voir la
 * réponse. La politique parfaite ne la lit pas non plus — elle la reçoit à la
 * construction, depuis le corpus (voir `politiqueParfaite`).
 *
 * **Les événements `api_change` sont absents pour la même raison**, et il a fallu
 * un audit adverse pour s'en apercevoir. Ils portent `fields_changed`,
 * c'est-à-dire l'ensemble exact des champs que le juge compare : une politique
 * qui les lisait ne pouvait mécaniquement plus produire ni `manque` ni
 * `excedent`. `state_after` cachait les valeurs, ces événements donnaient la
 * liste — la moitié de la réponse, ce qui suffit à fausser deux des quatre
 * classes du verdict.
 */
export type ReplayContext = {
  readonly episode_id: string;
  readonly task: string;
  readonly scope_fields: readonly string[];
  readonly cibles: readonly Cible[];
  /** État avant, par cible (`connector/object/id`). */
  readonly before: Readonly<Record<string, FlatState>>;
  readonly events: readonly Event[];
};

export interface Policy {
  readonly id: string;
  propose(ctx: ReplayContext): Promise<ToolCall[]>;
}

/** Construit le contexte de rejeu d'un épisode. Ne fuit jamais l'état d'après. */
export function contexteDe(ep: Episode): ReplayContext {
  const cibles: Cible[] = [];
  const before: Record<string, FlatState> = {};

  for (const entite of ep.entities) {
    for (const ref of entite.api_refs) {
      cibles.push({ connector: ref.connector, object: ref.object, object_id: ref.id });
      before[`${ref.connector}/${ref.object}/${ref.id}`] = entite.state_before ?? {};
    }
  }

  cibles.sort((a, b) =>
    `${a.connector}/${a.object}/${a.object_id}`.localeCompare(
      `${b.connector}/${b.object}/${b.object_id}`,
    ),
  );

  return {
    episode_id: ep.id,
    task: ep.task_slug,
    scope_fields: [...ep.scope_fields].sort(),
    cibles,
    before,
    // Ce que l'opérateur a fait, et ce que le système a signalé de lui-même —
    // jamais ce que le monde a changé. Un `api_change` EST l'observation que le
    // juge compare : la donner à la politique, c'est lui donner la réponse.
    events: ep.events.filter((e) => e.kind !== 'api_change'),
  };
}

/**
 * Politique parfaite — **stub de test uniquement**.
 *
 * Elle propose exactement le diff observé. Elle ne le devine pas : elle le reçoit
 * à la construction, depuis le corpus. C'est le test d'auto-cohérence du socle
 * (R6.3) : si proposer ce qui s'est réellement passé ne donne pas 100 % d'accord,
 * le bug est dans le juge, pas dans la politique.
 */
export function politiqueParfaite(corpus: readonly Episode[]): Policy {
  const reponses = new Map<string, ToolCall[]>();

  for (const ep of corpus) {
    const perimetre = new Set(ep.scope_fields);
    const calls: ToolCall[] = [];

    for (const [cible, champs] of [...diffObserve(ep)].sort(([a], [b]) => a.localeCompare(b))) {
      const [connector, object, object_id] = cible.split('/');
      if (connector === undefined || object === undefined || object_id === undefined) continue;

      const fields: FlatState = {};
      for (const champ of [...champs.keys()].sort()) {
        // Hors périmètre : la tâche n'a pas à l'écrire, donc on ne le propose pas.
        if (!perimetre.has(champ)) continue;
        fields[champ] = champs.get(champ) ?? null;
      }
      if (Object.keys(fields).length === 0) continue;

      calls.push({ connector, object, object_id, action: 'update_fields', fields });
    }

    reponses.set(ep.id, calls);
  }

  return {
    id: 'parfaite',
    propose: (ctx) => Promise.resolve(reponses.get(ctx.episode_id) ?? []),
  };
}

/**
 * Politique nulle : ne propose rien.
 *
 * Sur un corpus non vide, elle doit produire des `manque` partout. Si le juge la
 * déclarait en accord, il serait cassé — c'est le test négatif du socle.
 */
export const politiqueNulle: Policy = {
  id: 'nulle',
  propose: () => Promise.resolve([]),
};
