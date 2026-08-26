# Spec 001 — design

Répond aux requirements R1-R6 de cette spec uniquement.

## 1. Vue d'ensemble

Deux packages, zéro réseau, zéro UI :

```
packages/episode-spec/          # MIT — le format ouvert
  src/schema.ts                 # schémas Zod canoniques (source unique des types)
  src/migrate.ts                # registre de migrateurs schema_v
  src/index.ts
packages/harness/               # AGPL — le socle de preuve
  src/replay.ts                 # boucle de rejeu fixtures
  src/judge.ts                  # comparaison normalisée + verdicts
  src/policy.ts                 # interface Policy + stubs (perfect, noop)
  src/report.ts                 # rapports (json + texte)
  src/cli.ts                    # noe replay | noe judge
  golden/                       # les 5 épisodes dorés + canaris.json
  test/
```

Dépendances autorisées : `zod`, `ulid`, `vitest`, `commander`. Rien d'autre.
`episode-spec` ne dépend de rien d'interne ; `harness` dépend d'`episode-spec`
seulement.

## 2. Schémas

Champs de l'épisode : `schema_v`, `id` (ULID), `task_slug`, `t0`, `t1`, `events[]`,
`entities[]`, `grade`, `grade_reason`, `scope_fields[]`, `completeness`,
`supersedes?`.

Événements — union discriminée sur `kind` : `ui_action` (avec `target`, `payload?`),
`api_change` (avec `connector`, `object`, `object_id`, `fields_changed[]`), `gap`
(avec `gap: {cause, from_seq, to_seq}`).

`FlatState` : `Record<string, string | number | boolean | null>`.

`ToolCall` : `{connector, object, object_id, action: "update_fields", fields}`.
Une seule action en v1, volontairement.

Invariants validés au parse (`superRefine`) : `seq` strictement croissant ;
`t0 ≤ ts ≤ t1` ; grade cohérent avec la présence de gaps et d'entités non résolues
(R2.1 recalculé et comparé au grade déclaré).

> **Note d'implémentation.** Zod 4 a déplacé plusieurs validateurs :
> `z.iso.datetime()` remplace `z.string().datetime()`, `z.ulid()` remplace
> `z.string().ulid()`, et `z.record()` exige explicitement le type de clé.

## 3. Migrations (R1.5)

`migrate.ts` : `MIGRATORS: Record<number, (old: unknown) => unknown>` ;
`load(raw)` lit `schema_v`, applique la chaîne N→N+1→…→`SCHEMA_V`, puis parse.
Absence de migrateur = `MigrationError` explicite.

Test permanent : une fixture `golden/legacy/episode_v0.json` migre et parse vert —
le test existe **avant** la première vraie migration, pour que le mécanisme soit
prouvé à vide.

## 4. Rejeu (R3) — algorithme

```
pour chaque fichier épisode (ordre lexicographique — déterminisme):
  ep = load(raw)                        # migrations incluses
  ctx = { task, before: états_avant(ep.entities), events }
  calls = policy.propose(ctx)           # AUCUN I/O ici
  observed = diff(state_before, state_after) par entité, restreint à scope_fields
  verdict = judge(calls, observed, scope_fields)
  accumule(rapport)
sortie: rapport.json + rapport texte ; exit 0/1/2
```

Déterminisme (R3.3) : itérations sur clés triées partout ; sérialisation via
stringify stable ; aucun `Date.now()` ni aléa dans le chemin de calcul — le `ts` du
rapport est isolé dans l'en-tête, exclu de la comparaison octet à octet.

Interface politique (R3.4) :

```ts
export interface Policy { readonly id: string; propose(ctx: ReplayContext): Promise<ToolCall[]> }
export const perfectPolicy: Policy   // rejoue le diff observé (auto-cohérence R6.3)
export const noopPolicy: Policy      // ne propose rien (doit produire des « manqué »)
```

Le type `ReplayContext` ne porte ni client, ni `fetch`, ni handle : l'absence d'I/O
est garantie par le type, pas par la discipline.

## 5. Juge (R4) — normalisation et verdict

`normalize(v)` : `null | undefined | "" → null` ; chaînes : trim + `\r\n → \n` ;
date parsable → ISO-8601 UTC ; numérique en chaîne → number.

Classement par champ du périmètre (R4.2) : proposé ∧ observé ∧ égaux → `accord` ;
proposé ∧ observé ∧ ≠ → `désaccord` ; observé seul → `manqué` ; proposé seul →
`excédent`. Champs hors `scope_fields` : comptés `hors_périmètre`, jamais dans le
verdict.

Verdict épisode (R4.3) : `accord` ssi désaccord = manqué = excédent = 0.

Rapport (R4.4) : par épisode — verdict, table champ→classe→(proposé, observé) ;
agrégé — n, taux d'accord, top des champs en échec, répartition par classe. Deux
formats : `--json` (machine, stable) et texte (humain, colonnes).

## 6. Canaris (R5)

`golden/canaris.json` : liste des chaînes (`CANARY_PII_001`,
`canary.pii@example.invalid`, IBAN factice…). Test CI `canary-sweep.test.ts` :
exécute un rejeu complet vers un dossier temporaire, balaye récursivement TOUTES
les sorties, échoue à la première occurrence. Aucune variable d'échappement lue
(R5.3) — le test est inconditionnel.

## 7. Corpus doré (R6) — contenu exact

Tâche fictive de référence `maj-crm-post-echange` (2 entités : un `contact`, un
`deal` ; `scope_fields` : `statut`, `prochaine_action`, `date_relance`, `notes`).

- (a) `001_nominal.json` — A, diff = {statut, prochaine_action, date_relance} → accord.
- (b) `002_branche_alt.json` — A, même tâche, séquence d'actions différente.
- (c) `003_trou.json` — gap `sleep` → grade B, `grade_reason` explicite, exclu des agrégats.
- (d) `004_hors_perimetre.json` — le diff contient `derniere_connexion` (hors scope).
- (e) `005_canaris.json` — A, contient les canaris en `payload` et en `notes`.

## 8. Tests (stratégie)

Unitaires : schémas (valides/invalides/invariants), normalisation (dates, nombres,
null/vide, CRLF), classement des champs, grades. Intégration : rejeu complet avec
`perfectPolicy` (100 % accord sur A) et `noopPolicy` (0 %, tout en `manqué`).
Propriété : déterminisme ×3 octet à octet. CI : lint + typecheck + tests + rejeu +
canary-sweep + validation du corpus au schéma.

## 9. Erreurs

Fichier épisode invalide → erreur listée dans le rapport, épisode marqué
`unreadable`, exit 2 **seulement si aucun** épisode n'est lisible ; sinon le rejeu
continue — un corpus ne meurt pas d'un fichier. `MigrationError` → même traitement,
message avec la version trouvée et attendue.
