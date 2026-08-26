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
| `002-capture-bornee` | en cours — tâche 0 en attente du verdict du spike |
