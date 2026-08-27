import { describe, expect, it } from 'vitest';
import { contexteDe } from './policy.js';
import { chargerCorpus } from './replay.js';

/**
 * Le contexte de rejeu ne doit pas contenir la réponse.
 *
 * `state_after` en était absent depuis le début, et un commentaire le disait.
 * Mais les événements `api_change` portent `fields_changed` — l'ensemble **exact**
 * des champs que le juge compare — et ils descendaient jusqu'à la politique.
 *
 * Ce n'est pas une fuite de valeurs : les valeurs restaient cachées, donc un
 * `desaccord` restait possible. C'est une fuite de **liste**, et elle suffit à
 * supprimer mécaniquement deux des quatre classes du verdict : `juger` fonde
 * `manque` sur « observé et non proposé » et `excedent` sur « proposé et non
 * observé ». Une politique qui connaît la liste ne peut plus se tromper ni dans
 * un sens ni dans l'autre.
 *
 * Aujourd'hui aucune politique ne lit `ctx.events`, donc rien ne fuyait en
 * pratique. La fuite devient vivante au moment exact où elle compte : quand une
 * politique LLM se branchera — ce que R3.4 promet possible sans modifier le
 * harness — et qu'on lui donnera tout le contexte parce qu'il est là.
 */

const GOLDEN = 'packages/harness/golden';

describe('le contexte de rejeu ne porte pas la reponse', () => {
  it('aucun evenement api_change ne descend jusqu a la politique', async () => {
    const { episodes } = await chargerCorpus(GOLDEN);
    expect(episodes.length).toBeGreaterThan(0);

    let vus = 0;
    for (const ep of episodes) {
      vus += ep.events.filter((e) => e.kind === 'api_change').length;
      for (const e of contexteDe(ep).events) {
        expect(e.kind, `${ep.id} livre un api_change a la politique`).not.toBe('api_change');
      }
    }
    // Sans ce compte, le cas passerait au vert sur un corpus qui n'en contient
    // aucun — et ne prouverait rien du tout.
    expect(vus, 'le corpus ne contient aucun api_change : le controle serait vide').toBeGreaterThan(
      0,
    );
  });

  it('aucun ENSEMBLE de champs changes n apparait dans le contexte', async () => {
    // Ce qui fuyait n'était pas un nom de champ, c'était une **liste**.
    //
    // Un nom seul a le droit d'être là : `state_before` porte les champs lus, et
    // la politique doit les voir pour proposer quoi que ce soit. Savoir qu'un
    // champ existe n'est pas savoir qu'il a changé — l'épisode 004 le montre
    // bien, avec cinq champs lus et deux changés.
    //
    // Le contrôle porte donc sur le contexte SERIALISE et cherche l'ensemble
    // exact, pas ses membres. Il attraperait la même fuite revenue par un autre
    // chemin — une commodité ajoutée un jour à `cibles`, par exemple.
    const { episodes } = await chargerCorpus(GOLDEN);
    let eprouves = 0;
    for (const ep of episodes) {
      const ctx = contexteDe(ep);
      const serialise = JSON.stringify(ctx);
      const perimetre = JSON.stringify(ctx.scope_fields);

      for (const e of ep.events) {
        if (e.kind !== 'api_change' || e.fields_changed.length === 0) continue;
        // Dans l'ordre du corpus et trié : deux façons d'écrire le même ensemble.
        for (const ensemble of [e.fields_changed, [...e.fields_changed].sort()]) {
          const rendu = JSON.stringify(ensemble);
          // **Quand TOUS les champs du périmètre ont changé**, l'ensemble égale
          // `scope_fields` — et celui-là est public, c'est la tâche qui le
          // déclare. Le signaler serait accuser une information légitime, ce
          // qu'un premier jet de ce banc a fait sur l'épisode 002.
          if (rendu === perimetre) continue;
          eprouves += 1;
          expect(
            serialise,
            `${ep.id} : l ensemble des champs changes de ${e.object_id} fuit`,
          ).not.toContain(rendu);
        }
      }
    }
    expect(eprouves, 'aucun ensemble a eprouver : le controle serait vide').toBeGreaterThan(0);
  });

  it('ce qui reste dans le contexte suffit encore a decider', async () => {
    // Le correctif ne doit pas vider le contexte de sa substance : la politique
    // a toujours besoin de savoir ce que l'operateur a fait.
    const { episodes } = await chargerCorpus(GOLDEN);
    for (const ep of episodes) {
      const ctx = contexteDe(ep);
      expect(ctx.events.length, `${ep.id} : contexte vide`).toBeGreaterThan(0);
      expect(ctx.events.some((e) => e.kind === 'ui_action')).toBe(true);
      expect(ctx.cibles.length).toBeGreaterThan(0);
    }
  });
});
