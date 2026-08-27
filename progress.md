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

## Session — 2026-08-26 (suite) · Spec 002, tâches 0 à 4

### Fait

La spec 002 **existe enfin en fichiers**. Son texte ne vivait que dans un
message : on la citait de mémoire depuis des sessions. Extraite de la
transcription, déposée sans reformulation, amendée D19/D20 — `DomSource` à côté
de `UiaSource`, tâche 6 dédoublée en 6a/6b/6c/6d, R2.5 à R2.7.

**Six tâches cochées : 0, 0bis, 1, 2, 3, 4.**

- **0** — verdict UIA inscrit au design : stratégie *globale filtrée* (seule sous
  le budget CPU, 3,16 % contre 8,48 %), walker profondeur 12 / 1500 nœuds.
- **0bis** — spike DOM, zone VERT.
- **1** — coquille Tauri : tray 3 états, menu, hotkeys avec refus notifié.
- **2** — horloge injectable, traits de capture, moteur temporel. Les quatre
  scénarios rejouables ; l'heure d'épisode du scénario timeout se rejoue en
  0,03 s de temps réel.
- **3** — pipeline de redaction : HMAC-SHA256 sur valeur normalisée, clé 256 bits
  sous DPAPI, miroir JSON des motifs consommé par Rust.
- **4** — writer JSONL fiable et kill-test d'un **vrai** processus tué.

### Ce que la mesure a contredit

Cinq défauts trouvés, dont quatre étaient les miens.

1. **Un numéro de téléphone français fuyait** (D24). `+33 6 12 34 56 78` — la
   graphie la plus courante — n'était réclamée par aucun motif : `TEL_FR`
   exigeait le chiffre collé à l'indicatif, `TEL_INTL` excluait la France. Trouvé
   en construisant les vecteurs partagés, avant qu'aucune capture ne tourne. Les
   79 tests existants ne l'avaient pas vu : ils vérifiaient les graphies
   auxquelles l'auteur avait pensé.
2. **La bibliothèque était inconsommable par Rust** (D25). Elle est déclarée en
   chaînes « pour que l'adaptateur Rust la lise telle quelle » depuis le premier
   jour ; le moteur a refusé net une anticipation négative qu'il ne supporte pas.
   La promesse était fausse depuis le début, et seule l'écriture du consommateur
   pouvait le révéler.
3. **Le test inter-implémentations était incomplet** — il comparait la détection
   mais pas l'arbitrage. C'est un avertissement de champ mort qui l'a signalé,
   pas une relecture.
4. **Rien n'appelait le battement.** Le vidage à 5 s ne se serait produit qu'au
   centième événement, et la clôture automatique à 60 minutes jamais. Les tests
   avançaient une horloge simulée à la main ; l'application réelle n'avait
   personne pour frapper.
5. **Le 34,5 % d'UIA ne mesurait pas ce qu'on lui faisait dire** (D22) : obtenu
   sur une surface navigateur, que D19 retire précisément à `UiaSource`.

### Ce qui est explicitement reporté

- **Snapshots redactés** (R4.5) → tâche 7, qui les crée. Le rédacteur est prêt et
  testé, mais il n'existe encore aucun snapshot à redacter.
- **Troisième implémentation des motifs** → tâche 6b. La bibliothèque est
  vérifiée sur deux moteurs, pas trois.
- **`allow(dead_code)` de `source.rs` et `moteur.rs`** → à retirer en tâche 6a,
  consigne inscrite dans la tâche.

### Vert

