# Spec 002 — La capture bornée (N3) · design

**Statut :** approuvé, **amendée** le 2026-08-26 (D19 + verdict du spike DOM).

> Le texte de cette spec est celui de l'opérateur, découpé en triptyque sans
> reformulation. Les ajouts postérieurs sont marqués `[amendé D19]` ou
> `[amendé D20]` et ne suppriment aucune ligne d'origine.

Périmètre : capturer une occurrence de tâche réelle, bornée à la main, et en produire un épisode valide, redacté et rejouable structurellement. **Dépend de** : spec 001 verte (le schéma et le harness existent). **Nourrie par** : le verdict du spike (deux points de design marqués `[SPIKE]`, fixés en tâche 0). **Hors périmètre explicite** : N1 ambiant (spec ultérieure), connecteurs et états API (spec 003 — donc les épisodes de cette spec plafonnent au grade B, entités non résolues : c'est attendu et testé comme tel ; la boucle complète ferme en 003), NER par modèle (différé : regex + pseudonymisation couvrent la v1, les canaris surveillent, le NER arrive avant tout utilisateur externe), toute UI au-delà du menu tray.

---

### 1. Architecture
Un seul process Tauri v2. La capture vit dans un thread Rust dédié, découplée par un canal borné :

```
[UIA callbacks] → thread capture → RawEvent → Redactor (regex→HMAC) → Serializer
      ↓ (canal mpsc borné 1024)
  Writer (JSONL append, flush 5 s/100 ev, fsync à la clôture)
  Snapshotter (déclencheurs → walker canonisé ≤ 50 Ko)
```
Crates : `uiautomation` (bindings UIA), `windows` (veille, DPAPI), `tauri-plugin-global-shortcut`, `ring` (HMAC), `serde_json`, `ulid`. Horloge injectable : trait `Clock { fn now(&self) -> Instant }` (impl réelle + `FakeClock` pilotable) — les déclencheurs temporels (2 s, 10 s, 60 s, timeout 60 min) se testent en avance de temps simulée, jamais en sleep réel. Le code de traitement (redaction, sérialisation, assemblage) est indépendant d'UIA via le trait :

```rust
trait CaptureSource { fn subscribe(&self, sink: Sender<RawEvent>) -> Result<Subscription>; }
// impl UiaSource (prod)  ·  impl FakeSource (tests : rejoue des scénarios déterministes)
```
Le `FakeSource` est la clé de la testabilité : tout R1-R6 se teste sans UIA, en CI, déterministe. Seul R2 (fidélité UIA) et R7 (empreinte) exigent la machine réelle.

#### `[amendé D19]` Deux sources, une frontière nette

Le repli est **total par classe de surface**, jamais partiel par échec :

```rust
trait CaptureSource { fn subscribe(&self, sink: Sender<RawEvent>) -> Result<Subscription>; }
// impl UiaSource  : TOUTES les applications natives
// impl DomSource  : TOUTES les surfaces navigateur
//                   (extension MV3 → native messaging → app Tauri)
// impl FakeSource : tests
```

La classe de la fenêtre au premier plan décide, et rien d'autre. **Pas de bascule
dynamique UIA↔DOM sur une même surface** : une bascule conditionnelle fabrique un
système à états dont les bugs sont invisibles, là où une partition par classe se
diagnostique en regardant quelle source a parlé.

Le pipeline aval — redaction, writer, snapshotter, assemblage — **ne bouge pas
d'une ligne** : les deux sources produisent le même `RawEvent`. C'est
précisément ce que le trait était là pour absorber.

#### `[amendé D20]` Ce que le spike DOM impose au `DomSource`

Trois contraintes mesurées, pas supposées (`docs/spike-verdict-dom.md`) :

1. **Le capteur pousse, il n'accumule pas.** Un tampon dans la page ne survit
   pas à une navigation — la mesure a perdu 100 % de ses observations sur ce
   point avant qu'on le corrige. Le script de contenu émet vers le service
   worker à chaque événement.
2. **Les racines shadow se branchent par balayage**, pas par patch
   d'`attachShadow` : Salesforce réassigne `Element.prototype.attachShadow`
   après tout script d'init. Les 270 racines de Lightning sont **toutes
   ouvertes** donc énumérables ; 411 racines branchées coûtent 10,6 ms.
   Rebalayage sur mutation.
3. **`change` est `composed: false`** et ne franchit aucune frontière shadow. Un
   capteur branché sur le seul `document` ne verra jamais un changement de
   valeur — d'où le point 2, qui n'est pas une optimisation mais la condition
   d'existence de la fonction.

### 1bis. Impact inter-specs déclaré
Cette spec ÉTEND l'enum `Gap.cause` de la spec 001 : ajout de `"pause"` et `"timeout"`. Rien n'ayant shippé, l'amendement se fait directement dans `episode-spec` (même `schema_v`), avec une note datée dans docs/decisions.md. Règle générale actée : toute spec qui touche le schéma d'une spec antérieure le déclare dans une section « Impact inter-specs » — sinon les documents divergent.

`[amendé D19]` **Ajout d'un champ `source` au `RawEvent`** — valeurs closes
`"uia" | "dom" | "fake"`. Sans lui, un épisode mixte (l'opérateur passe d'Outlook
au navigateur au milieu d'une tâche) ne dit plus quelle source a produit quel
événement, et un défaut de capture devient indiagnosticable. Le champ est
descriptif : aucune règle de grade ne s'y adosse.

`[amendé D19]` **La bibliothèque de motifs de redaction devient tri-consommée** :
TypeScript (`packages/episode-spec`), Rust (`UiaSource`), et désormais JavaScript
(`DomSource`, dans la page). Elle reste déclarée en **chaînes**, jamais en
littéraux d'expression régulière — c'est ce qui permet aux trois de la lire
telle quelle. Une divergence entre les trois implémentations rendrait les
mesures incomparables et les canaris menteurs : la tâche 3 la teste sur les
trois.

`[amendé D19]` **Ce que ça ne change pas.** Le format d'épisode, le juge, le
rejeu, les grades, le harness : rien. Les deux sources convergent avant le
`Redactor`, donc tout l'aval de la spec 001 est intact.

### 2. Points fixés par le spike `[SPIKE]`
(a) **Stratégie d'abonnement** : événements globaux filtrés vs abonnement par conteneur au focus — le spike dit laquelle tient le budget CPU sur Lightning-class. (b) **Paramètres du walker** : profondeur max, budget de nœuds, propriétés lues, debounce (défaut 300 ms) — calés sur les mesures du spike. La tâche 0 inscrit ces valeurs ici même, avec le verdict en référence.

**Valeurs inscrites en tâche 0** — source : `docs/spike-verdict.md`.

**(a) Stratégie retenue : globale filtrée.** Le CPU tranche, seul.

| Stratégie | Stabilité post-pipeline | Couverture | CPU p95 (30 s) | RAM max |
| --- | --- | --- | --- | --- |
| **globale filtrée** ✅ | 34,5 % | 100 % | **3,16 %** ✅ | 22,5 Mo |
| par conteneur au focus | 47,1 % | 100 % | **8,48 %** ❌ | 22,8 Mo |

L'abonnement au focus ancre mieux (+12,6 points) mais dépasse le budget R7.1 de
70 %. Une stratégie qui chauffe n'est pas une option, si bien ancrée soit-elle :
on prend la globale filtrée.

**(b) Paramètres du walker** : profondeur max **12**, budget de nœuds **1500**,
debounce **300 ms**. Mesures de la phase focus (seule à avoir exercé le walker) :
nœuds p95 **242**, durée p95 **117 ms**.

> ⚠️ **21 % de snapshots tronqués** à 1500 nœuds. Le budget est serré pour une
> application de cette classe. Le plafond est reconduit tel quel faute d'une
> nouvelle mesure, mais la troncature est un fait connu : la tâche 7 remonte le
> plafond et remesure plutôt que de conclure sur ces 21 %.

#### `[amendé D19]` Ce que le 34,5 % ne dit PAS

Ce chiffre a été obtenu sur **Salesforce Lightning, dans un navigateur** — c'est
exactement la classe de surface que D19 retire au `UiaSource`. Après D19, UIA ne
répond plus que des **applications natives**, et sa stabilité sur ce périmètre-là
**n'est pas mesurée**.

Conséquence : les 34,5 % ne caractérisent pas le `UiaSource` tel que la spec le
définit désormais. Ils restent la raison pour laquelle le repli navigateur a été
décidé, et rien de plus. Aucun choix de conception du `UiaSource` ne doit s'y
adosser — c'est la tâche 13 qui produira le chiffre natif, sur une surface
native.

#### `[amendé D20]` Points fixés par le spike DOM — `docs/spike-verdict-dom.md`

Zone **VERT** de la grille pré-enregistrée. Les trois nombres, sur trois
exécutions indépendantes contre l'org de démo :

| | Seuil | Mesuré |
| --- | --- | --- |
| Stabilité post-pipeline de l'ancrage d'action d'état | ≥ 90 % | **100 %** |
| Couverture des actions d'état | 100 % | **100 %** |
| Surcoût CPU in-page du calcul d'ancrage | < 5 % | **0,017 – 0,021 %** |

**(c) Formule d'ancrage du `DomSource`**, dans cet ordre :

```
rôle ARIA (explicite sinon déduit) | nom accessible normalisé
                                   | chemin structurel (départage)
                                   | data-* de LISTE BLANCHE sémantique
```

Le nom accessible normalisé passe la même bibliothèque de motifs que
`normaliser_nom()` côté UIA — c'est ce qui rend les deux mondes comparables, et
toute divergence entre les deux implémentations est un bug.

**(d) Les `data-*` entrent par liste blanche, jamais en bloc.** C'est le résultat
qui commande la conception : l'enrichissement **dégrade** la stabilité, comme au
spike UIA. Le nom seul tient 100 % sur les trois exécutions ; les `data-*` en
bloc font tomber à 80 %.

| Clé | Présence | Stabilité |
| --- | --- | --- |
| `data-tab-value` | 30 % | **100 %** |
| `data-label` | 30 % | **100 %** |
| `data-aura-rendered-by` | 30 % | **81,8 %** |

`data-aura-rendered-by` est un identifiant de rendu du framework (« 931:0;a ») :
il change à chaque re-rendu et **traverse la normalisation intact**, aucun motif
de la bibliothèque ne lui ressemblant. Un attribut dont la valeur est produite
par le moteur de rendu est un poison d'ancrage, pas un renfort.

**(e) Ce que le spike n'a PAS démontré**, et qui devient donc une tâche : la
capture d'un **changement de valeur** de bout en bout. Aucun `input` ni `change`
n'a été observé pendant les répétitions mesurées, pour une raison de banc (la
saisie n'atteignait pas le champ visé) et non d'encapsulation. Rien ne permet de
l'inscrire ici comme acquis.

**(f) Deux garde-fous de méthode**, appris à nos dépens et non négociables pour
toute mesure ultérieure : un banc **se calibre avant de mesurer** (sans quoi une
répétition ratée plafonne l'intersection et affiche 9 %), et un ancrage dont
l'union descend sous le nombre de contrôles réellement actionnés **fusionne des
éléments distincts** — sa stabilité est flatteuse et irrecevable.

### 3. Ciblage et snapshots
Target = `{role, name, region}` où region = ancêtre landmark/pane le plus proche nommé. Résolution du nom : Name → LabeledBy → texte adjacent (dans cet ordre) ; échec → `unresolved:true` + compteur (R2.4). Snapshot : marche du sous-arbre du conteneur actif, sérialisation canonique (tri des attributs, exclusion des propriétés volatiles), redaction appliquée AVANT écriture, coupe au budget avec marqueur `truncated`.

### 4. Redaction (ordre exact)
`payload` et valeurs de snapshot passent : regex (email, tel, IBAN, carte — bibliothèque de patterns versionnée et testée) → chaque match remplacé par `TYPE_` + hex(HMAC(clé, valeur_normalisée))[0..8]. Normalisation avant HMAC : lowercase pour emails, chiffres seuls pour tel/IBAN — pour que deux graphies du même identifiant donnent le même token. La clé : générée à l'install, DPAPI-protected, chargée en mémoire au démarrage, jamais loggée (R4.4 testé par sweep sur logs).

### 5. Stockage
```
~/.noe/
  events/<episode_ulid>.jsonl      # flux brut redacté, append-only
  episodes/<ulid>.json             # épisode assemblé, validé, immuable
  quarantine/<ulid>.json + .err    # invalides, avec l'erreur
  meta/counters.json               # santé : unresolved, gaps, canaris
```
Panique (R5.3) : suppression par intersection de fenêtre temporelle sur les trois dossiers + recalcul des compteurs. Pas de corbeille — irréversible est une feature.

### 6. Assemblage à la clôture
Relecture du JSONL → vérification `seq` (trous → gaps) → extraction des entités CANDIDATES depuis les événements — uniquement des motifs typés sûrs (tokens EMAIL_*, URLs, identifiants au format connu), marquées `candidate:true`, jamais d'heuristique floue (la résolution réelle est le travail de la spec 003) — SANS api_refs ni états → grade (B attendu : entités non résolues) → validation `load()` harness → écriture immuable ou quarantaine.

### 7. Chargement dans le harness
`noe replay` sur un épisode 002 : verdict `hors_périmètre` propre (« aucun état API à juger ») — le harness le dit explicitement au lieu d'échouer. Test d'intégration croisé 001×002 obligatoire.

### 8. Mesure d'empreinte (R7)
Script `scripts/measure-footprint.ps1` : échantillonne CPU/RAM du process à 1 Hz pendant une capture réelle de 5 occurrences, sort p50/p95/max sur fenêtres 30 s. Résultat archivé dans `docs/footprint/<date>.json`.

### 9. CI
Runner `windows-latest` obligatoire pour ce package (build Tauri + tests). Les tests UIA réels ne tournent pas en CI (pas de session interactive) : ils vivent derrière un tag `#[ignore]` exécuté sur la machine de dev ; TOUT le reste passe par FakeSource + FakeClock en CI.

---
