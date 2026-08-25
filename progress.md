# Journal des sessions — Noe

## Session 0 — Initialisation du dépôt et inventaire de configuration

**Date :** 2026-08-25 · **Poste :** Windows 11 Home 10.0.26200

### Livré

- **Monorepo pnpm** — `apps/desktop` (coquille Tauri v2), `packages/core`,
  `packages/episode-spec` (licence MIT propre), `packages/harness` (CLI `noe`),
  `packages/connectors`. TypeScript strict partout
  (`noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `verbatimModuleSyntax`).
- **CI** — `.github/workflows/ci.yml` : deux jobs, `lint · typecheck · tests`
  (Biome + tsc + Vitest) et `scan de secrets` (gitleaks sur l'historique complet).
- **Docs** — `invariants.md`, `edition-boundary.md`, `spike-verdict.md` (template),
  `setup-checklist.md` (l'inventaire complet), `prompt-maitre-v0.md` (**vide**).
- **Racine** — `CLAUDE.md` (5 règles), `LICENSE` (AGPL-3.0), `SECURITY.md`,
  `README.md`, `.env.example`, `features.json` (12 features, toutes `false`).

### Vérification

`pnpm verify` vert : lint ✓ · typecheck 5/5 packages ✓ · tests 5/5 ✓.

### Réparé pendant la session

| Problème | Cause réelle | Résolution |
| --- | --- | --- |
| `rustup`, `cargo`, `az` absents | jamais installés | installés via winget (2ᵉ passe avec `--source winget`, la 1ʳᵉ échouait sur une source ambiguë) |
| Supabase CLI introuvable dans winget | pas de paquet winget | ajouté en **devDependency** du dépôt (`pnpm supabase`), conforme à la règle « pas de global » |
| `curl`, `winget/msstore`, `az login` échouent tous en TLS | **Avast Web/Mail Shield intercepte tout le TLS** et re-signe avec son root CA | bundle CA construit dans `~/.certs/`, `REQUESTS_CA_BUNDLE` · `SSL_CERT_FILE` · `CURL_CA_BUNDLE` · `NODE_EXTRA_CA_CERTS` posés en variables utilisateur |
| `az login` échoue **toujours** après le bundle | le root CA d'Avast a `basicConstraints` **non critique** — OpenSSL 3 le refuse par principe, aucun bundle ne corrige ça | **escaladé** : voir checklist B.0 |
| `biome migrate` a posé `"preset": "none"` | la migration 2.2 → 2.5 a mal traduit `recommended: true` | remis en `"preset": "recommended"`, vérifié en injectant une violation (`noExplicitAny` bien détecté) |

### Non livré, et pourquoi

- **`docs/prompt-maitre-v0.md` est vide.** Le fichier annoncé comme « fourni à
  côté » est introuvable — recherche par nom et par contenu sur tout
  `C:\Users\marti`, dossier `noe/` initialement vide. En conséquence,
  `features.json` et `docs/invariants.md` sont **dérivés du brief de session 0**
  et marqués PROVISOIRE. Les **5 critères de choix du terrain** manquent : le
  template de verdict les laisse en blanc, et F01 est bloqué sans eux.
- **Aucune ressource distante créée.** Azure, Supabase, Stripe et Vercel sont
  tous derrière un gate — coût, compte de production, ou authentification
  bloquée. Détail dans `docs/setup-checklist.md`.
- **La coquille Tauri n'a pas encore été compilée.** Le workload C++ de Visual
  Studio manque sur le poste (aucune installation VS détectée) ; c'est une étape
  strictement humaine.

### État des features

12 features, **0 à `"passes": true`**. Conforme : seul le juge mécanique promeut.

### Prochaine session

**Session 1 — F01, le spike.** Bloquée tant que le prompt maître n'est pas
déposé (les 5 critères de choix du terrain en dépendent).

---
