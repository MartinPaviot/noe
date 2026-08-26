# Spec 005 — Les modes, la promotion et les ecritures sures · design

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

**Port d'écriture** : `interface WriteConnector extends ReadConnector { write(ref, action: ToolCall, opts:{idempotency_key}): Promise<WriteResult> }` — implémenté par l'adaptateur CRM démo + FakeConnector (scénarios : conflit optimiste, panne mi-vol, double envoi). Le moteur d'exécution ne dépend que du port.

**Machine à états d'une occurrence promue** : `decided → [garde_ok] → for each step: journal(pending, before) → read+check(4.2) → write(key) → read → journal(done)` ; toute anomalie → `escalated` avec contexte complet. Machine à états de branche : `observed → copilot → promoted ⇄ demoted` — transitions uniquement par le module promotion (les seuils y vivent, l'UI ne fait qu'afficher).

**SPRT** : implémentation Wald classique sur épreuves de Bernoulli (témoin + escalades comptées comme échecs), log-vraisemblance cumulée entre bornes A/B calculées de α=0,05/an, β=0,10 ; n_min = 10. `scripts/sprt-calibration.ts` génère les trajectoires (stable, panne franche, dérive lente 95→85 sur 60 occ) et sort le rapport joint au repo.

**Rodage & notifications** : toasts via l'API notification Tauri ; « annuler » profond-lié vers l'occurrence. Échantillonnage témoin : `hash(occurrence_id) mod 100 < ratio` — déterministe, auditable.

**Squelette traversant (D22)** : onglets « Branches » (statuts, seuils, boutons permission actifs/inactifs selon 2.1 — la carte de permission complète naît ici, elle sera réutilisée par la 008) et « Journal » (occurrences, pas, before-images, bouton annuler). Tests visuels : 4 états × 2 onglets, plus l'état « bouton production désactivé avec la raison ».

**Impact inter-specs** : Branch gagne `status`, `permission`, `witness_ratio`, `rodage_count` ; nouveau `JournalEntry` (déjà spécifié en v1 §3) entre dans episode-spec ; le juge apprend à consommer les occurrences témoin comme épisodes ordinaires.

---