`pnpm verify` (88 tests TS) · `cargo test --all-targets` (88 de bibliothèque +
2 d'intégration) · les deux workflows verts sur `98e7f75`.

### Prochaine tâche

Spec 002, **tâche 5** — gaps système : veille, `seq_break`, pause à la reprise,
timeout. Le moteur produit déjà les gaps de veille et de timeout ; il reste à
brancher les événements système Windows réels et à étendre l'enum côté
`episode-spec` avec sa note dans `decisions.md`.

### En attente

Rien. Aucun des quatre irréductibles n'a été touché.

### Coûts

Inchangés — Supabase `noe-prod` ~10 €/mois, Azure sur crédits. Aucun appel modèle
de la session.

---

## Session — 2026-08-26 (fin) · Spec 002, tâche 5

### Fait

**Tâche 5 cochée — gaps système.** Sept tâches de la spec 002 sont vertes : 0,
0bis, 1, 2, 3, 4, 5.

La **veille** se mesure au lieu de se deviner. Windows tient deux compteurs —
`GetTickCount64` (veille comprise) et `QueryUnbiasedInterruptTime` (veille
exclue) — dont l'écart *est* le temps suspendu. Guetter `WM_POWERBROADCAST`
aurait exigé une fenêtre et une boucle de messages, beaucoup de code non
testable ; et une notification manquée est perdue, là où un écart de compteurs se
rattrape au battement suivant.

`seq_break` naît désormais d'une **discontinuité de numéros**, pas seulement
d'une ligne coupée : une ligne disparue proprement ne laisse aucune autre trace.
Les numéros manquants sont nommés un par un — savoir qu'il en manque trois ne dit
pas où.

Le trou de **pause** s'écrit à la reprise, quand sa borne de fin existe enfin ;
une pause jamais reprise se termine à la clôture.

### Ce que les tests ont corrigé

- **Le type `Veille` porte deux durées**, parce qu'un test a saturé. Un épisode
  ouvert deux secondes avant une veille de quatre-vingt-dix en subit
  quatre-vingt-dix, mais seules deux tombent dans son intervalle. Les confondre
  ferait apparaître des veilles minuscules là où la machine a dormi une nuit.
- **Le test croisé des causes de gap a été vérifié capable d'échouer**, en
  amputant le miroir. Un test qui ne peut pas rougir ne prouve rien — la leçon
  du témoin qui fabriquait ses propres événements, plus tôt dans la session.
- **Biome et le générateur se disputaient les miroirs JSON.** Le formateur
  repliait les tableaux courts, le vérificateur les déclarait périmés à chaque
  `pnpm format`. Les fichiers générés sont désormais exclus du formateur : deux
  outils qui se battent pour un fichier finissent par faire désactiver le plus
  utile des deux.

### Vert

`pnpm verify` · `cargo test --all-targets` : **110 tests de bibliothèque + 2
d'intégration**, 88 TypeScript. Les deux workflows verts sur `f7d9036`.
**43 commits.**

### Prochaine tâche

Spec 002, **tâche 6a** — `UiaSource` réel. C'est une surface entièrement
nouvelle : bindings `uiautomation`, stratégie *globale filtrée* et paramètres de
walker fixés en tâche 0, `RawEvent` portant `source:"uia"`. À faire à cette
occasion, consigne déjà inscrite dans la tâche : **retirer les
`#![allow(dead_code)]`** de `source.rs` et `moteur.rs`, qui ne couvrent que
l'attente de ce consommateur.

### En attente

Rien. Aucun des quatre irréductibles n'a été touché de toute la session.

### Coûts

Inchangés — Supabase `noe-prod` ~10 €/mois, Azure sur crédits. Aucun appel modèle
de la session.

---

## Session — 2026-08-27 · D26, D27, et la première fenêtre

### Fait

**Spec 002 à 11/20.** Tâches 6a, 7, 8 et 8bis cochées, plus D27 tranché et
implémenté.

- **6a — adaptateur UIA réel.** Vérifié sur un vrai bureau : 5 boutons invoqués
  via UIA → exactement 5 `invocation` captées, plus focus et structure.
- **7 — snapshots.** Photo réelle du Bloc-notes contenant un courriel et un
  téléphone que je venais d'y taper : racine `window`, 37 nœuds, 2 522 octets,
  **0 PII restante**.
- **D27 — hook clavier**, accordé par l'opérateur, posé pendant l'épisode
  seulement. 1 `Ctrl+C` → 1 copie, 2 `Ctrl+V` → 2 collages, `Ctrl+A`, `Ctrl+S`
  et tout le texte tapé comptés pour **rien**.
- **8 — assemblage.** Le harness TypeScript **accepte** un épisode produit par
  Rust, et les deux rendent le même grade et la même raison, caractère pour
  caractère.
- **8bis — le squelette traversant.** La fenêtre existe, s'ouvre depuis le menu
  de barre d'état, et lit les vrais épisodes. Quatre baselines Playwright.

**Specs 004, 005 et 006 déposées** (texte opérateur, sans reformulation), et
l'**evidence quotidienne** tourne — elle capture désormais la vue elle-même.

### Ce que les contrôles ont trouvé

Sept défauts, tous les miens, et aucun n'aurait été vu par relecture.

1. **Le seuil de grade était mal miroité.** `gradeOf` tolère au plus UN défaut
   pour rester en B ; j'avais écrit « zéro trou et zéro entité non résolue ». Le
   harness a refusé l'épisode — le bon comportement — mais la divergence ne s'est
   vue qu'en produisant un épisode complet et en le lui soumettant. Elle est
   désormais figée dans `vecteurs-grade.json`.
2. **`Abonnement` mentait.** Son commentaire promettait « le relâcher coupe le
   flux » ; le type ne portait qu'un champ privé vide.
3. **Les rôles UIA et DOM ne parlaient pas la même langue** — `Button` contre
   `button`. Les clés de branches de la 004 ne se seraient jamais rejointes.
4. **Les tests visuels étaient instables** : 4, puis 1, puis 5 tests passés sur
   trois exécutions du même code. En série, cinq passent en 4,6 s.
5. **Mon script d'evidence laissait un serveur qui écoutait encore.** `kill()` ne
   suffit pas sous Windows quand `shell: true` interpose un interpréteur.
6. **La vue restait indéfiniment « pas prête »** si une seule frise échouait —
   `Promise.all` rejetait, et le drapeau n'arrivait jamais.
7. **`test-results/` est entré dans le dépôt**, et la CI est tombée dessus.

### Ce qui reste ouvert, et déclaré

- **Le filtre « surfaces activées »** (R5.4) — rattaché à la tâche 9. La copie est
  lue dès qu'un `Ctrl+C` est observé pendant un épisode ; il manque la
  vérification que la fenêtre au premier plan est autorisée.
- **`Text_TextChanged`** n'a pas été observé — le Bloc-notes ne le lève pas.
  Dépendant de l'application, et non déclaré comme prouvé.
- **La troisième implémentation des motifs** (in-page JS) → tâche 6b.

### Vert

`pnpm verify` · `cargo test --all-targets` : **197 tests Rust + 2 d'intégration
+ 5 visuels**. Les deux workflows verts sur `94b2c55`. **54 commits.**

### Prochaine tâche

Spec 002, **tâche 9** — pause étanche et liste blanche vide, qui porte aussi le
filtre des surfaces activées laissé par D27. Puis 10 (panique), 6b (extension
MV3 + native messaging), et le gate.

### Coûts

Inchangés. Aucun appel modèle de la session.

---

## 2026-08-27 (2) — Tâche 9, et dix défauts trouvés par une revue adverse

### Fait

**Tâche 9 de la spec 002** — pause étanche (R5.2) et liste blanche des surfaces
(R5.4). Arbitrée en **D28** : hors périmètre, on compte et on ne raconte pas.

Puis une **revue adverse** (trente agents, cinq angles, réfutation
contradictoire) a rendu dix défauts confirmés. Ils sont tous fermés, chacun avec
son test. Quatre arbitrages en sont sortis : **D29** (motifs v4 et juge
indépendant), **D30** (chemin de clôture unique, horloge unique), **D31** (jeton
sur 65 bits en base32), **D32** (les photos aussi passent par la liste blanche).

### Ce que la revue a trouvé, et que je n'avais pas vu

Par ordre de gravité, pas de difficulté :

1. **Trois graphies de numéro traversaient la redaction en clair** — `+33 (0)6 …`,
   qui est la graphie d'affichage standard française, `0033 …`, et n'importe quel
   numéro séparé par des insécables. Vérifié par exécution, pas par lecture.
2. **Le juge R4.6 s'auto-validait** : il cherchait des PII avec la bibliothèque
   même qui avait servi à redacter. Aveugle par construction — c'est pour ça que
   le point 1 avait pu passer trois fois.
3. **La lecture du presse-papiers** partait sur un `Ctrl+C` observé n'importe où
   sur le poste, sans vérifier ni la surface ni que quelque chose ait vraiment
   été copié. Un `Ctrl+C` dans un gestionnaire de mots de passe était lu et haché.
4. **Elle tournait même hors épisode** : le bloc précédait la garde d'épisode
   ouvert, et `desarmer()` ne purgeait pas les compteurs.
5. **Les copies et collages n'atteignaient jamais le moteur.** Le vecteur était
   construit, rempli, puis abandonné à la fin de l'itération. Rust n'a rien dit —
   un `push` compte comme un usage. `Declencheur::CopierColler` ne pouvait pas se
   produire en production.
6. **Le hook clavier n'était jamais retiré.** `Drop` postait `WM_QUIT` sur le fil
   `0`, croyant à une diffusion que `PostThreadMessage` n'a pas. Un hook de plus
   par épisode, tous chaînés : au troisième, un `Ctrl+V` comptait trois collages.
7. **`armer()` puis `poser()` s'effaçait lui-même** — l'affectation droppe
   l'ancien hook après avoir évalué le nouveau, et son `Drop` désarme.
8. **La clôture automatique à 60 min perdait l'épisode.** Journal jamais fermé,
   aucun assemblage, source et hook laissés vivants. Une heure de travail ne
   produisait aucun `episode.json`.
9. **La reprise après crash n'était pas branchée.** `orphelins` n'avait d'autre
   appelant que le binaire de banc. Le kill-test validait la fonction, pas son
   branchement.
10. **Deux origines de temps monotone** dans le même journal : le délai
    d'inactivité de 2 s partait après 1 s, et tous les gaps ressortaient
    horodatés à `t1`.

Et une onzième, trouvée par le banc lui-même : le test de non-collision a rougi.
Il avait raison — 32 bits de jeton donnent 1,2 % de collision sur dix mille
entités, et une collision **invente** une jointure au lieu d'en perdre une.

### Ce qui n'est toujours pas prouvé

- **La revue est une lecture de code.** Rien n'a été exécuté sur un vrai bureau
  Windows avec un épisode ouvert : ni le comptage réel du hook après trois
  épisodes, ni le timing de la lecture du presse-papiers, ni la clôture à
  soixante minutes, ni ce que `get_focused_element()` rend sur un bureau
  sécurisé. Les corrections sont testées ; leur effet en production ne l'est pas.
- **`completeness.out_of_scope` n'entre pas dans le grade.** Un épisode de deux
  actions avec quarante refus est gradé comme un épisode de deux actions sans
  refus. Le seuil appartient à la spec 001 et vit en dix vecteurs miroités ; à
  trancher au gate.
- **`paste{paired:false}`** passe par `payload`, faute de champ `paired` au
  format. Décision de spec 001, à prendre au gate ; perdre l'information en
  attendant n'en était pas une.

### Vert

`pnpm verify` · `cargo test --all-targets` : **247 tests Rust + 2 d'intégration
+ 5 visuels**, 169 TypeScript. **59 commits.**

### Prochaine tâche

Spec 002, **tâche 10** — panique (fenêtres 5/15/60, suppression d'épisodes
entiers, irréversibilité). Puis 6b (extension MV3 + native messaging), 11
(export/import), 12, 13, et le gate.

### Coûts

Inchangés. La revue adverse a consommé environ 2,9 M de jetons de sous-agents.

---

## 2026-08-27 (3) — Le pont DOM, de la page jusqu'au tuyau

### Fait

**Tâches 10, 11, 6b, 6c et 6d.** Spec 002 : **17/20**.

- **10 — panique.** Trois fenêtres, épisodes entiers, jamais de découpe. Trois
  formes d'épisode coexistent sur le disque et chacune se date autrement ; celui
  en quarantaine n'a que son identifiant, d'où le décodage du ULID.
- **11 — export/import.** AES-256-GCM, PBKDF2 à 600 000 itérations, clé HMAC
  enveloppée. La migration de machine est prouvée de bout en bout : même entité,
  même jeton, autre poste.
- **6b, 6c, 6d — le pont DOM.** Extension MV3, hôte de native messaging, tuyau
  nommé restreint au compte courant. Verdict complet dans
  [`docs/verdict-pont-dom.md`](docs/verdict-pont-dom.md).

### Ce que la mesure a montré et que la relecture n'aurait pas vu

Six défauts, tous sur le pont DOM, tous trouvés en regardant ce qui sort du
tuyau — pas en relisant le code.

Deux méritent d'être retenus. **Chaque clic arrivait en double** : le `click` est
`composed: true`, il réveillait l'écouteur du document ET celui de la racine
shadow, et l'épisode aurait compté deux actions pour un geste. Et **le serveur de
tuyau ne servait qu'une connexion à la fois** : Chrome redémarre l'hôte à chaque
relance du service worker, le nouveau ne trouvait personne, et la capture
navigateur s'arrêtait en silence pour le reste de l'épisode.

Le sixième était dans le banc : `Number(argv[i + 1] || 5)` rendait `NaN`, la
boucle ne tournait pas une fois, et « PILOTAGE TERMINE » s'affichait sans que
rien n'ait été piloté. L'absence d'observations s'est lue une heure durant comme
un défaut de capture qui n'existait pas. C'est le coût d'un no-op silencieux.

### Ce qui n'est toujours pas prouvé

- **Une page de démonstration n'est pas Salesforce.** Quatre racines shadow
  contre 270, aucun re-rendu d'Aura pendant la mesure.
- **Le pont n'a pas tourné pendant un épisode réel.** `DomSource` est branchée et
  testée, mais l'aller-retour capture → journal → épisode assemblé reste à faire.
- **L'empreinte du transport n'est pas mesurée** — c'est la tâche 13.
- Rien sur Edge ni Firefox : l'hôte n'est déclaré que sous la clé Chrome.

### Vert

`pnpm verify` · `cargo test --all-targets` : **295 tests Rust + 2 d'intégration
+ 5 visuels**, 176 TypeScript. **12 commits poussés**, les deux workflows verts
sur `dc15103`.

### Prochaine tâche

**12** (canaris sur capture réelle), **13** (empreinte mesurée, les deux sources
sur leur propre classe de surface), puis le **gate**.

### Coûts

Inchangés côté modèle. Deux écritures hors dépôt, dans le profil de l'opérateur :
le manifeste d'hôte sous le dossier de données de l'application, et une clé sous
`HKCU`, branche `Software/Google/Chrome/NativeMessagingHosts`.
`node scripts/installer-pont-dom.mjs --desinstaller` défait exactement ces
deux-là.

---

## 2026-08-27 (4) — La spec 003 jusqu'au mur, et le mur est un coffre

### Ce qui s'est passé

Le gate de la spec 002 est franchi, la 003 est ouverte, et son premier prérequis
est tombé : le coffre DPAPI qui portait les identifiants de l'org de démo a
disparu du poste. L'incident est daté dans `docs/decisions.md`. Les pistes
**locales** de récupération sont désormais épuisées, et c'est consigné : aucun
fichier `.dpapi` ne subsiste dans le profil utilisateur, la corbeille n'en
contient aucun, et les clichés instantanés de volume demandent des droits
d'administration que la session n'a pas.

La consigne était de continuer sur ce qui n'est pas bloqué. Il restait plus de
travail que prévu.

### Fait

- **Tâche 4** — l'adaptateur CRM. Tout ce qui transforme et tout ce qui construit
  une requête, plus l'implémentation complète de `Federation`. 40 tests.
- **Tâche 11** — l'adaptateur Gmail, même forme. 41 tests.
- **Tâche 2** — l'échange de jetons et le rafraîchissement. 47 tests.
- **Tâche 6** — les candidates : d'où viennent les clés fortes, et à qui elles
  s'adressent. Plus un routeur qui porte les deux connecteurs derrière un seul
  `Federation`. 23 tests.
- **Tâche 10** — la fédération entre dans l'épisode, et **le grade A s'ouvre**.
  11 tests.
- **D35** — le transport HTTP, et ce qu'un porteur de jeton doit refuser.
  19 tests.
- **D36** — une résolution empêchée n'est pas une résolution négative.

### Ce que l'écriture a trouvé et que la relecture n'aurait pas vu

Trois défauts, tous de la même famille : **un miroir qui ne transporte pas ce
qu'il ressemble.**

1. **`resoudre` rendait trois issues au lieu de quatre.** Un adaptateur qui prend
   un `403` n'avait qu'un choix : répondre `Introuvable`. Or `not_found` affirme
   que l'enregistrement n'existe pas — c'est une conclusion, et elle envoie
   chercher au mauvais endroit. Le contrat TypeScript, lui, rend un `Result` : le
   miroir Rust l'avait aplati en oubliant ce qu'il portait.
2. **Le miroir de `Entity` avait perdu deux champs.** `resolved` — la clé qui a
   tranché, sans laquelle une résolution fausse est indiagnosticable — et
   `state_meta`. Le registre jetait la première ; il la garde.
3. **Le `Debug` de `Jetons` était dérivé.** Il imprimait le jeton d'accès et
   celui de rafraîchissement en clair, à côté d'un `Pkce` et d'un `ClientHttp`
   qui masquent tous les deux les leurs.

Et un quatrième, trouvé par un contrôle qui a mordu : le suffixe de contrôle des
identifiants Salesforce a **refusé deux valeurs que j'avais inventées pour les
tests**. C'est la quatrième fois de la journée qu'un garde-fou est vu échouer
avant d'être cru.

### Ce qui n'est toujours pas prouvé

- **Aucun de ces adaptateurs n'a parlé à une org.** Tout est vérifié contre des
  réponses enregistrées et un serveur de boucle locale. C'est honnête, ce n'est
  pas la même chose.
- **Six modules attendent un jeton** — `federation`, `oauth`, `transport`,
  `salesforce`, `gmail`, `candidates` — chacun avec un `allow(dead_code)` qui
  nomme la tâche chargée de le retirer. C'est beaucoup, et c'est le prix de
  l'incident.
- **Le worker n'a jamais démarré en production.** `fusionner_federation` n'a pas
  d'appelant pour la même raison : poser un appel qu'aucun test ne peut exercer
  aurait été un garde-fou décoratif de plus.

### Vert

`pnpm verify` · `cargo test --all-targets` : **517 tests Rust + 2 d'intégration
+ 5 visuels**, 248 TypeScript. Clippy strict propre. Six commits poussés.

### Prochaine tâche

**0** — le terrain, et il faut pour cela reprendre la main sur
`contact+noespike@elevay.app`. Tout le reste de la 003 en dépend : les tests
d'intégration de la 4 et de la 11, la connexion de la 2, les canaris de la 9, la
cohérence de la 12, et le jalon de la 13.

### Coûts

Aucune écriture hors dépôt cette session. Une dépendance ajoutée : `ureq` et ses
dix caisses transitives, choisie contre `reqwest` parce que tout le code de Noe
est synchrone et qu'un ordonnanceur complet pour faire des GET dans un fil qui
n'a rien d'autre à faire serait du poids sans contrepartie.

---

## 2026-08-27 (5) — Ce que « non bloqué » voulait encore dire

### Ce qui s'est passé

J'avais annoncé qu'il ne restait plus rien à avancer sans l'org. C'était faux, et
la spec le disait : R1.1 exige que **le code n'encode jamais le CRM hors de son
adaptateur**, et `candidates.rs` le nommait dans une constante depuis le matin
même. Relire l'exigence a rouvert une demi-journée de travail.

### Fait

- **R1.1** — la forme d'une URL Lightning et l'algorithme du suffixe de contrôle
  sont des faits sur Salesforce : ils vivent dans `salesforce.rs`, les
  identifiants de fil dans `gmail.rs`. `terrain.rs` lit et valide
  `terrain.json`, qui porte le choix. Changer de CRM, c'est changer une ligne.
- **R5.3** — le budget d'appels aussi vient du terrain, pas d'une constante.
- **Tâche 9** — une troisième famille de canaris, `hors_perimetre`, et un sweep
  qui sait enfin **dire non**.
- **Tâche 0** — `plan.mjs` (pur, dix tests) et `peupler.mjs` (rejouable, et qui
  refuse de faire semblant sur l'historique des champs).
- **Un défaut OAuth réel**, trouvé par un test devenu instable.

### Ce que l'instabilité a appris

Un test s'est mis à échouer une fois sur six. La tentation était de le relancer.
En cherchant : `attendre()` met l'écouteur en non-bloquant pour borner l'accept,
et **la socket acceptée en hérite**. La lecture rendait `WouldBlock` dès que les
octets du navigateur n'étaient pas déjà arrivés — et le délai de lecture ne
s'applique pas à une socket non bloquante, donc il ne rattrapait rien.

En production, l'écart entre la connexion et la requête est plus grand qu'en
test. Le défaut mordait donc **plus** souvent sur un vrai navigateur, et il se
serait lu « la connexion OAuth ne marche pas », sans rien pour dire pourquoi. Un
test déterministe le garde maintenant : le navigateur attend quatre cents
millisecondes avant d'envoyer sa requête.

**Un test instable n'est pas un test fragile. C'est un défaut qui se montre une
fois sur six.**

### Les contrôles vus échouer

Neuf tests écrits cette session ont été mis en échec avant d'être crus : les
quatre de la revue adverse, les deux du budget de terrain, et les trois de la
socket. Le sweep de canaris, lui, n'avait **aucun** test de sa propre détection —
il ne balayait que des sorties propres, donc un `includes` cassé serait passé
inaperçu pour toujours. Il en a un.

### Ce qui n'est toujours pas prouvé

Rien n'a changé de ce côté : aucun adaptateur n'a parlé à une org, sept modules
attendent un jeton, et le worker n'a jamais démarré en production.

### Vert

`pnpm verify` · `cargo test --all-targets` : **550 tests Rust + 2 d'intégration
+ 5 visuels**, 260 TypeScript. Six passes complètes de suite sans une seule
instabilité. Clippy strict propre.

### Prochaine tâche

Toujours la **0**, et toujours pour la même raison. Mais le jour où l'accès
revient, la suite tient en trois commandes : `sonder`, `peupler`, puis la
connexion OAuth.

### Coûts

Aucune écriture hors dépôt. Un chemin codé en dur retiré de `sonder.mjs` : il
pointait sur un profil utilisateur nommé, et un outil qui ne tourne que sur une
machine n'est pas un outil.

---

## 2026-08-27 (6) — L'audit des exigences, une par une

### Ce qui s'est passé

Deux fois j'avais annoncé qu'il ne restait rien à avancer sans l'org. Deux fois
c'était faux. Cette fois, au lieu de conclure à l'intuition, j'ai relu les vingt
et une exigences de la spec 003 et vérifié chacune contre le code.

**Cinq d'entre elles n'étaient pas tenues.** Aucune ne demandait l'org.

### Fait

- **R5.1** — « TOUTE requête DOIT passer par le client commun ». Le client
  TypeScript existait depuis la tâche 3 ; le chemin Rust appelait le transport
  **une fois**, sans reprise, sans `Retry-After`, sans rafraîchissement sur 401.
  Un `429` devenait un trou définitif.
- **R3.1** — « restreint aux `scope_fields` **plus les champs observés
  changés** ». La seconde moitié n'existait pas.
- **R3.3** — le verdict sur l'état d'avant existait en TypeScript, pas en Rust.
- **R4.1** — la jointure des changements était écrite, la **collecte** ne l'était
  nulle part.
- **R4.4 et R1.2** — deux exigences gardées par une intention et rien d'autre.

### Ce que l'audit a appris

**On ne respecte pas un en-tête qu'on ne lit pas.** R5.1 exige `Retry-After` ;
le transport ne rendait que le statut et le corps. L'exigence était inatteignable
par construction, et rien ne le disait. La signature a changé.

**Les libellés sont traduits.** Rapprocher un champ vu à l'écran d'un nom d'API
demande un index, et une table écrite à la main marcherait sur la machine de son
auteur et nulle part ailleurs. L'index vient du `describe` de l'org.

**Aucune API ne dit si un champ est historisé.** C'est pourquoi `terrain.json`
porte désormais `field_history`, établi par l'expérience dans `peupler.mjs`. Sans
lui, « l'historique est vide » et « le champ n'est pas suivi » sont
indistinguables — et mènent à des conclusions opposées.

**« Maintenant » n'est pas une borne.** Le delta n'en avait pas de haute : il
dérive si l'appel est retardé ou repris, et un changement postérieur à la clôture
serait devenu un trou attribué à un épisode fermé.

**Le scan de secrets ne connaissait aucun jeton.** Vérifié sur fichier témoin :
les quatre formes qui comptent ici passaient toutes.

### Et une leçon dans la leçon

Mon premier témoin de jeton était bricolé à la main. Trois règles sur quatre
mordaient, et l'explication qui m'est venue — « le préfiltre désactive la règle en
silence » — était fausse. C'est le jeu par défaut de gitleaks qui écarte les
trouvailles contenant ses mots-vides : **le témoin n'en était pas un**. J'ai
failli committer cette explication avant de la vérifier.

Un diagnostic plausible qu'on n'éprouve pas est une deuxième erreur posée
par-dessus la première, et celle-là on la relit sans la voir.

### Ce qui n'est toujours pas prouvé

Rien n'a changé : aucun adaptateur n'a parlé à une org, huit modules attendent un
jeton, et le worker n'a jamais démarré en production. Ce qui a changé, c'est
qu'ils sont maintenant conformes à ce que la spec leur demande.

### Vert

`pnpm verify` · `cargo test --all-targets` : **599 tests Rust + 2 d'intégration
+ 5 visuels**, 260 TypeScript. `pnpm secrets` propre sur 96 commits. Clippy
strict propre.

### Prochaine tâche

Toujours la **0**. Après l'audit, il ne reste plus une seule exigence de la spec
003 que je puisse satisfaire sans l'org : R7.2 et R7.3 demandent des épisodes
réels, et tout le reste est tenu.

### Coûts

Aucune écriture hors dépôt. Les témoins de jetons créés pour éprouver le scan ont
été tirés au hasard, écrits dans le répertoire temporaire de la session, et
effacés — aucun n'a jamais été un vrai jeton, et aucun n'a touché le dépôt.

---

## 2026-08-27 (7) — L'audit des miroirs

### Ce qui s'est passé

L'audit des exigences était clos. Restait un autre axe, que la veille avait
laissé ouvert : **qu'est-ce qui vit en double entre les deux langages, sans que
rien ne le vérifie ?** J'avais moi-même créé un cas la veille, en recopiant à la
main les constantes du client TypeScript en Rust.

Quatre miroirs plus tard, trois divergences réelles.

### Fait

- **Le client de reprise** — constantes et, surtout, **soixante vecteurs de
  sortie**. Comparer `TENTATIVES_MAX = 5` des deux côtés ne prouve presque rien :
  les constantes sont la partie qu'on relit. Ce qui diverge en silence, c'est
  l'arithmétique.
- **Les règles de résolution** — l'ordre de force des clés et la normalisation
  des identifiants, jusque-là garanties par un commentaire dans quatre fichiers.
  La normalisation vit maintenant une seule fois.
- **Le corpus doré**, que rien ne lisait côté Rust.
- **Un épisode « tout allumé »**, parce que le corpus doré n'exerce ni
  `resolved`, ni `state_meta`, ni `supersedes`.

### Les trois divergences

**`api_change`.** Le schéma porte un quatrième type d'événement depuis la spec
001. Le miroir Rust ne l'a jamais eu : le type refusait les cinq épisodes du
corpus, purement et simplement, avec un « unknown variant » qu'aucun banc n'avait
jamais provoqué. Un capteur qui aurait écrit un épisode que le harness refuse ne
l'aurait découvert qu'au rejeu. Et c'est précisément là que les changements
collectés par R4.1 devaient atterrir.

**`supersedes`.** Il porte l'INVARIANT IV — un épisode clôturé n'est jamais
modifié, une correction produit un épisode neuf qui référence l'ancien. Le type
Rust ne l'avait pas : une lecture-écriture l'aurait **effacé en silence**, et avec
lui le seul fil qui relie une correction à ce qu'elle corrige.

**`degraded.reason`** — celle-là est la mienne. J'ai inventé un champ qui
n'existe nulle part, Zod l'a accepté sans un mot parce que ses objets ne sont pas
stricts par défaut, et c'est le miroir Rust qui l'a signalé. En le comptant comme
un champ perdu, ce qu'il était, mais pour la mauvaise raison.

### Ce que ça apprend sur les contrôles

**Un contrôle ne vaut que ses vecteurs.** J'ai mesuré, plutôt que supposé, que le
corpus doré n'exerce pas trois des champs d'entité. Le générateur de l'épisode
complet **refuse** désormais de produire un fichier qui n'allume pas tout ce que
le schéma déclare — le jour où quelqu'un ajoute un champ, ça s'arrête là, et pas
six mois plus tard sur un épisode réel.

**Et un contrôle doit refuser dans les deux sens.** Exiger que tout soit exercé
ne dit rien de ce qui est exercé en trop.

### Vert

`pnpm verify` · `cargo test --all-targets` : **611 tests Rust + 2 d'intégration
+ 5 visuels**, 260 TypeScript. **Sept miroirs vérifiés** dans `pnpm verify`.
Clippy strict propre. Les deux workflows verts.

### Prochaine tâche

Toujours la **0**. Les exigences sont tenues, les miroirs tiennent ensemble.

### Coûts

Une dépendance ajoutée : `@types/node` sur `@noe/core`, qui n'avait pas de build
du tout. En lui en donnant un, le compilateur a refusé `setTimeout` — le type de
ce global n'arrivait que par les **fichiers de test**, qui importent vitest. Le
typage d'un module de production dépendait de la présence de ses bancs.

---

## 2026-08-27 (8) — L'audit des invariants

### Ce qui s'est passé

Troisième axe, après les exigences et les miroirs : **`docs/invariants.md`
énonce sept invariants. Lesquels sont gardés par une mécanique ?**

Deux ne l'étaient par rien du tout, et ce sont les deux premiers de la liste.

### INVARIANT VI — « `@noe/core` est pur »

Quatre violations y vivaient : un `setTimeout` et deux `Math.random` en valeurs
par défaut dans `client.ts`, un `ulid()` dans `close.ts`. Aucune n'était
malveillante ; chacune était une commodité. C'est précisément pour ça qu'un
invariant a besoin d'un banc — **personne n'écrit une violation en se disant
qu'il en écrit une.**

Le plus gênant : le compilateur me l'avait dit la veille. En donnant un build à
`@noe/core`, `tsc` a refusé `setTimeout`, et j'ai fait taire le message en
ajoutant `@types/node` — c'est-à-dire en déclarant que ce paquet a le droit
d'avoir une horloge. Il ne l'a pas.

### INVARIANT I — « aucun contenu utilisateur ne quitte le poste »

La première des cinq règles, gardée par des commentaires. Le banc pose trois
contrôles : chaque `fetch` de l'extension vise une ressource empaquetée, aucune
autre voie d'émission n'y existe, et le capteur ne parle au réseau que par
`transport.rs`.

Le `fetch` de `motifs.js` visait bien une ressource locale — mais la cible était
calculée trois lignes plus haut. L'appel a été recollé : **une garantie qui
demande au lecteur de la suivre est une garantie qu'un jour il ne suivra pas.**

### Ce que j'ai failli committer

J'ai d'abord écrit, dans la documentation du banc, que le commentaire du service
worker mentait en affirmant « aucun `fetch` ». Il ne ment pas : il parle de **ce
fichier-là**, où c'est vrai. Ce qui manquait, c'est que personne ne regardait le
reste de l'extension.

La différence entre « rien ne sortait » et « rien ne le vérifiait » est toute la
valeur d'un banc — et j'ai failli écrire l'inverse dans le dépôt.

### Vert

**612 tests Rust + 2 d'intégration + 5 visuels**, 269 TypeScript, sept miroirs,
deux invariants mécanisés. Clippy strict propre.

### Prochaine tâche

Toujours la **0**.

---
