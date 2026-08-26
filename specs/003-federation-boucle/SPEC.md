# Spec 003 — La fédération et la boucle fermée

> ⚠️ **SCELLÉE.** Fournie le 2026-08-26, à n'ouvrir qu'**au gate de sortie de la
> spec 002**. Elle est ici pour qu'aucune session ne s'échoue sur une spec
> manquante — pas pour être travaillée maintenant.
>
> Contenu verbatim. Le découpage en `requirements.md` / `design.md` / `tasks.md`
> se fera à l'ouverture.

---

### Connecteur CRM · Lectures avant/après · Réconciliation · Grade A

Périmètre : brancher les deux systèmes de vérité du terrain en LECTURE, résoudre les entités candidates de la spec 002, produire les états avant/après, réconcilier les deux plans, et fermer la boucle — un épisode capturé sur du travail réel qui rejoue au vert dans le harness. **C'est la spec du jalon.** **Dépend de** : 001 verte, 002 tâches 1-5 et 8 vertes (capture bornée fonctionnelle). **Doctrine d'exécution** (docs/doctrine-execution.md) : tout ce qui est exécutable l'est par l'agent — création de l'org de démo, apps OAuth, connexions de SES comptes opérationnels — sous les quatre seules exceptions (captcha, SMS vers le téléphone de l'opérateur, > 30 €/mois, juridique/live). Deux terrains distincts : le **terrain de construction** (org de démo créée et peuplée par l'agent — tout s'y développe et s'y teste) et le **terrain de preuve** (les comptes réels de l'opérateur et son vrai travail — seuls habilités à produire les chiffres du jalon). **Hors périmètre explicite** : toute ÉCRITURE vers les systèmes (la promotion/exécution est une spec ultérieure — ce connecteur est en lecture seule, structurellement : le port de cette spec n'expose pas `write`), les webhooks temps réel (polling delta en v1), les brouillons Gmail (spec Assisté), le juge sémantique.

---

## requirements.md

### Requirement 1 — Le terrain et la connexion
**User story** : en tant qu'opérateur, je connecte mes deux systèmes en quelques minutes, une fois, et je n'y pense plus.
1.1. LE CRM du terrain DOIT être fixé en tâche 0 (décision datée dans docs/decisions.md) ; le code NE DOIT JAMAIS encoder le CRM hors de son adaptateur — `terrain.json` (config) porte le choix, les scopes et les `scope_fields` par tâche.
1.2. LA connexion d'un système DOIT dérouler un OAuth PKCE via navigateur avec callback loopback local. Sur le terrain de construction, l'agent connecte ses propres comptes opérationnels de bout en bout ; sur le terrain de preuve, l'opérateur tape ses identifiants lui-même (irréductible secrets) puis l'agent reprend la main et termine. Les tokens DOIVENT être stockés via DPAPI et NE DOIVENT JAMAIS apparaître dans un fichier suivi, un log ou un export non enveloppé.
1.3. QUAND un access token expire, LE SYSTÈME DOIT le rafraîchir silencieusement ; SI le refresh échoue définitivement, ALORS l'état du connecteur passe à `reauth_required`, visible au tray, sans crash ni perte d'épisode (les lectures manquées deviennent des trous classés).

### Requirement 2 — La résolution des entités
**User story** : en tant que corpus, mes entités pointent vers de vrais enregistrements, ou disent honnêtement qu'elles ne savent pas.
2.1. QUAND un épisode contient des entités candidates (spec 002), LE SYSTÈME DOIT tenter leur résolution en `api_refs` par clés fortes uniquement : identifiant système exact, email exact (comparé en tokens HMAC des deux côtés — voir R6.2), domaine + nom exact.
2.2. SI la résolution est ambiguë (0 ou ≥ 2 candidats), ALORS l'entité DOIT rester non résolue avec la raison précise (`not_found` | `ambiguous:n`) — LE SYSTÈME NE DOIT JAMAIS deviner.
2.3. QUAND une entité est résolue, LE SYSTÈME DOIT enregistrer `resolved:{by, at}` (la clé qui a tranché, l'horodatage).

### Requirement 3 — Les lectures avant/après
**User story** : en tant que juge, je dispose d'un état de référence dont je connais la provenance et les limites.
3.1. QUAND une entité est résolue PENDANT un épisode ouvert, LE SYSTÈME DOIT lire immédiatement son état (`state_before`), restreint aux `scope_fields` de la tâche plus les champs observés changés.
3.2. QUAND l'épisode se clôt, LE SYSTÈME DOIT relire chaque entité résolue (`state_after`).
3.3. SI l'historique du système montre une écriture sur l'entité ANTÉRIEURE à la première lecture (dans la fenêtre de l'épisode), ALORS LE SYSTÈME DOIT reconstituer le `state_before` via cet historique et marquer l'état `reconstituted:true` ; SI la reconstitution est impossible (champ non historisé), ALORS le champ est marqué `unknown_before` et exclu du verdict avec raison — jamais silencieusement compté.
3.4. LES états DOIVENT être des objets plats champ→valeur normalisés (mêmes règles que le juge, spec 001 §5) et passés au pipeline de redaction AVANT persistance (R6).

### Requirement 4 — La réconciliation et le bilan de complétude
**User story** : en tant que système, je me vérifie moi-même : chaque changement du monde est expliqué, hors périmètre, ou déclaré trou.
4.1. PENDANT un épisode (+ marge de 60 s après clôture), LE SYSTÈME DOIT collecter les changements API des entités résolues (polling delta) et joindre chaque changement aux événements UI par fenêtre de 30 s + même entité.
4.2. CHAQUE changement API DOIT finir dans exactement une colonne : `expliqué` (joint à une action UI), `hors_périmètre` (champ hors scope, ou acteur ≠ opérateur quand le système l'expose), `trou` (aucune action UI jointe, dans une fenêtre de gap ou pas).
4.3. LE bilan DOIT être écrit dans `episode.completeness` et agrégé par jour ; LE taux d'expliqué agrégé est LA métrique de santé, exposée par `noe health`.
4.4. LES changements API survenant HORS de tout épisode NE DOIVENT PAS être collectés en v1 (périmètre N3 strict — l'ambiant API viendra avec le N1).

### Requirement 5 — La robustesse
**User story** : en tant que process, une API distante en colère ne me fait ni crasher ni mentir.
5.1. TOUTE requête DOIT passer par le client commun : backoff exponentiel + jitter sur 429/5xx (plafond 5 tentatives), respect des en-têtes Retry-After, refresh sur 401, timeout par requête.
5.2. CHAQUE erreur définitive DOIT être classée : `retryable_exhausted` → trou avec cause ; `permission` → hors_périmètre avec raison ; `not_found` → résolution échouée — un connecteur NE DOIT JAMAIS faire crasher le process ni bloquer la clôture d'un épisode (les lectures manquantes déclassent le grade, elles n'empêchent rien).
5.3. LE SYSTÈME DOIT respecter un budget d'appels par épisode (config, défaut 30) et le journaliser ; dépassement → arrêt des lectures + trou déclaré, jamais de tempête de requêtes.

### Requirement 6 — La confidentialité de la fédération
**User story** : en tant qu'utilisateur, brancher mes systèmes n'élargit pas ce qui touche mon disque en clair.
6.1. LES états et payloads fédérés DOIVENT passer le MÊME pipeline de redaction que la capture (regex → HMAC) avant persistance ; les canaris sont étendus : un canari planté dans un champ du CRM de démo NE DOIT JAMAIS apparaître en clair dans un épisode, un log ou un rapport.
6.2. POUR comparer sans exposer, LES valeurs d'identification lues des APIs (emails…) DOIVENT être tokenisées à la volée et comparées en tokens — la valeur claire ne DOIT vivre qu'en mémoire, jamais persistée.
6.3. LES corps de messages (Gmail) NE DOIVENT PAS être persistés dans cette spec : métadonnées et en-têtes seulement (le contexte pour brouillons arrive avec la spec Assisté, avec ses propres règles).

### Requirement 7 — Le grade A et la boucle fermée (le jalon)
**User story** : en tant que projet, je sais enfin si ma thèse tient : un épisode réel rejoue et se juge.
7.1. QUAND un épisode clôturé a toutes ses entités résolues avec `state_before`/`state_after` présents, séquence sans trou et redaction validée, LE SYSTÈME DOIT le régrader A (règles 001 R2.1 inchangées, recalculées).
7.2. QUAND `noe replay` tourne sur un épisode A réel avec la `perfectPolicy`, LE VERDICT DOIT être `accord` — c'est le test d'auto-cohérence bout en bout (capture + fédération + juge alignés).
7.3. LE JALON de la spec : sur une journée réelle de la tâche de campagne, réalisée par l'opérateur sur son terrain de preuve — ≥ 5 épisodes clôturés dont ≥ 3 de grade A, bilan de complétude agrégé ≥ 85 % d'expliqué, zéro canari, et le rejeu des A au vert. **Des occurrences scriptées (Playwright ou autre) NE PEUVENT PAS produire le jalon** : elles mesureraient l'agent, pas le travail — c'est l'irréductible « les gestes sont la donnée », qui s'applique au corpus même quand il ne s'applique plus aux bancs d'essai. Chiffres archivés dans `docs/milestones/boucle-fermee.md`.

---

## design.md

### 1. Le port (lecture seule, structurellement)

```ts
// packages/core/src/ports/connector.ts
export interface ReadConnector {
  readonly id: string;                                      // "salesforce" | "hubspot" | "gmail" | "fake"
  resolve(candidate: EntityCandidate): Promise<Resolution>; // R2 — clés fortes seulement
  read(ref: ApiRef, fields: string[]): Promise<FlatState>;  // R3 — normalisé, non persisté tel quel
  changes(ref: ApiRef, since: string): Promise<ApiChange[]>;// R4 — polling delta
  history(ref: ApiRef, field: string, window: TimeWindow): Promise<HistoryPoint[]>; // R3.3 — corroboration
}
```
Pas de `write` dans le type : l'écriture n'existe pas dans cette spec, le compilateur l'interdit. `FakeConnector` (scénarios déterministes : résolution ambiguë, 429 en rafale, write-avant-lecture, champ non historisé) rend R1-R7 testables en CI — même pattern que FakeSource/FakeClock.

### 2. Configuration du terrain
`~/.noe/terrain.json` : `{ crm: "…", tasks: { "<task_slug>": { scope_fields: [...], objects: [...] } }, budgets: { reads_per_episode: 30 } }`. Fixé en tâche 0. L'adaptateur est choisi par ce fichier — le domaine ne connaît que le port.

### 3. OAuth et tokens
PKCE via navigateur système + callback `http://127.0.0.1:<port>/cb` (listener éphémère Tauri). Tokens → DPAPI (même mécanique que la clé HMAC, 002). Refresh proactif (marge 5 min) dans le client commun. État `reauth_required` remonté au tray (3e état « question en attente » réutilisé).

### 4. Le flux avant/après (algorithme)
```
entité candidate détectée (002) → resolve() [async, hors chemin de capture]
  résolue → read(scope_fields ∪ champs_observés) → normalisation → redaction → state_before (+ ts_read)
clôture → pour chaque résolue : read() → state_after
  puis history(fenêtre [t0, ts_read]) par champ du diff :
    écriture antérieure à ts_read détectée → reconstitution → reconstituted:true
    champ non historisé et suspicion d'écriture → unknown_before, exclu du verdict avec raison
réconciliation → completeness → regrade → load() harness → immuable
```
La résolution et les lectures tournent dans un worker séparé du thread de capture (jamais de latence de capture due au réseau) ; si une lecture n'est pas revenue à la clôture + 60 s, elle est classée trou (R5.2) et la clôture n'attend pas.

### 5. Notes d'adaptateurs (les deux probables + Gmail)
**Salesforce** : lectures directes REST (`/sobjects/{o}/{id}?fields=`) = source primaire ; `updated-since` par objet pour le delta ; Field History = corroboration SEULEMENT — limites vérifiées : 20 champs/objet suivis, longs textes sans valeurs, horodatage non garanti (document de référence §06). `unknown_before` sera fréquent sur les champs non suivis : c'est prévu, pas un bug.
**HubSpot** : `propertiesWithHistory` sur les objets CRM v3 donne l'historique par propriété — la reconstitution y est plus riche ; delta via `hs_lastmodifieddate`.
**Gmail** : `users.history.list` pour le delta, `threads.get(format=metadata)` pour en-têtes/labels — jamais de corps (R6.3). Sert : résolution par email (tokens), bornes de threads, futur signal d'envoi.

### 6. Réconciliation
Index des actions UI par (entité, ts) ; pour chaque ApiChange : cherche action UI |Δts| ≤ 30 s sur la même entité → expliqué ; sinon champ ∉ scope → hors_périmètre ; sinon → trou (avec sous-cause : dans un gap déclaré / hors gap = le vrai signal d'alarme). Compteurs dans `completeness` + `meta/counters.json`.

### 7. Impact inter-specs (déclaré)
`episode-spec` s'enrichit (même schema_v, decisions.md) : `Entity.resolved:{by,at}?`, `FlatState` valeurs annotables `{v, reconstituted?, unknown_before?}` — représenté par un champ parallèle `state_meta` pour garder FlatState plat (le juge lit state, les exclusions lisent state_meta). Le juge (001) apprend une règle : un champ `unknown_before` est retiré du périmètre de CE verdict avec trace au rapport.

### 8. CI
Tout en FakeConnector sur runner standard. Les tests contre le CRM de démo réel : tag `integration`, exécutés sur la machine de dev + avant chaque release (pas à chaque push — quotas).

---

## tasks.md

- [ ] **0. Les terrains, fixés — en autonomie** — (a) Terrain de construction : l'agent crée l'org de démo (Salesforce Developer Edition ou équivalent du CRM retenu) avec son identité opérationnelle, la peuple de fiches réalistes + canaris, crée les apps OAuth (CRM + Google, mode test, domaine neutre) par Playwright, journalise dans decisions.md et docs/evidence/. (b) Terrain de preuve : le CRM réel de la campagne de l'opérateur, fixé par décision datée de l'opérateur au plus tard avant la tâche 12 — l'agent la lui demande en une ligne dès l'ouverture de cette spec, puis n'attend pas dessus. `terrain.json` porte les deux, avec les scope_fields de la tâche de campagne. _Req : 1.1._
- [ ] **1. Port + FakeConnector** — interfaces du design §1, FakeConnector avec les 4 scénarios adverses, tests. _Req : socle._
- [ ] **2. OAuth PKCE + tokens DPAPI** — flow complet navigateur système + loopback, stockage, refresh proactif, état `reauth_required` au tray ; test : token corrompu → reauth propre sans perte. _Req : 1.2, 1.3._
- [ ] **3. Client commun robuste** — backoff+jitter, Retry-After, 401→refresh, timeouts, classification des erreurs, budget d'appels par épisode journalisé ; tests FakeConnector : rafale de 429, panne 5xx, budget dépassé. _Req : 5.1, 5.2, 5.3._
- [ ] **4. Adaptateur CRM (lecture)** — resolve/read/changes/history sur le CRM fixé, notes du design §5 appliquées ; tests d'intégration tagués sur l'org de démo (celle que l'agent a créée en tâche 0). _Req : 2.1, 3.1, 3.2._
- [ ] **5. Résolution des candidates** — clés fortes, comparaison en tokens (R6.2), `resolved:{by,at}`, ambiguïtés honnêtes ; tests : 1 candidat, 0, 2, email en deux graphies → même token → résolu. _Req : 2.1, 2.2, 2.3._
- [ ] **6. Lectures avant/après + worker** — hook first-seen (002→003), worker hors chemin de capture, clôture qui n'attend jamais plus de 60 s, états normalisés + redactés. _Req : 3.1, 3.2, 3.4._
- [ ] **7. Reconstitution par historique** — détection d'écriture antérieure, reconstitution, `reconstituted`/`unknown_before` + règle du juge (retrait du périmètre avec trace). _Req : 3.3 + impact §7._
- [ ] **8. Réconciliation + bilan** — jointure 30 s + entité, trois colonnes avec sous-causes, `completeness` écrit, `noe health` agrégé. _Req : 4.1, 4.2, 4.3, 4.4._
- [ ] **9. Redaction fédérée + canaris étendus** — même pipeline sur états/payloads API, canaris plantés dans le CRM de démo, sweep étendu vert. _Req : 6.1, 6.2._
- [ ] **10. Regrade A** — recalcul des grades à la clôture avec les règles 001 inchangées, raisons journalisées, quarantaine pour les incohérences. _Req : 7.1._
- [ ] **11. Adaptateur Gmail minimal** — history delta + threads metadata (jamais de corps), résolution par email en tokens. _Req : 6.3 + 2.1._
- [ ] **12. Auto-cohérence bout en bout** — un épisode RÉEL capturé (002, terrain de preuve : le travail de l'opérateur, jamais un script) traverse résolution→lectures→réconciliation→regrade→`noe replay` avec perfectPolicy → accord. Prérequis : la décision terrain de preuve (tâche 0b) est rendue et les comptes réels connectés (l'opérateur tape ses identifiants, l'agent finit). _Req : 7.2._
- [ ] **13. LE JALON** — une journée réelle de campagne de l'opérateur (non scriptable, cf. 7.3) : ≥ 5 épisodes, ≥ 3 grade A, complétude ≥ 85 %, zéro canari, rejeu des A au vert, chiffres archivés dans `docs/milestones/boucle-fermee.md`. **La boucle est fermée — on écrit la spec 004 (politique & shadow) avec ces épisodes réels comme matière.**
