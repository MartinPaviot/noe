# Spec 003 — La fédération et la boucle fermée · design

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
