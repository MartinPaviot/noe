# Spec 001 — Le socle de preuve

**Statut :** approuvé · **Périmètre :** la première brique du projet, et rien d'autre.

**Pourquoi cette spec en premier** : le harness est la CI du produit entier ; rien
ne se construit sans lui, et lui ne dépend de rien. Cette spec ne contient ni
capture, ni connecteur, ni UI, ni modèle appelé en réseau : elle établit le format
des épisodes, leur rejeu hors ligne, et le juge qui rend des verdicts.

---

## Requirement 1 — Le format d'épisode

**User story** : en tant que système, je représente chaque occurrence de tâche
comme un épisode autosuffisant, pour qu'il puisse être rejoué, jugé et audité sans
aucun contexte externe.

1.1. QUAND un épisode est sérialisé, LE SYSTÈME DOIT produire un objet validé par
le schéma Zod canonique de `packages/episode-spec`, contenant : `schema_v`, `id`
(ULID), `task_slug`, `t0`/`t1`, `events[]`, `entities[]`, `grade`, `completeness`.

1.2. CHAQUE événement DOIT porter : `schema_v`, `seq` (entier strictement
croissant dans l'épisode), `ts`, `source` (`"ui" | "api" | "system"`), `kind`, et
selon le kind : `target` (`{role, name, region}`), `payload` (déjà pseudonymisé),
ou `gap` (`{cause, from_seq, to_seq}`).

1.3. CHAQUE entité DOIT porter : une clé forte (`{type, value_pseudo}`),
`first_seen_seq`, ses références API (`{connector, object, id}`), et ses états
figés `state_before` / `state_after` (objets plats champ→valeur, pseudonymisés).

1.4. SI un épisode est clôturé, ALORS LE SYSTÈME NE DOIT JAMAIS le modifier —
toute correction produit un nouvel épisode référençant l'ancien (`supersedes`).

1.5. QUAND un épisode de `schema_v` N−1 est chargé, LE SYSTÈME DOIT le migrer vers
N via un migrateur enregistré et testé ; SI aucun migrateur n'existe, ALORS le
chargement DOIT échouer avec une erreur explicite (jamais de lecture partielle
silencieuse).

## Requirement 2 — Les grades

**User story** : en tant que politique de promotion, je ne consomme que des
épisodes dont la qualité est prouvée, pour que mes statistiques ne soient jamais
contaminées.

2.1. LE SYSTÈME DOIT attribuer le grade à la clôture selon des règles mécaniques :
**A** = séquence sans trou ET toutes les entités résolues (`state_before` et
`state_after` présents) ET redaction validée ; **B** = au plus un trou OU une
entité non résolue ; **C** = le reste.

2.2. LE JUGE NE DOIT compter dans les statistiques de branche que les épisodes de
grade A ; les B/C restent lisibles (contexte, débogage) mais marqués exclus.

2.3. QUAND le grade est attribué, LE SYSTÈME DOIT journaliser la raison exacte
(quelle règle a déclassé), pour que « pourquoi ce n'est pas un A » soit toujours
répondable.

## Requirement 3 — Le rejeu à froid (mode fixtures)

**User story** : en tant que développeur, je rejoue le corpus entier en local en
moins d'une minute, pour que chaque changement de politique soit évalué avant
d'exister.

3.1. QUAND `noe replay <chemin>` s'exécute en mode fixtures, LE SYSTÈME DOIT :
reconstruire le contexte de chaque épisode (`state_before` + événements jusqu'aux
points de décision), demander à la politique ses propositions d'écritures
(`ToolCall[]`), et les transmettre au juge — SANS exécuter aucun appel réseau,
d'aucune sorte.

3.2. LE REJEU DOIT s'achever en < 60 s sur un corpus de 50 épisodes, sur une
machine de développement standard.

3.3. LE REJEU DOIT être déterministe : trois exécutions consécutives sur les mêmes
entrées DOIVENT produire trois sorties strictement identiques (octet pour octet,
horodatages de rapport exclus).

3.4. En v1 de cette spec, la « politique » DOIT être un stub injectable (interface
`Policy`), afin que le harness soit testable avant toute intégration LLM ; le port
DOIT accepter plus tard une politique LLM sans modification du harness.

3.5. QUAND le rejeu se termine, LE SYSTÈME DOIT sortir avec le code 0 si tous les
verdicts attendus sont conformes, 1 sinon, 2 sur erreur d'exécution — pour être
utilisable tel quel en CI.

## Requirement 4 — Le juge mécanique

**User story** : en tant que système de promotion, je fonde chaque verdict sur une
comparaison d'états déterministe, pour que « ça marche » soit une mesure et jamais
une opinion.

4.1. QUAND le juge compare les écritures proposées au diff observé (`state_before`
→ `state_after`), IL DOIT évaluer champ par champ après normalisation : trim des
espaces, unification des fins de ligne, dates comparées en ISO-8601 UTC, nombres
comparés en valeur (pas en chaîne), `null` ≡ champ absent ≡ chaîne vide.

4.2. LE JUGE DOIT classer chaque champ modifié en : `accord` (proposé ≡ observé),
`désaccord` (proposé ≠ observé), `manqué` (observé mais non proposé), `excédent`
(proposé mais non observé), et chaque champ non couvert par le périmètre de la
tâche en `hors_périmètre`.

4.3. LE VERDICT d'un épisode DOIT être `accord` si et seulement si zéro
`désaccord`, zéro `manqué`, zéro `excédent` sur les champs du périmètre.

4.4. QUAND `noe judge --summary` s'exécute, LE SYSTÈME DOIT produire un rapport
lisible : par épisode (verdict + diff détaillé) et agrégé (taux d'accord,
répartition des échecs par type et par champ).

4.5. LE JUGE NE DOIT JAMAIS appeler un modèle : cette spec ne contient aucun juge
sémantique, par décision.

## Requirement 5 — Les canaris de confidentialité

**User story** : en tant qu'utilisateur observé, j'ai la preuve mécanique — pas la
promesse — qu'aucune donnée sensible ne fuit du socle.

5.1. LE CORPUS doré DOIT contenir des chaînes canari documentées (emails,
téléphone, IBAN factices, marqueurs uniques type `CANARY_PII_001`).

5.2. QUAND la CI s'exécute, UN TEST DOIT balayer toutes les sorties du socle
(rapports, logs, artefacts sérialisés) et ÉCHOUER si un canari y apparaît en clair.

5.3. Ce test DOIT être impossible à désactiver sans diff visible en revue (pas de
variable d'environnement d'échappement).

## Requirement 6 — Le corpus doré initial

**User story** : en tant que harness, je dispose dès le premier jour d'un corpus de
référence qui couvre les cas qui comptent, pour que chaque brique suivante soit
construite contre lui.

6.1. LE CORPUS DOIT contenir 5 épisodes écrits à la main, couvrant chacun un cas
distinct : (a) nominal grade A à verdict accord ; (b) même tâche, branche
différente ; (c) épisode avec trou → grade B, exclu des stats ; (d) épisode dont le
diff observé contient un champ hors périmètre ; (e) épisode contenant les canaris.

6.2. LES ÉPISODES dorés DOIVENT être versionnés dans le repo et validés par le
schéma en CI.

6.3. QUAND le stub de politique « parfaite » tourne sur le corpus, LE VERDICT DOIT
être 100 % accord sur les grades A — c'est le test d'auto-cohérence du socle entier.

## Hors périmètre de cette spec (explicite)

Capture réelle (spec 002) · connecteurs et lectures API (spec 003) · politique LLM,
shadow, divergences (spec 004) · branches et promotion (spec 005) · UI (spec 006+) ·
tout le commercial.
