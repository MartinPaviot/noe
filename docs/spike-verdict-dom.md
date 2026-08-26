# Verdict du spike — capteur DOM (D20)

> **Date :** 2026-08-26 · **Application cible :** Salesforce Lightning, org de démo
> **Protocole :** 5 répétitions scriptées identiques, normalisation post-pipeline
> identique à celle du binaire Rust, stabilité = |∩| ÷ |∪| sur les actions d'état résolues.
>
> ⚠️ **Occurrences scriptées Playwright — banc capteur, pas donnée
> comportementale.** Ces chiffres mesurent le capteur face à une application, pas
> la façon dont un humain travaille. Mention obligatoire, `docs/decisions.md` D11.

## 1. Question tranchée

D20 demandait trois nombres sur les ancrages navigateur — `data-*`, rôles ARIA
explicites, chemin structurel, nom — pour décider si l'on construit
l'adaptateur `DomSource` de la spec 002, et sur quoi.

## 2. Les trois nombres

| | Seuil | Mesuré | |
| --- | --- | --- | --- |
| **Stabilité post-pipeline** (ancrage `rôle \| nom normalisé`) | ≥ 90 % | **100 %** sur trois exécutions | ✅ |
| **Couverture** des actions d'état | 100 % | **100 %** sur trois exécutions | ✅ |
| **Surcoût CPU in-page** | < 5 % | **0,017 – 0,021 %** | ✅ |

Détail par formule d'ancrage, trois exécutions indépendantes :

| Formule | parcours (union 4) | large A (union 10) | large B (union 10) |
| --- | --- | --- | --- |
| `rôle \| nom brut` | 100 % | 100 % | 100 % |
| **`rôle \| nom normalisé`** | **100 %** | **100 %** | **100 %** |
| `rôle \| chemin structurel` | 100 % | 100 % | 100 % |
| `rôle \| data-*` (tous) | 100 % | 100 % | **80 %** |
| `rôle \| nom \| data-* \| chemin` | 100 % | 100 % | **81,8 %** |
| `rôle \| data-testid` | *dégénéré* | *dégénéré* | *dégénéré* |

**Zone de la grille D20 : VERT (≥ 90 %).** → *amende la spec 002 (D19) et déroule.*

## 3. Le résultat qui commande la conception

**L'enrichissement dégrade la stabilité — exactement comme au spike UIA.**

Le nom accessible normalisé tient 100 % sur les trois exécutions. Dès qu'on y
ajoute les `data-*` en bloc, on tombe à 80 %. L'analyse clé par clé désigne le
responsable sans ambiguïté :

| Clé `data-*` | Présence | Stabilité |
| --- | --- | --- |
| `data-tab-value` | 30 % | **100 %** |
| `data-label` | 30 % | **100 %** |
| `data-aura-rendered-by` | 30 % | **81,8 %** |

`data-aura-rendered-by` est un identifiant de rendu du framework (« 931:0;a ») :
il change quand Aura re-rend, et il traverse la normalisation partagée **intact**
— aucun motif de la bibliothèque ne lui ressemble. Le banc témoin l'avait prédit
avant la mesure, et la mesure l'a confirmé.

> **Règle pour la spec 002** : les `data-*` entrent dans l'ancrage par **liste
> blanche sémantique**, jamais en bloc. Un attribut dont la valeur est produite
> par le moteur de rendu est un poison d'ancrage, pas un renfort.

## 4. Ce que le spike a corrigé de mes propres affirmations

Trois choses que j'ai avancées en cours de route et que la mesure a démenties.
Elles sont ici parce qu'un verdict qui ne garde que ce qui l'arrange ne vaut rien.

1. **« Les racines shadow fermées empêchent la capture. » — Faux.** Le
   diagnostic d'encapsulation rend `270 éléments personnalisés, 270 racines
   ouvertes, 0 sans racine accessible`. Toutes les racines de Lightning sont
   **ouvertes** et énumérables. J'avais bâti cette hypothèse sur l'absence
   d'événements, sans la vérifier.

2. **« Le patch d'`attachShadow` suffit. » — Faux.** Salesforce réassigne
   `Element.prototype.attachShadow` après notre script d'init (`patchTient:
   false`). La parade qui marche est le **balayage des racines ouvertes** :
   411 racines branchées en **10,6 ms**, coût négligeable.

3. **« Le témoin valide le capteur. » — Il l'a d'abord invalidé en silence.** Ma
   première page témoin fabriquait un `change` avec `composed: true`, ce qu'un
   navigateur ne fait jamais. Le test passait au vert sur un capteur aveugle aux
   changements de valeur. Depuis, le banc n'émet plus que des événements natifs,
   via les locators Playwright.

## 5. Ce que ce spike n'affirme pas

- **Les événements de valeur (`input`, `change`) n'ont pas été observés** pendant
  les répétitions mesurées. La cause n'est PAS l'encapsulation — voir §4.1. La
  saisie du banc n'atteignait pas le champ visé (la valeur du `textarea` ne
  contenait pas le texte tapé). **La capture des changements de valeur reste donc
  à démontrer de bout en bout**, et c'est une tâche de la spec 002, pas un acquis.
  Le fait de spécification demeure : `change` est `composed: false` et ne franchit
  aucune frontière shadow — un capteur branché sur le seul `document` ne le verra
  jamais.
- **Un tampon in-page ne survit pas à une navigation.** La première phase large a
  perdu 100 % de ses observations pour cette raison. Le capteur doit **pousser au
  fil de l'eau** ; c'est aussi ce que fera l'extension vers son service worker.
- **Le banc exige une passe de calibrage.** Sans elle, une répétition ratée
  (1 contrôle atteint sur 24) plafonnait l'intersection et affichait 9 %. Deux
  exécutions du même protocole non calibré ont rendu 76,9 % puis 7,7 % : c'était
  la dérive du banc, pas celle des ancrages.
- **Seulement 50 % des éléments actionnés portent un rôle ARIA explicite.**
  L'autre moitié est déduite de la balise.
- **Aucun `data-testid` sur toute la surface** (0 %). La formule correspondante
  est marquée *dégénérée* : réduite à `rôle|`, elle range tous les boutons dans
  le même seau et affiche une stabilité flatteuse qui ne distingue rien. Le
  garde-fou de dégénérescence est mécanique, pas déclaratif.
- Une seule org, une seule fiche, un seul jour. Rien sur la résistance à une mise
  à jour de Salesforce. Unions de 4 à 10 signatures : petites.

## 6. Conséquences pour la spec 002

- [ ] `DomSource` : ancrage = `rôle | nom accessible normalisé`, **plus** chemin
      structurel en départage, **plus** `data-*` par liste blanche sémantique.
- [ ] Le capteur **pousse** chaque observation ; aucun tampon qui doive survivre
      à une navigation.
- [ ] Branchement des racines shadow **par balayage**, pas par patch
      d'`attachShadow` ; rebalayage sur mutation.
- [ ] Tâche dédiée : démontrer la capture d'un changement de valeur de bout en
      bout — c'est le trou laissé par ce spike.
- [ ] Budget CPU : le poste réel n'est pas le calcul d'ancrage (0,02 %) mais le
      balayage et le transport. À mesurer là où ça coûte.

---

**Grille D20 appliquée sans redemander : VERT. La spec 002 est amendée et la
mission continue le jour même.**
