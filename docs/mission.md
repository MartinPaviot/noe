# Noe — Prompt de mission (long-run autonome)

> Ce document remplace les briefs de session. Il est relu au début de **chaque**
> session. Il donne tout : les arbitrages en attente, la méthode, la feuille de
> route complète des specs, le standard de tests visuels, et le protocole
> d'autonomie longue durée.
>
> On tourne dessus jusqu'au **mur du viable**, sans solliciter l'opérateur hors
> des quatre exceptions.

---

## 1. Arbitrages rendus

Consignés dans `docs/decisions.md` — D19, D20, D21.

**D19 — Le repli est TOTAL par classe de surface, pas partiel par échec.**
L'extension Chrome (MV3) devient l'adaptateur de capture de **toutes** les
surfaces navigateur ; UIA garde **toutes** les applications natives. Pas de
bascule dynamique UIA↔DOM sur la même surface : deux mondes, deux adaptateurs,
une frontière nette — la classe de la fenêtre au premier plan.

*Raison* : la bascule conditionnelle crée un système à états dont les bugs sont
invisibles ; la partition par classe est diagnosticable.

Le trait `CaptureSource` absorbe le changement : `UiaSource` (natif) et
`DomSource` (extension → native messaging vers l'app Tauri) implémentent la même
interface. Le pipeline aval — redaction, writer, assemblage — ne bouge pas d'une
ligne.

**D20 — On re-mesure AVANT de construire, grille pré-enregistrée.**
Spike DOM d'une journée max, même protocole que le spike UIA : 5 occurrences
scriptées identiques, org de démo, normalisation post-pipeline. Ancrages :
`data-*`, rôles ARIA explicites, chemin structurel, nom.

| Stabilité post-pipeline | Décision |
| --- | --- |
| **≥ 90 %** | vert — amende la spec 002 (D19) et déroule |
| **60-89 %** | on construit, ciblage *best-effort*, chaîne de repli en régime normal |
| **< 60 %** | **on construit quand même** : film best-effort assumé, clés de branches sur les **transitions de champs** (plan API), ciblage UI corroboratif, note de portée au jalon |

*Raison* : la preuve vit sur le plan API. **Aucune valeur de stabilité du film ne
bloque la mission.** Le spike sert à savoir sur quoi on roule, jamais à s'arrêter.

**D21 — Tests visuels Playwright obligatoires, partout, pour toujours.**
Standard au §4. Toute tâche qui produit ou modifie des pixels livre ses tests
visuels **dans la même tâche**. Une tâche UI sans test visuel n'est pas
terminable.

## 2. Méthode permanente (Kiro, auto-portée)

Une spec = un triptyque `requirements.md` (EARS) / `design.md` / `tasks.md` dans
`specs/NNN-slug/`. Une tâche = un incrément vérifié de bout en bout ; cocher = le
critère passe.

**Les specs 004, 005 et 006 ont été fournies par l'opérateur** le 2026-08-26 et
déposées sans reformulation. Elles portent déjà leur challenge « trois scènes
réelles » intégré : le rituel ci-dessous ne s'applique donc qu'aux specs que
j'écris moi-même, c'est-à-dire **007 à 010**, au gate de sortie de la précédente,
même format et même profondeur. Chaque spec auto-écrite passe le rituel avant
d'être travaillée :

1. **auto-challenge « trois scènes réelles »** — projeter trois situations
   concrètes d'usage et chercher ce qui casse ; consigner les trouvailles et
   corriger ;
2. **section « Impact inter-specs »** si elle touche un schéma ou une règle
   antérieure ;
3. **hors-périmètre explicite**.

L'opérateur relit en asynchrone ; je n'attends pas sa relecture pour commencer.

Toute dérogation à une spec ou à l'ordre = entrée datée dans `decisions.md`
**avant** le code. Un DoD inatteignable se durcit ou s'arbitre dans
`decisions.md` — jamais ne se contourne.

CI verte = condition de fin de session. Un commit par vert. `progress.md` tenu
par session.

## 3. Feuille de route des specs

| # | Spec | Contenu | Gate de sortie |
| --- | --- | --- | --- |
| 001 | Socle de preuve | ✅ verte | — |
| 002 | Capture bornée | **Amendée par D19** : `DomSource` (extension MV3 + native messaging) pour le navigateur, `UiaSource` pour le natif ; R1-R7 valent pour les deux sources ; tâche 6 dédoublée (6a natif, 6b web) ; spike DOM (D20) = tâche 0bis | une occurrence réelle sur l'org de démo → épisode valide grade B, zéro canari, budgets tenus |
| 003 | Fédération & boucle | scellée, à jour dans le dépôt | **LE JALON technique** : ≥ 5 épisodes chaîne réelle, ≥ 3 grade A, complétude ≥ 85 %, rejeu vert → `docs/milestones/boucle-fermee.md` |
| 004 | Politique & shadow | prompt + règles + few-shot par clés, hash de politique, shadow à la clôture, file bornée et priorisée par gain, cascade modèle Azure avec coûts journalisés, **canaris d'injection** (D6 échue ici) | sur ≥ 10 épisodes : file produite, une règle ajoutée change un verdict au re-rejeu, accord par branche affiché |
| 005 | Modes & promotion | classement exhaustif par mode avec justification, permission/gardes/refus→règle, rodage 5 exécutions, écritures sûres (idempotence, verrouillage optimiste, before-images, annuler), témoin 15 %, SPRT calibré avec simulation jointe ; **le port connecteur gagne `write` ICI seulement** | une branche promue sur l'org de démo, exécution autonome au diff vert, annulée, rejouée, journal complet |
| 006 | Assisté | drafts Gmail dans le thread, zéro envoi auto, apprentissage silencieux (diff proposé/envoyé), métriques | 5 brouillons réels créés, diffs récoltés |
| 007 | N1 & revue | capture ambiante métadonnées (liste blanche vide), carte du temps, motifs, bilan d'automatisabilité, `noe week` | carte + bilan générés sur 3 jours de données |
| 008 | UI produit — **finir**, plus construire (D26) | onboarding 3 écrans + diagnostic de stack, revue/bilan, file en cartes, bibliothèque + graphe React Flow, permission, états vide/erreur/chargement partout, pause/panique écran 1 | parcours souris complet installation→bilan, plus « jour 1 vide » et « OAuth refusé » propres — **tout sous tests visuels** |
| 009 | Commercial | licence ed25519 + grâce 72 h, Stripe TEST bout en bout, backend `noe-prod` réactivé (lint anti-contenu), installeur signé + auto-update, landing + page trust | parcours acheteur test complet + coupure réseau 72 h simulée. **Stripe LIVE = exception opérateur** |
| 010 | Durcissement | machine vierge, SmartScreen, Sentry + PostHog opt-in, funnel, support in-app, docs publiques, revue sécurité, bêtas | **le MUR DU VIABLE** vrai ligne à ligne — la signature est à l'opérateur |

L'ordre est un ordre de **dépendances**. À l'intérieur d'une spec, je réordonne
librement ce que le graphe permet, en le consignant.

### `[D26]` Le squelette traversant — l'UI ne commence plus en 008

Une **fenêtre Tauri d'une seule vue** naît à la fin de la **tâche 8 de la spec
002** et grandit ensuite à chaque spec. Elle liste les épisodes réels du poste
avec leur **grade**, leur **complétude** et leur **timeline d'événements**,
branchée sur les vraies données, sous tests visuels dès le premier écran.

| Spec | Ce qu'elle ajoute au squelette |
| --- | --- |
| **002** (tâche 8bis) | liste des épisodes · grade et sa raison · complétude · timeline des événements et des trous |
| 003 | états d'entité avant/après, et le verdict du juge par branche |
| 004 | la file priorisée, l'accord par branche, le coût de la politique |
| 005 | les branches promues, leur mode, leur rodage, le bouton d'annulation |
| 006 | les brouillons proposés et le diff avec ce qui a été envoyé |
| 007 | la carte du temps et le bilan d'automatisabilité |
| 008 | onboarding, permission, bibliothèque, graphe — **le reste**, pas le début |

**Ce n'est pas de l'UI en avance de phase.** Chaque ajout affiche une donnée que
la spec vient de produire ; il n'y a rien à inventer, seulement à montrer.

**Ce que ça change au périmètre de la 002.** Elle gagne une tâche, **8bis**, et
son gate reste inchangé — le squelette accompagne l'épisode, il ne le remplace
pas.

## 4. Tests visuels Playwright — le standard (D21)

**Harness** : `@playwright/test` + `toHaveScreenshot`, baselines **commitées**
dans `tests/visual/__screenshots__/`, `maxDiffPixelRatio: 0.01`, viewport fixe
**1280×800**, animations désactivées, fontes embarquées — le déterminisme des
pixels n'est pas négociable.

**Cible** : l'UI React tourne sous Vite en mode test avec la couche IPC Tauri
**mockée** (fixtures d'épisodes et de branches réalistes, versionnées). Les tests
visuels ne dépendent ni de la capture ni du réseau. L'extension : popup et pages
testées pareil. La landing : idem sur le build statique.

**Couverture exigée par surface** — une surface = **au minimum 4 baselines** :

1. état nominal **avec données** ;
2. état **vide** (jour 1) ;
3. état d'**erreur** ;
4. état de **chargement**.

`pnpm test:visual` intégré à la CI. Le runner Linux suffit pour l'UI web ; le
rendu Tauri natif est vérifié en session de dev, pas en CI.

**Un diff visuel = rouge = la tâche n'est pas finie.** Une évolution voulue
régénère la baseline dans le **même commit** que le changement, jamais
séparément.

En plus des tests : chaque jalon et chaque opération Playwright sur un portail
externe archive ses captures dans `docs/evidence/<date>-<sujet>/`. Les tests
prouvent la non-régression ; l'evidence documente l'histoire.

### `[D26]` L'evidence quotidienne — voir, pas lire

**À la clôture de chaque session, et au minimum une fois par jour calendaire où
des commits sont poussés**, une capture de l'état courant atterrit dans
`docs/evidence/daily/`, nommée `AAAA-MM-JJ-<sujet>.png`.

La règle existe parce que huit specs de tests verts ne montrent rien. Un
fondateur qui ne voit pas ce qui se construit ne peut corriger le tir qu'au
moment où c'est le plus cher.

Trois conditions, sans quoi la règle mourrait d'elle-même :

1. **Produite par un script**, jamais à la main — une preuve visuelle qui dépend
   de la discipline de quelqu'un cesse d'exister en trois semaines.
2. **En session de développement**, pas en CI : le rendu natif exige un
   affichage.
3. **Ce qui existe vraiment.** Tant que le squelette n'est pas né, la capture
   montre les trois états de l'icône de barre d'état et l'état de démarrage.
   C'est peu, et c'est mieux qu'une case cochée sans image.

## 5. Protocole de long-run

**Boucle de session** : relire `mission.md` + les `decisions.md` récentes + le
`tasks.md` de la spec ouverte → smoke (`pnpm verify`) → une tâche → vérification
bout en bout → commit → `progress.md` → tâche suivante. J'enchaîne tant que le
contexte le permet ; je clos proprement — commit + note de reprise **précise**
dans `progress.md` — quand il s'épuise.

**Les quatre exceptions**, seules sollicitations autorisées :

1. captcha ou mur anti-bot infranchissable après **3 tentatives** ;
2. vérification exigeant le **téléphone** de l'opérateur ;
3. dépense **> 30 €/mois** cumulés ;
4. engagement **juridique ou live** — Stripe live, signature du mur du viable.

Format : une ligne actionnable, l'item mis en attente, et je continue ailleurs.
**Je ne m'immobilise que si TOUT est bloqué.**

**Anti-échouage** : un CLI qui échoue se configure ; un test *flaky* se répare ou
se met en quarantaine **avec une issue**, jamais ne se mute en `skip` silencieux ;
une ambiguïté de spec se tranche par la **lecture la plus stricte** + note dans
`decisions.md`.

**Budget** : coûts journalisés dans `decisions.md`, alerte à **25 €/mois**
cumulés. Azure = crédits : surveiller le facturé réel contre le nominal.

**Rapport** : fin de session = 5 lignes dans `progress.md` — fait / vert /
prochaine tâche / en attente / coûts. Chaque gate de spec = un rapport de jalon
court dans `docs/milestones/`.

**Sécurité constante** : canaris à chaque build, `gitleaks`, jamais un secret
hors coffre ou `.env` ; l'exposition inévitable se **déclare** (jurisprudence de
l'org de démo, D13).

## 6. Démarrage

1. `decisions.md` : D19, D20, D21.
2. **Spike DOM** (D20) : construire, mesurer, appliquer la grille, consigner dans
   `docs/spike-verdict-dom.md`. Quel que soit le résultat, **la mission continue
   le jour même**.
3. Amender la spec 002 (D19 + verdict DOM) : `DomSource`/`UiaSource`, tâche 0bis,
   tâche 6 dédoublée, section Impact inter-specs.
4. Dérouler la 002 jusqu'à son gate, puis ouvrir la 003 scellée, puis auto-écrire
   la 004 et les suivantes au rythme des gates.
5. À chaque spec avec des pixels : le standard visuel §4 s'applique dès le
   premier écran.

---

**La mission s'arrête à un seul endroit : le mur du viable, dont la signature est
à l'opérateur. Tout le reste est à moi.**
