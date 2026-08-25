# Inventaire de configuration — Noe

> Généré en session 0. **Tout ce qui était automatisable par CLI a été fait par
> l'agent** ; cette checklist ne garde en « À TOI » que l'inautomatisable.
>
> Détection effectuée le **2026-08-25** sur `Windows 11 Home 10.0.26200`.

## Légende

| Marque | Sens |
| --- | --- |
| ✅ | Vérifié présent et fonctionnel |
| 🔧 | Installé ou réparé par l'agent pendant la session 0 |
| ⏳ | Automatisable, bloqué sur un gate humain (coût, ou compte en production) |
| 👤 | **Toi seul** peux le faire — voir le récapitulatif final |
| ❌ | Absent, non résolu |

---

## A. Outils locaux

| Outil | Statut | Version détectée | Commande de vérification |
| --- | --- | --- | --- |
| Node LTS | ✅ | `v22.19.0` | `node -v` |
| pnpm | ✅ | `10.15.1` | `pnpm -v` |
| corepack | ✅ | `0.34.0` | `corepack -v` |
| Git | ✅ | `2.49.0.windows.1` | `git --version` |
| GitHub CLI | ✅ | `2.83.0`, authentifié `MartinPaviot` | `gh auth status` |
| rustup | 🔧 | `1.29.0`, installé par l'agent | `rustup --version` |
| cargo | 🔧 | fourni par rustup | `cargo --version` |
| tauri-cli | 🔧 | `cargo install tauri-cli` | `cargo tauri --version` |
| **VS Build Tools — workload C++** | 👤 | **aucune installation Visual Studio détectée** | `vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath` |
| WebView2 Runtime | ✅ | préinstallé Win11 | voir commande ci-dessous |
| Azure CLI | 🔧 | installé par l'agent | `az version` |
| Supabase CLI | 🔧 | **devDependency du dépôt**, pas un global | `pnpm supabase --version` |
| Stripe CLI | ✅ | `1.32.0` — ⚠️ **clé test expirée le 2026-02-28** | `stripe config --list` |
| Vercel CLI | ✅ | `50.37.3` — ⚠️ **token invalide** | `vercel whoami` |
| Biome · Vitest · gitleaks | ✅ | devDependencies / CI, jamais en global | `pnpm lint` · `pnpm test` |

**Vérifier WebView2 :**

```powershell
Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -Name pv
```

### 👤 A.1 — Visual Studio Build Tools, workload « Développement Desktop en C++ »

Prérequis Tauri sur Windows. Aucune installation Visual Studio n'existe sur ce
poste : sans ce workload, `cargo build` échouera sur l'édition de liens.

- [ ] Télécharger **Build Tools for Visual Studio 2022** :
      <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
- [ ] Dans l'installeur, onglet **Charges de travail**, cocher
      **« Développement Desktop en C++ »**
- [ ] Volet de droite, vérifier que ces composants sont cochés :
      `MSVC v143 - VS 2022 C++ x64/x86 build tools`, `Windows 11 SDK`,
      `C++ CMake tools for Windows`
- [ ] Installer (~6-7 Go), puis **rouvrir le terminal**
- [ ] Vérifier : `cargo --version` puis `cd apps/desktop && cargo tauri build --debug`

> ⏱️ ~25 min dont ~20 min de téléchargement sans surveillance.

---

## B. Azure — crédits Microsoft for Startups

> ⚠️ **Le piège documenté.** Sur une souscription sponsorisée, seuls les services
> **first-party** Azure consomment les crédits. Tout modèle **partenaire ou
> Marketplace** (Claude via Foundry, Mistral, Cohere…) est facturé **sur carte
> bancaire**, pas sur les crédits — des débits surprises ont été rapportés.
> D'où les deux règles ci-dessous, non négociables.

