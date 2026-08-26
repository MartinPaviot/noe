# Noe

> Du grec *noûs* : l'esprit qui comprend. Noe observe le travail reel, le rejoue,
> et ne propose d'agir que lorsqu'un juge mecanique l'a prouve.

**Avant toute session, lis `docs/prompt-maitre-v0.md`.** C'est la source de verite.
Les regles longues vivent dans `docs/invariants.md` ; ci-dessous le strict necessaire.

## Commandes

| But | Commande |
| --- | --- |
| Tout verifier (ce que la CI fait) | `pnpm verify` |
| Lint + format check | `pnpm lint` |
| Corriger le formatage | `pnpm format` |
| Typecheck strict, tous packages | `pnpm typecheck` |
| Tests | `pnpm test` |
| Scan de secrets | `pnpm secrets` |
| CLI du harness | `pnpm --filter @noe/harness exec tsx src/cli.ts` |
| Coquille desktop (dev) | `pnpm --filter @noe/desktop tauri dev` |
| Evidence quotidienne (D26) | `pnpm evidence` |

## Mission

[`docs/mission.md`](docs/mission.md) est le document de reference, relu au debut de CHAQUE session : arbitrages, methode, feuille de route des specs, standard de tests visuels, protocole de long-run.

## Doctrine d'exécution

Avant de classer une tâche « humaine », descendre l'échelle en entier :
**API → CLI → MCP → Playwright (profil Chrome déjà connecté) → humain guidé**.

Trois irréductibles seulement : les **gestes de travail quand ils sont la donnée
mesurée**, les **secrets** (jamais demandés — on opère sur session ouverte), les
**décisions** (signatures, gates, verdicts).

Détail et garde-fous : [`docs/doctrine-execution.md`](docs/doctrine-execution.md).

## Les cinq regles

1. **Aucun contenu utilisateur ne quitte jamais le poste.** Pas de telemetrie de
   contenu, pas d'envoi de corpus, pas de « juste pour debugger ».
2. **Seul le juge mecanique promeut.** Aucune feature ne passe `"passes": true`
   sur une impression, une demo ou un avis — uniquement sur un verdict reproductible.
3. **Une feature par session, verifiee de bout en bout, commit a chaque vert.**
4. **Les episodes sont immuables ; tout trou de capture est un evenement de
   premiere classe.** On ne rebouche jamais un trou en silence : on l'enregistre.
5. **Jamais un secret dans un fichier suivi.** `.env.local` uniquement ;
   `.env.example` documente les noms de cles, jamais les valeurs.

## Etat

Les `specs/<nnn>-<nom>/tasks.md` sont **l'unique liste de vérité** : elles portent
l'ordre des tâches et leur statut. `progress.md` porte le journal des sessions.
`docs/decisions.md` porte les arbitrages.

On ne coche une case que sur une vérification de bout en bout, et on n'édite
**jamais** une tâche pour la faire passer.

| Spec | État |
| --- | --- |
| `001-socle-de-preuve` | ✅ 15/15 — format d'épisode, rejeu, juge mécanique |
| `002-capture-bornee` | en cours — 7/20, tâches 0 à 5 vertes |
| `003-federation-boucle` | scellée, ouvre au gate de la 002 |
| `004-politique-shadow` | déposée (opérateur), ouvre au gate de la 003 |
| `005-modes-promotion` | déposée (opérateur), ouvre au gate de la 004 |
| `006-assiste` | déposée (opérateur), ouvre au gate de la 004 |
| `007` à `010` | à écrire par l'agent, au gate de la précédente |
