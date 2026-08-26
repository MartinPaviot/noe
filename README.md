# Noe

> Du grec *noûs* — l'esprit qui comprend.

Noe observe le travail réel sur votre poste, le rejoue, et ne propose d'agir que
lorsqu'un juge mécanique l'a prouvé.

**Aucun contenu utilisateur ne quitte jamais votre poste.** Ce n'est pas une
option de configuration : c'est la contrainte dont découle toute l'architecture.
Voir [`SECURITY.md`](SECURITY.md) et [`docs/invariants.md`](docs/invariants.md).

## État

**Spec 001 — verte.** Format d'épisode, rejeu à froid, juge mécanique : les 15
tâches sont cochées, 129 tests passent, le corpus doré rejoue à 100 % d'accord
sur les grades A. Voir
[`specs/001-socle-de-preuve/tasks.md`](specs/001-socle-de-preuve/tasks.md).

**Spec 002 — capture bornée**, en cours. Les `tasks.md` des specs sont l'unique
liste de vérité du projet.

## Structure

| Chemin | Rôle | Licence |
| --- | --- | --- |
| `apps/desktop` | Coquille Tauri v2 (Windows) | AGPL-3.0 |
| `packages/core` | Domaine pur — aucun I/O, aucun réseau | AGPL-3.0 |
| `packages/episode-spec` | Schémas du format d'épisode | **MIT** |
| `packages/harness` | CLI de rejeu et de jugement | AGPL-3.0 |
| `packages/connectors` | Adaptateurs vers les systèmes de vérité | AGPL-3.0 |
| `docs` | Prompt maître, invariants, frontière d'édition, checklist | — |

Le format d'épisode est sous licence MIT à dessein : il doit pouvoir être lu,
validé et réimplémenté par n'importe qui, y compris dans un produit
propriétaire. C'est un format, pas un moteur.

## Démarrer

```sh
corepack enable
pnpm install
pnpm verify        # lint + typecheck + tests, ce que la CI exécute
```

Puis copiez `.env.example` en `.env.local` et suivez
[`docs/setup-checklist.md`](docs/setup-checklist.md).

## Les cinq règles

1. Aucun contenu utilisateur ne quitte jamais le poste.
2. Seul le juge mécanique promeut.
3. Une feature par session, vérifiée de bout en bout.
4. Les épisodes sont immuables ; tout trou de capture est un événement de première classe.
5. Jamais un secret dans un fichier suivi.

## Licence

[AGPL-3.0-only](LICENSE), à l'exception de `packages/episode-spec`
([MIT](packages/episode-spec/LICENSE)).