| Item | Statut | Vérification |
| --- | --- | --- |
| `az` installée | 🔧 | `az version` |
| `az` connectée à la souscription sponsorisée | 👤 | `az account show --query "{nom:name, id:id, etat:state}"` |
| Budget + alertes posés **avant** tout déploiement | ⏳ | `az consumption budget list -o table` |
| Ressource Azure OpenAI | ⏳ | `az cognitiveservices account list -o table` |
| Déploiement « mini » (tri) | ⏳ | `az cognitiveservices account deployment list -n <res> -g <rg> -o table` |
| Déploiement « frontier » (exécution) | ⏳ | idem |
| **Aucune carte bancaire** sur l'abonnement sponsorisé | 👤 | portail — voir B.2 |
| **Aucun modèle Marketplace activable** | 👤 | portail — voir B.2 |

### 👤 B.0 — PRÉALABLE BLOQUANT : Avast intercepte le TLS

`az login` **ne peut pas fonctionner** sur ce poste en l'état. Diagnostic complet :

1. `curl`, `winget/msstore` et `az` échouaient tous en TLS — ce n'était pas un hasard.
2. Le certificat réellement présenté par `login.microsoftonline.com` est émis par
   `CN=Avast Web/Mail Shield Root` : **Avast déchiffre et re-signe tout le trafic HTTPS.**
3. Windows fait confiance à ce root CA — d'où le fait que `gh` et le navigateur
   fonctionnent. Mais `az` est écrit en Python et utilise son propre magasin (`certifi`).
4. Un bundle CA a été construit (`~\.certs\ca-bundle-avast.pem`) et câblé via
   `REQUESTS_CA_BUNDLE`. L'erreur a alors changé — preuve que le bundle est bien lu.
5. **Erreur finale, non contournable :** `Basic Constraints of CA cert not marked
   critical`. Le root CA d'Avast viole la RFC 5280 sur ce point, et OpenSSL 3 le
   refuse **quel que soit le bundle**. Aucune correction côté client n'existe.

**Ce qu'il faut faire — une seule des deux options :**

- [ ] **Option 1 (recommandée, ~30 s)** — désactiver l'analyse HTTPS d'Avast :
      Avast → **Menu** → **Paramètres** → **Protection** → **Protections principales**
      → onglet **Agent Web** → décocher **« Activer l'analyse HTTPS »** → OK.
      Répare du même coup `curl`, `winget/msstore`, Python et Node.
- [ ] **Option 2 (ciblée)** — même écran, section **Exclusions** : ajouter
      `login.microsoftonline.com`, `management.azure.com`, `*.openai.azure.com`.
      Plus étroit, mais à rouvrir à chaque nouveau domaine.

Puis **redémarre le terminal** et dis-le-moi : je relance le device-code et je
te donne le lien et le code dans la foulée.

> Les variables `REQUESTS_CA_BUNDLE`, `SSL_CERT_FILE`, `CURL_CA_BUNDLE` et
> `NODE_EXTRA_CA_CERTS` posées pendant la session 0 restent inoffensives une fois
> l'interception coupée : le bundle contient les 147 CA publics standard en plus.

### 👤 B.1 — `az login` sur la bonne souscription

```sh
az login
az account list --output table          # repérer la souscription sponsorisée
az account set --subscription "<NOM OU ID SPONSORISÉ>"
az account show --query name -o tsv     # doit afficher la sponsorisée
```

> ⏱️ ~3 min. Ouvre un navigateur. **Vérifie bien la souscription active** : si un
> abonnement « Pay-As-You-Go » traîne, tout partirait sur carte.

### 👤 B.2 — Portail : retirer la carte et verrouiller le Marketplace

- [ ] <https://portal.azure.com> → **Cost Management + Billing** → profil de
      facturation de l'abonnement sponsorisé → **Payment methods** →
      **retirer toute carte bancaire enregistrée**
- [ ] Souscription → **Resource providers** → vérifier que
      `Microsoft.MarketplaceOrdering` n'est **pas** enregistré (le laisser
      `NotRegistered`)
- [ ] Souscription → **Policies** → optionnel mais recommandé : assigner la
      policy intégrée **« Not allowed resource types »** sur
      `Microsoft.MarketplaceOrdering/agreements` et `Microsoft.SaaS/resources`
- [ ] Dans Azure AI Foundry, **ne déployer que des modèles marqués
      « Azure OpenAI »** — jamais l'onglet « Partner & Community models »

> ⏱️ ~10 min. C'est la protection la plus rentable de toute la checklist.

