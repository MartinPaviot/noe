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
