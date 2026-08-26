# Spec 005 — Les modes, la promotion et les ecritures sures · requirements

**Statut :** fourni par l'opérateur le 2026-08-26, déposé **sans reformulation**.

> Le texte de cette spec est celui de l'opérateur, découpé en triptyque. Les
> ajouts postérieurs porteront un marqueur `[amendé Dn]` et ne supprimeront
> aucune ligne d'origine.
>
> **Numérotation.** Les mentions de « D22 » dans ce texte désignent l'arbitrage
> du **squelette traversant**, enregistré sous le numéro **D26** dans
> `docs/decisions.md` — D22 y désignait déjà la reclassification du 34,5 %
> d'UIA. Le texte n'a pas été modifié ; c'est la note qui lève l'ambiguïté.

Périmètre : la promesse centrale — une branche observée devient une branche exécutée, sous permission, avec preuve et retour arrière. Le port connecteur gagne `write` ICI et nulle part avant. **Dépend de** : 004 verte. **Hors périmètre explicite** : brouillons (006), UI produit complète (008 — seule la vue squelette s'étend), tout compte utilisateur réel (org de démo uniquement ; la table de réversibilité est ratifiable en asynchrone par l'opérateur mais la démo tourne sur ratification-agent marquée `demo_only` — AUCUNE écriture vers un compte réel n'est possible avant ratification opérateur, et c'est structurel : l'adaptateur réel n'existe pas encore).
Challenge « trois scènes réelles » intégré : (1) l'humain modifie la fiche pendant que l'agent exécute → verrouillage optimiste : relecture pré-écriture, tout drift → escalade, jamais d'écrasement ; (2) le process meurt entre l'écriture et le journal → l'idempotency_key est posée AVANT l'appel, la reprise relit l'état distant et réconcilie (at-least-once côté journal, at-most-once côté effet) ; (3) une branche promue à 40 occurrences dérive lentement (accord 95→85 % sur un mois) → le SPRT est calibré pour la dérive lente, pas seulement la panne franche, et la simulation jointe le prouve sur ce scénario précis.

---

---

### Requirement 1 — Le classement exhaustif par mode
1.1. CHAQUE tâche observée DOIT recevoir exactement un mode (Autonome / Copilote / Assisté / Documenté) avec justification par critère (vérifiabilité API, fréquence, concentration des branches, réversibilité) et le chemin chiffré vers le mode supérieur (« il manque n occurrences / x points d'accord »).
1.2. LE classement DOIT être recalculé à chaque clôture d'épisode et exposé par `noe modes` (JSON + texte) — c'est la donnée du futur bilan.

### Requirement 2 — L'éligibilité et la permission
2.1. LE bouton production d'une branche NE DOIT être actif que si : n ≥ 40 (grade A), accord ≥ 95 % au hash de politique courant, TOUTES les actions de la branche `reversible` dans la table ratifiée, permission accordée. Aucun de ces seuils n'est contournable par l'UI ni par la config.
2.2. LA permission DOIT offrir : « une fois » / « toujours pour cette branche » / « refuser et expliquer » (l'explication devient une règle) ; « toujours » DOIT proposer des gardes (`sauf compte X`, `sauf plage horaire`) — chaque garde est une règle visible et le moteur d'exécution la vérifie AVANT chaque occurrence.
2.3. LA table de réversibilité DOIT être rédigée par l'agent depuis les docs API, versionnée, avec statut par action : `ratified_operator` | `demo_only` (ratification agent, suffisante pour l'org de démo, INSUFFISANTE pour tout compte réel — vérifié structurellement).

### Requirement 3 — Le rodage
3.1. LES 5 premières exécutions autonomes d'une branche DOIVENT notifier en direct (toast système) avec « annuler » à un geste ; à partir de la 6e, silencieuses (journal seulement). Le compteur de rodage se remet à zéro après toute rétrogradation.

### Requirement 4 — Les écritures sûres
4.1. CHAQUE pas d'exécution DOIT porter `idempotency_key = hash(occurrence_id, step)` posée au journal AVANT l'appel ; un pas dont la clé existe en statut `done` NE DOIT JAMAIS être ré-exécuté.
4.2. AVANT chaque écriture, LE SYSTÈME DOIT relire l'état ciblé ; SI un champ du périmètre a changé depuis la décision, ALORS `escalader()` — l'occurrence passe en Copilote avec le contexte, rien n'est écrit.
4.3. CHAQUE écriture DOIT journaliser sa before-image ; « annuler » DOIT rejouer l'inverse de toute action `reversible` et le prouver par relecture ; une action non annulable dans une branche promue est une contradiction détectée en CI (2.1).
4.4. QUAND le process reprend après interruption, LES pas en statut `pending` DOIVENT être réconciliés par relecture de l'état distant (effet présent → `done` ; absent → décision de reprise selon la politique de l'étape), jamais rejoués à l'aveugle.

### Requirement 5 — Le témoin et la rétrogradation
5.1. 15 % des occurrences d'une branche promue DOIVENT rester humaines (échantillonnage déterministe par hash d'occurrence) ; branches < 30 occ/mois : 30 %.
5.2. LA dérive DOIT être testée par SPRT (H0 : accord ≥ 95 % ; H1 : accord ≤ 85 %) avec n minimum avant tout déclenchement ; la calibration DOIT être un LIVRABLE : script de simulation joint prouvant fausses alertes < 5 %/an ET détection de la dérive lente (scène 3) < 30 occurrences.
5.3. QUAND le SPRT déclenche, LA branche DOIT rétrograder en Copilote immédiatement, notification avec les chiffres, rodage remis à zéro, et l'événement journalisé.

### Requirement 6 — Le mode Documenté
6.1. TOUTE tâche observée DOIT avoir sa spec vivante générée (`spec.md` : déroulé, branches, durées, entités) régénérée à chaque évolution — c'est le mode plancher, il ne demande rien à personne.

---