### ⏳ B.3 — Ce que l'agent exécute dès que B.1 est fait (budgets d'abord)

```sh
# 1. Budget AVANT toute ressource
az consumption budget create --budget-name noe-mensuel --amount 200 \
  --time-grain Monthly --category Cost \
  --start-date <AAAA-MM-01> --end-date <AAAA-MM-01 +1an>

# 2. Ressource Azure OpenAI (first-party)
az cognitiveservices account create -n noe-openai -g noe-rg \
  --kind OpenAI --sku S0 -l swedencentral --custom-domain noe-openai

# 3. Les deux déploiements
az cognitiveservices account deployment create -n noe-openai -g noe-rg \
  --deployment-name noe-tri --model-name <mini> --model-format OpenAI \
  --sku-name Standard --sku-capacity 50
az cognitiveservices account deployment create -n noe-openai -g noe-rg \
  --deployment-name noe-execution --model-name <frontier> --model-format OpenAI \
  --sku-name Standard --sku-capacity 20
```

Endpoint et clé partent dans `.env`, **jamais** dans un fichier suivi.

### Anthropic — hors Azure, à dessein

Claude en direct = clé API Anthropic sur budget cash séparé. **Optionnel** tant
que F01 (le spike) n'a pas tranché l'étage exécution. Ne pas prendre d'abonnement
avant le verdict.

---

## C. Terrain — les deux variantes

> Le choix se fait en session 1 (F01), selon les 5 critères du prompt maître.
> Les deux variantes sont documentées ; **tu n'en fais qu'une**, après le verdict.

### C.0 — Règle commune

- App OAuth en **mode test**, sur ton compte réel
- **Sous un domaine neutre** — jamais le domaine Elevay. C'est la seule pièce
  réellement collante du montage : elle se démêle très mal après coup.
- Redirect URI en `localhost`, clés dans `.env` local, jamais commitées

### Variante A — Salesforce

**Environnement de démo (gratuit, permanent) :**

