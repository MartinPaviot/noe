# Spec 001 — tasks

Une tâche = un incrément vérifié. Cocher = **le critère passe**, pas « le code
existe ». Ordre imposé.

- [x] **1. Squelette des deux packages** — `episode-spec` (MIT) et `harness` (AGPL)
      dans le monorepo, dépendances autorisées seulement (zod, ulid, commander,
      vitest), typecheck strict vert, CI qui exécute lint + typecheck + tests sur
      les deux. _Req : socle de R1-R6._
- [x] **2. Schémas Zod complets** — `schema.ts` conforme au design §2, invariants
      en `superRefine` (`seq` croissant, bornes temporelles, cohérence du grade),
      tests valides/invalides pour chaque invariant. _Req : 1.1, 1.2, 1.3, 2.1._
- [x] **3. Immutabilité et supersedes** — API de clôture qui gèle l'épisode
      (`Object.freeze` + type `readonly`), toute mutation post-clôture testée comme
      erreur, mécanisme `supersedes` testé. _Req : 1.4._
- [x] **4. Migrations** — registre de migrateurs, `load()` avec chaîne de
      migration, `MigrationError` explicite, fixture `legacy/episode_v0.json` qui
      migre vert, test « version inconnue échoue proprement ». _Req : 1.5._
- [x] **5. Attribution des grades** — règles mécaniques A/B/C avec `grade_reason`,
      recalcul au parse comparé au grade déclaré, table de cas testée. _Req : 2.1, 2.3._
- [x] **6. Normalisation du juge** — `normalize()` avec la table du design §5,
      tests exhaustifs (dates 3 formats, nombres en chaîne, null/vide/absent, CRLF,
      espaces). _Req : 4.1._
- [x] **7. Classement et verdict** — diff observé restreint au scope, classement
      accord/désaccord/manqué/excédent/hors_périmètre, verdict épisode, exclusion
      des B/C des agrégats. _Req : 4.2, 4.3, 2.2._
- [x] **8. Interface Policy + stubs** — `Policy`, `perfectPolicy`, `noopPolicy`,
      aucun I/O possible dans le chemin de rejeu (le type du contexte ne porte ni
      client ni fetch). _Req : 3.4._
- [x] **9. Boucle de rejeu + CLI** — `noe replay <dir> [--json]` conforme à
      l'algorithme §4, itération déterministe, exit codes 0/1/2, gestion des
      fichiers illisibles (§9). _Req : 3.1, 3.5._
- [x] **10. Rapports** — `noe judge --summary`, formats texte et json stable,
      par-épisode + agrégé + top des champs en échec. _Req : 4.4._
- [x] **11. Corpus doré** — les 5 épisodes du design §7 écrits à la main, validés
      au schéma en CI, versionnés. _Req : 6.1, 6.2._
- [x] **12. Auto-cohérence** — `perfectPolicy` = 100 % accord sur les A ;
      `noopPolicy` = 0 % accord, tout en `manqué` ; épisode (c) exclu des agrégats ;
      épisode (d) verdict accord malgré le champ hors périmètre. _Req : 6.3, 2.2, 4.2._
- [x] **13. Déterminisme prouvé** — test qui exécute le rejeu 3 fois et compare les
      sorties octet à octet (en-tête `ts` exclu). _Req : 3.3._
- [x] **14. Performance** — corpus synthétique de 50 épisodes générés (script),
      rejeu < 60 s mesuré en CI. _Req : 3.2._
- [x] **15. Canary sweep** — `canaris.json`, test CI inconditionnel qui balaye
      toutes les sorties d'un rejeu réel, rouge à la première occurrence, aucun
      échappement possible. _Req : 5.1, 5.2, 5.3._

> **Les 15 taches sont vertes au 2026-08-26.** 99 tests, rejeu du corpus dore
> a 100 % d accord sur les grades A, rejeu de 50 episodes en 1,5 s.

**Gate de sortie de la spec 001** : les 15 cases vertes + CI verte sur un clone
frais + `pnpm i && noe replay golden/` sur une machine produit le rapport attendu
sans aucune autre commande. Alors — et seulement alors — on écrit
`specs/002-capture/requirements.md`.
