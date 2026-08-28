# Noe

> Du grec *noûs* : l'esprit qui comprend. Noe observe le travail reel, le rejoue,
> et ne propose d'agir que lorsqu'un juge mecanique l'a prouve.

**Avant toute session, lis `docs/prompt-maitre-v0.md`.** C'est la source de verite.
Les regles longues vivent dans `docs/invariants.md` ; ci-dessous le strict necessaire.

## Commandes

| But | Commande |
| --- | --- |
| Tout verifier avant de committer | `pnpm verify` |
| Lint + format check | `pnpm lint` |
| Corriger le formatage | `pnpm format` |
| Typecheck strict, tous packages | `pnpm typecheck` |
| Tests | `pnpm test` |
| Scan de secrets | `pnpm secrets` |
| CLI du harness | `pnpm --filter @noe/harness build` puis `node packages/harness/dist/cli.js` |
| Coquille desktop (dev) | `pnpm --filter @noe/desktop tauri dev` |
| Evidence quotidienne (D26) | `pnpm evidence` |
| Tests visuels | `pnpm --filter @noe/desktop exec playwright test` |

> **`pnpm verify` n'est pas « ce que la CI fait ».** La CI en fait plus : elle
> rejoue le corpus doré, vérifie que la politique nulle échoue bien, et scanne
> l'historique complet avec gitleaks. `verify` est ce qu'on lance avant de
> committer ; il ne dispense pas de regarder la CI.


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
| `002-capture-bornee` | ✅ 20/20 — gate franchi le 2026-08-27 |
| `003-federation-boucle` | en cours — 9/14 ; tout ce qui ne demande pas l'org est écrit et vert. **Bloquée sur la tâche 0** : incident du coffre, voir `docs/decisions.md` |
| `004-politique-shadow` | déposée (opérateur), ouvre au gate de la 003 |
| `005-modes-promotion` | déposée (opérateur), ouvre au gate de la 004 |
| `006-assiste` | déposée (opérateur), ouvre au gate de la 004 |
| `007` à `010` | à écrire par l'agent, au gate de la précédente |