- [ ] Créer une **Developer Edition** : <https://developer.salesforce.com/signup>
      → username en `martin+noe@<domaine-neutre>` (jamais l'adresse Elevay)
- [ ] Noter le *My Domain* attribué (`https://xxx-dev-ed.develop.my.salesforce.com`)

**Connected App :**

- [ ] Setup → recherche **« App Manager »** → **New Connected App** →
      *Create an External Client App*
- [ ] **Connected App Name** : `Noe (dev)` · **Contact Email** : ton adresse
- [ ] Cocher **Enable OAuth Settings**
- [ ] **Callback URL** : `http://localhost:53682/callback`
- [ ] **Selected OAuth Scopes** — exactement ces deux, pas plus :
      - `Manage user data via APIs (api)`
      - `Perform requests at any time (refresh_token, offline_access)`
- [ ] **Décocher** *Require Proof Key for Code Exchange* ? → **NON, laisser coché** (PKCE requis)
- [ ] Save → attendre ~10 min la propagation
- [ ] App Manager → ta app → **View** → **Manage Consumer Details** →
      copier **Consumer Key** et **Consumer Secret**
- [ ] Les coller dans `.env` : `SALESFORCE_CLIENT_ID` / `SALESFORCE_CLIENT_SECRET`

> ⏱️ ~20 min dont 10 min d'attente de propagation.

### Variante B — Google Workspace

**Projet et écran de consentement :**

- [ ] <https://console.cloud.google.com> → **Nouveau projet** → nom `noe-dev`
      → sous **aucune** organisation si possible (« No organization »)
- [ ] **APIs & Services** → **Bibliothèque** → activer **Gmail API**
- [ ] **APIs & Services** → **OAuth consent screen** :
      - **User type** : **External**
      - **App name** : `Noe (dev)`
      - **User support email** : ton adresse
      - **Developer contact** : ton adresse
      - ⚠️ **Ne renseigner aucun domaine Elevay** dans *Authorized domains*
- [ ] **Scopes** → *Add or remove scopes* → ajouter exactement :
      - `https://www.googleapis.com/auth/gmail.readonly`
      - `https://www.googleapis.com/auth/gmail.compose`
- [ ] **Test users** → **ajouter ton propre email** (`martin.paviot@outlook.com`
      ou l'adresse Google que tu utiliseras)
- [ ] **Publishing status** : laisser sur **Testing** — surtout ne pas publier

**Client OAuth :**

- [ ] **Credentials** → **Create Credentials** → **OAuth client ID**
- [ ] **Application type** : **Desktop app** · **Name** : `Noe desktop (dev)`
- [ ] Créer → copier **Client ID** et **Client secret**
- [ ] Les coller dans `.env` : `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET`
- [ ] `GOOGLE_REDIRECT_URI=http://localhost:53683/callback`

> ⏱️ ~20 min. Le mode *Testing* limite à 100 testeurs et force un
> reconsentement tous les 7 jours — sans importance pour un usage personnel.

---

## D. Claude Code / MCP

| Item | Statut | Vérification |
| --- | --- | --- |
| Claude Code à jour | ✅ | `claude update` — `2.1.245`, dernière publiée |
| Ce dépôt comme workspace | ✅ | `CLAUDE.md` à la racine |
| `docs/prompt-maitre-v0.md` lu en début de session | ❌ | **fichier absent — voir D.1** |
| Serveur MCP requis pour construire | ✅ | **aucun**, à dessein |

### 👤 D.1 — Déposer le prompt maître

Le fichier annoncé comme « fourni à côté » est **introuvable** : recherche par
nom et par contenu sur tout `C:\Users\marti`, dossier `noe/` initialement vide.

En conséquence, `features.json` et `docs/invariants.md` ont été dérivés du brief
de session 0 et sont marqués **PROVISOIRE**. Il manque notamment les **5 critères
de choix du terrain**, que le template de verdict laisse en blanc.

- [ ] Déposer le contenu dans `docs/prompt-maitre-v0.md`
- [ ] Demander à l'agent de réconcilier `features.json` et `docs/invariants.md`

### MCP — plus tard, pas aujourd'hui

Aucun serveur MCP n'est nécessaire pour construire Noe. À la phase connecteurs
(F06), vérifier l'existence d'un serveur MCP officiel du système de vérité
retenu : Salesforce → <https://github.com/salesforcecli>, Google Workspace →
<https://github.com/googleapis>. **On ne configure rien aujourd'hui.**

---

## E. Socle commercial — branché sur l'existant Elevay

> Principe : **comptes réutilisés, données isolées.** Facturation sous l'entité
> existante (détenue à 100 %), ce qui est propre. Ce qui reste séparé **sans
> exception** : le projet Supabase, l'app OAuth Google, le dépôt public.

### E.1 — Stripe ⏳

Compte détecté : `acct_1SSP…NA2P` (tronqué) — *« Environnement de test New business »*.
⚠️ **La clé test a expiré le 2026-02-28.** Re-login requis avant toute action.

- [ ] 👤 `stripe login` (ouvre le navigateur, ~2 min)
- [ ] ⏳ Product **« Noe »** + Prices mensuel et annuel — **mode TEST**
- [ ] ⏳ Checkout Session + Customer Portal — **mode TEST**
- [ ] ⏳ Webhook `checkout.session.completed` → émission d'une clé de licence
      signée **ed25519**, vérifiée **hors ligne** par l'app (F11)
- [ ] 👤 **GATE — passage en live.** Le compte est celui d'Elevay en production :
      l'agent ne bascule jamais sans ta confirmation explicite

### E.2 — Supabase ⏳

- [ ] 👤 **Jeton d'accès personnel.** `supabase login` refuse de tourner hors TTY
      (`LegacyLoginMissingTokenError`). Générer un jeton sur
      <https://supabase.com/dashboard/account/tokens> puis :
      `setx SUPABASE_ACCESS_TOKEN "<jeton>"` — jamais dans un fichier suivi. (~2 min)
- [ ] ⏳ **GATE coût** — `supabase projects create noe-prod` dans l'org existante.
      **~10 $/mois de marginal** sur le plan payant. Projet **neuf**, jamais celui
      d'Elevay. Transférable vers une autre org plus tard.
- [ ] ⏳ Migrations initiales : `licences`, `compteurs`, `telemetrie_optin`
- [ ] ⏳ **RLS activée dès la première migration**, sur chaque table
- [ ] ⏳ **Lint anti-contenu en CI** dès la première migration : refuse toute
      colonne serveur capable d'accueillir du contenu utilisateur
- [ ] ⏳ Auth : email + Google
- [ ] ⏳ `supabase link --project-ref <ref>`

### E.3 — Vercel ⏳

⚠️ Token invalide détecté.

- [ ] 👤 `vercel login` (~2 min)
- [ ] ⏳ `vercel link` → projet **`noe`** dans la team existante
- [ ] ⏳ Landing + page de téléchargement + docs + **page trust**
- [ ] Le proxy edge attend que la cascade le justifie — pas maintenant

### E.4 — Resend · Sentry · PostHog ⏳

Comptes existants → **nouveaux projets et clés dédiés Noe**, jamais les clés Elevay.

- [ ] 👤 Fournir les tokens d'API si tu veux que l'agent crée les projets par CLI
- [ ] Sinon : créer les 3 projets à la main, coller les clés dans `.env`

### 👤 E.5 — Signature Windows : Azure Trusted Signing

**À activer tôt** : un installeur non signé déclenche SmartScreen et fait fuir un
visiteur de Product Hunt. ~10 $/mois, **first-party Azure** — donc couvert par les
crédits, contrairement aux modèles Marketplace.

- [ ] Portail Azure → créer une ressource **Trusted Signing Account**
- [ ] Créer un **Certificate Profile** de type **Public Trust**
- [ ] **Validation d'identité** : c'est l'étape longue — Microsoft vérifie
      l'entité juridique (documents d'entreprise). **Compter 1 à 5 jours ouvrés.**
- [ ] Une fois validé, l'agent branche la signature dans le build Tauri (F10)

> ⏱️ ~20 min de saisie, puis **plusieurs jours d'attente**. → À lancer **en premier**.

### E.6 — Cible Product Hunt (v1 vendable, 5-6 semaines)

Installeur signé + auto-update · onboarding + diagnostic de stack · capture +
bilan d'automatisabilité · brouillons quotidiens (Assisté) · checkout + clé de
licence · page trust + dépôt public.

Le moteur d'autonomie est l'upgrade annoncé **« en rodage »**, lancé quand le
harness l'a prouvé — **jamais promis avant**. Voir `docs/edition-boundary.md`.

---

## F. Ce que toi seul peux faire — dans l'ordre optimal

| # | Action | Section | Durée | Pourquoi cet ordre |
| --- | --- | --- | --- | --- |
| 1 | **Azure Trusted Signing** — lancer la validation d'identité | E.5 | 20 min **+ 1-5 j d'attente** | L'attente est le chemin critique. À lancer avant tout le reste. |
| 2 | **Déposer `docs/prompt-maitre-v0.md`** | D.1 | 2 min | Débloque `features.json`, les invariants et les 5 critères de F01. |
| 3 | **VS Build Tools — workload C++** | A.1 | 25 min (dont 20 sans surveillance) | Long téléchargement : à lancer en fond pendant les étapes suivantes. |
| 4 | **`az login`** + sélection de la souscription sponsorisée | B.1 | 3 min | Débloque budgets + Azure OpenAI. |
| 5 | **Portail Azure** — retirer la carte, verrouiller le Marketplace | B.2 | 10 min | À faire **avant** tout déploiement de modèle. |
| 6 | **`stripe login`** (clé test expirée) | E.1 | 2 min | Débloque le Product et le webhook en mode test. |
| 7 | **`vercel login`** (token invalide) | E.3 | 2 min | Débloque le projet et la landing. |
| 8 | **`pnpm supabase login`** | E.2 | 2 min | Débloque `noe-prod` — l'agent demandera le gate coût. |
| 9 | **OAuth du terrain retenu** (Salesforce **ou** Google) | C | 20 min | **Après le verdict F01** seulement — sinon tu configures le mauvais. |

**Total avant session 1 : ~45 min de travail actif** (étapes 1 à 8, l'étape 3
tournant en arrière-plan), plus l'attente de validation Trusted Signing.

L'étape 9 attend le verdict du spike.
