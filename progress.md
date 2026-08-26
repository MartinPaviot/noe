# Journal des sessions — Noe

## Session 0 — Initialisation du dépôt et inventaire de configuration

**Date :** 2026-08-25 → 2026-08-26 · **Poste :** Windows 11 Home 10.0.26200

### Livré

**Monorepo** — pnpm workspaces, TypeScript strict partout
(`noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `verbatimModuleSyntax`).
`apps/desktop` (coquille Tauri v2), `packages/core` (domaine pur),
`packages/episode-spec` (**licence MIT propre**), `packages/harness` (CLI `noe`),
`packages/connectors`.

**CI** — deux jobs : `lint · typecheck · tests` (Biome + tsc + Vitest + lint
anti-contenu) et `scan de secrets` (gitleaks 8.30.1 sur l'historique complet).
Verte à chaque commit. Protection de branche posée : checks requis, force-push
et suppression bloqués, historique linéaire, `enforce_admins: false`.

**Docs** — `invariants.md`, `edition-boundary.md` (les 3 régimes, la règle du
geste humain), `setup-checklist.md`, `spike-verdict.md` (template),
`CLAUDE.md` (5 règles), `SECURITY.md`, `.env.example`.
`features.json` : 12 features, **toutes à `false`**.

**Azure AI Foundry** — opérationnel, vérifié en inférence réelle :

| Déploiement | Latence mesurée |
| --- | --- |
| `gpt-5.4` | 1590 ms (18 tokens entrée, 9 sortie) |
| `gpt-5.4-mini` | 621 ms (18 entrée, 8 sortie) |
| `text-embedding-3-large` | déployé, pour le corpus doré |

**Supabase** — projet `noe-prod` (`tbkwagmviekohzdnstbg`), org LeadSens (pro),
eu-west-3, taille micro. Gate coût confirmé avant création. Trois tables
(`licences`, `compteurs`, `telemetrie_optin`), **RLS activée et forcée, zéro
politique** — vérifié en interrogeant `pg_class` sur la base distante.

**Lint anti-contenu** — `scripts/lint-anti-contenu.mjs` : l'invariant I devient
une contrainte exécutable. Refuse les types fourre-tout et les colonnes
textuelles au nom évocateur, avec une échappatoire explicite tracée dans le diff.
7 tests.

**Vercel** — projet `noe` lié, landing squelette déployée en production :
<https://noe-martins-projects-02d07974.vercel.app>

### Vérification

`pnpm verify` vert : lint · lint:sql · typecheck 5/5 packages · **12/12 tests**.

### Réparé pendant la session

| Problème | Cause réelle | Résolution |
| --- | --- | --- |
| `curl`, `winget/msstore`, `az` échouent tous en TLS | **Avast Web/Mail Shield intercepte tout le HTTPS** et re-signe avec son root CA, dont `basicConstraints` n'est pas critique — OpenSSL 3 le refuse quel que soit le bundle | analyse HTTPS désactivée par l'utilisateur ; bundle de contournement retiré ensuite pour qu'il ne rouille pas |
| `az login` : 0 tenant, 0 souscription, 6 fois | `az` interrogeait l'annuaire par défaut du compte MSA, qui est réellement vide | annuaire réel découvert via l'en-tête `WWW-Authenticate` d'une requête ARM **sans jeton** |
| Clé Azure refusée partout (23 combinaisons) | la ressource n'était pas `elevay-foundry` mais `martinpaviot-4001-resource` | corrigé ; l'en-tête doit être `Bearer`, pas `api-key` |
| 28 noms de déploiement en `DeploymentNotFound` | le projet n'avait **aucun** déploiement | `GET {projet}/deployments` → `200 {"value":[]}`, réponse sans ambiguïté |
| `biome migrate` avait posé `"preset": "none"` | mauvaise traduction de `recommended: true` en 2.2 → 2.5 | remis en `recommended`, vérifié en injectant un `noExplicitAny` |
| le lint anti-contenu ratait les `alter table … add column` | l'extracteur prenait `alter` pour un nom de colonne | corrigé — trou révélé par un test écrit avant de le constater |
| landing déployée mais invisible | Deployment Protection active par défaut | `ssoProtection` désactivée via l'API Vercel |

### Non livré, et pourquoi

- **`docs/prompt-maitre-v0.md` est vide.** Le fichier annoncé comme « fourni à
  côté » est introuvable — recherche par nom et par contenu sur tout
  `C:\Users\marti`. `features.json` et `invariants.md` sont donc **dérivés du
  brief** et marqués PROVISOIRE. Les **5 critères de choix du terrain** manquent :
  **F01 ne peut pas démarrer sans eux.**
- **Stripe** — CLI apparié, mais sur `Usenareo`, un compte `business_type:
  individual`. Le brief demande l'entité société. Ni la création ni le renommage
  d'un compte propriétaire ne sont automatisables. Clé live retirée du poste.
- **Accès ARM Azure** — bloqué par une stratégie d'accès conditionnel du tenant
  (`AADSTS530035`, appareil non enregistré). Le CLI **et** Azure PowerShell sont
  bloqués à l'identique : la règle couvre toute la surface Azure Management.
  Aucune tentative de contournement. Sans impact sur F01, qui appelle des modèles
  sans en provisionner. Bloque les budgets et alertes.
- **Coquille Tauri non compilée** — aucune installation Visual Studio sur le
  poste, le workload C++ manque. Étape strictement humaine, bloque F10.
- **Sentry, PostHog, Resend** — en attente de jetons d'API.

### État des features

12 features, **0 à `"passes": true`**. Conforme : seul le juge mécanique promeut.

### Prochaine session

**Session 1 — F01, le spike.** Bloquée tant que le prompt maître n'est pas
déposé. Les modèles répondent, l'infrastructure est prête ; il manque les
critères de décision.

---

## Session 1 — Spec 001, le socle de preuve

**Date :** 2026-08-26

### Livré

Les **15 tâches** de `specs/001-socle-de-preuve/tasks.md` sont vertes.

- **`@noe/episode-spec` (MIT)** — schémas Zod du format d'épisode, invariants au
  parse, règles de grade A/B/C avec motif, clôture gelante en profondeur,
  `supersedes`, registre de migrateurs avec fixture v0.
- **`@noe/harness` (AGPL)** — normalisation, classement en cinq classes, verdict,
  interface `Policy` sans I/O possible par construction, boucle de rejeu
  déterministe, rapports texte et JSON, CLI `noe replay | judge`.
- **Corpus doré** — 5 épisodes écrits à la main, `canaris.json`, fixture legacy.

### Verdict mécanique

```
politique parfaite   4/4 grades A en accord   100 %   exit 0
politique nulle      0/4                        0 %   exit 1, 11 « manqué »
50 épisodes synthétiques                              1,5 s  (seuil 60 s)
99 tests
```

Gate de sortie vérifié sur un **clone frais** du dépôt public :
`pnpm i && noe replay packages/harness/golden` produit le rapport attendu sans
aucune autre commande.

### Deux décisions de conception qui méritaient d'être prises

**`ReplayContext` ne contient pas `state_after`.** La politique parfaite doit
rejouer le diff observé, mais lui donner la réponse par le contexte ouvrirait le
même chemin à une vraie politique. Elle reçoit donc le corpus **à la
construction**. L'absence d'I/O est garantie par le type, pas par la discipline.

**Les rapports n'émettent jamais de valeur en clair.** Les chaînes sortent en
empreinte `sha256`. L'égalité reste visible — ce qu'un diff exige — sans que le
contenu quitte le processus. Sans cela, le canary sweep aurait échoué à juste
titre sur l'épisode (e), et il aurait eu raison.

### Ce que les tests ont attrapé

Le lint anti-contenu ratait les `alter table … add column`. Le corpus synthétique
produisait des ULID de 25 caractères, donc 50 épisodes tous illisibles. Le
binaire `noe` n'existait pas sur un clone frais — pnpm ne lie pas le bin d'un
package du workspace tant qu'il n'est la dépendance de personne.

Trois trous réels, trouvés par des tests écrits avant de les constater.

### État des features

**F02, F03, F04 passent à `true`** — les premiers du projet, et ils viennent d'un
verdict mécanique reproductible, pas d'une impression. F01 est réordonné après le
socle : le spike est une mesure, pas une feature, et le harness ne l'attend pas.

### Prochaine session

`specs/002-capture/requirements.md`, nourrie du verdict du spike.

---

## Session — 2026-08-26 · Spike DOM (D20) et amendement de la spec 002

### Fait

Le **spike DOM** est construit, mesuré, et son verdict est consigné dans
`docs/spike-verdict-dom.md`. Zone **VERT** de la grille pré-enregistrée : trois
exécutions indépendantes contre l'org de démo donnent **100 %** de stabilité
post-pipeline (seuil 90), **100 %** de couverture, **0,02 %** de surcoût CPU
in-page (seuil 5).

Le résultat qui commande la conception : **l'enrichissement dégrade la
stabilité**, comme au spike UIA. Le nom accessible normalisé tient 100 % partout ;
ajouter les `data-*` en bloc fait tomber à 80 %. L'analyse clé par clé nomme le
responsable, `data-aura-rendered-by` (81,8 %), un identifiant de rendu qui
traverse la normalisation intact. Les `data-*` entreront donc dans l'ancrage par
**liste blanche sémantique**, jamais en bloc.

La **spec 002 existe enfin en fichiers**. Son texte ne vivait que dans un
message : on la citait de mémoire depuis des sessions. Extraite de la
transcription, déposée sans reformulation, puis amendée D19/D20 — `DomSource` à
côté de `UiaSource`, tâche 0bis, tâche 6 dédoublée en 6a/6b + 6c/6d, R2.5 à R2.7,
section Impact inter-specs complétée. Tâches 0 et 0bis cochées.

### Ce que la mesure m'a contredit

Quatre défauts trouvés **par** le spike, dont trois étaient les miens :

- Mon banc témoin fabriquait un `change` avec `composed: true`, ce qu'aucun
  navigateur ne fait. Il validait au vert un capteur **aveugle** aux changements
  de valeur. Depuis : événements natifs uniquement.
- J'ai affirmé que les racines shadow fermées empêchaient la capture. **Faux** :
  270 racines, **270 ouvertes, 0 fermée**. J'avais bâti l'hypothèse sur une
  absence d'événements sans la vérifier.
- Le ciblage par index dérivait à chaque clic — **76,9 % puis 7,7 %** sur le même
  protocole. Je mesurais mon propre script.
- Un tampon in-page ne survit pas à une navigation : la première phase large a
  perdu **100 %** de ses observations.

D22 reclasse un chiffre qui aurait mal vieilli : les **34,5 %** d'UIA ont été
mesurés sur Salesforce **dans un navigateur**, soit la classe de surface que D19
lui retire. Ils justifient le repli ; ils ne caractérisent pas le `UiaSource`,
dont la stabilité sur applications natives reste **non mesurée**.

### Vert

`pnpm verify` — 129 tests, 12 fichiers. Deux commits : `51c9309` (spike +
verdict), `1fd0e05` (spec 002 + D22/D23).

### Prochaine tâche

Spec 002, **tâche 1** — squelette Tauri v2 : tray 3 états, menu, hotkeys globaux
début/fin, CI `windows-latest` verte. Puis tâche 2 (traits `CaptureSource` +
`Clock`, `FakeSource` + `FakeClock`), qui débloque tout R1-R6 en CI.

### En attente

Rien. Aucun des quatre irréductibles n'a été touché.

### Coûts

Inchangés : Supabase `noe-prod` ~10 €/mois, Azure sur crédits. Le spike DOM n'a
rien coûté — org de démo, Chrome local, aucun appel modèle.

---
