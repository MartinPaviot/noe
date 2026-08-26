# Spec 005 — Les modes, la promotion et les ecritures sures · tasks

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

- [ ] **1. Schémas 005** — Branch étendu, JournalEntry ; migrateurs ; tests. _Req : impact._
- [ ] **2. Table de réversibilité** — générée depuis les docs API du CRM démo, statuts, vérification structurelle (compte réel impossible sans `ratified_operator`). _Req : 2.3._
- [ ] **3. `noe modes`** — classement exhaustif recalculé à la clôture, justifications, chemins chiffrés. _Req : 1.1, 1.2._
- [ ] **4. Port write + FakeConnector adverse** — conflit, panne mi-vol, double envoi ; moteur testable sans réseau. _Req : 4.x socle._
- [ ] **5. Moteur d'exécution sûr** — machine à états, idempotence pré-posée, relecture/verrouillage, before-images, réconciliation de reprise (kill-test). _Req : 4.1, 4.2, 4.3, 4.4._
- [ ] **6. Annuler** — inverse rejoué + preuve par relecture ; test sur chaque action de la table. _Req : 4.3._
- [ ] **7. Éligibilité + permission + gardes** — seuils non contournables, 3 réponses, gardes→règles vérifiées à l'exécution, refus→règle. _Req : 2.1, 2.2._
- [ ] **8. Rodage** — toasts, annuler à un geste, compteur, reset post-rétrogradation. _Req : 3.1._
- [ ] **9. Témoin déterministe** — échantillonnage, ratios par fréquence. _Req : 5.1._
- [ ] **10. SPRT + calibration livrée** — implémentation + script de simulation (3 trajectoires dont dérive lente) + rapport commité. _Req : 5.2._
- [ ] **11. Rétrogradation** — déclenchement, notification chiffrée, retour Copilote, journal. _Req : 5.3._
- [ ] **12. Spec vivante (Documenté)** — génération + régénération, sous test (le fichier change quand une branche apparaît). _Req : 6.1._
- [ ] **13. Squelette : Branches + Journal** — permission réelle, annuler cliquable, 4 états × 2 + état « désactivé avec raison », tests visuels. _Req : D22, D21._
- [ ] **14. GATE** — sur l'org de démo : une branche atteint l'éligibilité réelle (n ≥ 40, accord ≥ 95), permission accordée avec une garde, 10 exécutions autonomes propres dont 5 en rodage notifié, UNE annulation prouvée par relecture, UNE rétrogradation SPRT déclenchée par divergences injectées puis retour en promotion après re-rodage — journal complet à l'appui. → la spec 006 s'ouvre.
