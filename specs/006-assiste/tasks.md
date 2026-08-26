# Spec 006 — L'Assiste et l'apprentissage silencieux · tasks

**Statut :** fourni par l'opérateur le 2026-08-26, déposé **sans reformulation**.

> Le texte de cette spec est celui de l'opérateur, découpé en triptyque. Les
> ajouts postérieurs porteront un marqueur `[amendé Dn]` et ne supprimeront
> aucune ligne d'origine.
>
> **Numérotation.** Les mentions de « D22 » dans ce texte désignent l'arbitrage
> du **squelette traversant**, enregistré sous le numéro **D26** dans
> `docs/decisions.md` — D22 y désignait déjà la reclassification du 34,5 %
> d'UIA. Le texte n'a pas été modifié ; c'est la note qui lève l'ambiguïté.

Périmètre : le mode Assisté — pour les occurrences dont la sortie est un email, Noe rédige un brouillon DANS le thread avec le contexte de l'épisode ; l'humain reste l'auteur pour toujours ; l'amélioration se nourrit du diff entre le proposé et l'envoyé, sans jamais poser une question. **Dépend de** : 004 verte (politique) ; 005 non requise (aucune écriture d'état — un brouillon n'est pas une action d'état, c'est un texte offert). **Terrain** : la boîte opérationnelle de l'agent + threads de démo qu'il ensemence lui-même (les deux rôles de la conversation). **Hors périmètre explicite** : tout envoi automatique (interdit structurel, pas une option différée), les brouillons hors email (CRM notes : plus tard), la boîte réelle de l'opérateur (dogfooding, phase durcissement), le NER (inchangé).
Challenge « trois scènes réelles » intégré : (1) l'humain envoie SANS passer par le brouillon (il écrit le sien) → la détection d'envoi rattache quand même la réponse à l'occurrence et le diff se calcule contre le proposé — c'est le signal le plus riche (reprise totale = 100 % de désaccord utile) ; (2) le brouillon traîne trois jours puis le thread meurt → expiration : brouillon supprimé proprement après 7 jours sans envoi, métrique `expired` comptée, jamais un cimetière de drafts ; (3) deux occurrences sur le même thread → le rattachement se fait par (thread, fenêtre temporelle, entité), pas par thread seul, et le cas ambigu est marqué `unmatched` plutôt que mal apparié.

---

---

- [ ] **1. Schéma AssistSignal + amendement R6.3** — types, migrateur, entrée decisions.md. _Req : 3.1, 3.3, impact._
- [ ] **2. Façade Gmail sans envoi** — types sans `send`, test structurel de non-atteignabilité, scopes minimaux vérifiés au flow OAuth. _Req : 1.3._
- [ ] **3. Détection des occurrences assistées** — règle de déclenchement, config par tâche, hook de clôture partagé (point d'extension). _Req : 1.1, impact._
- [ ] **4. Génération du brouillon** — contexte délimité, politique 004, création dans le thread, `draft_failed` propre. _Req : 1.2, 1.4._
- [ ] **5. Cycle de vie** — expiration 7 jours + suppression API, compteurs. _Req : 2.1._
- [ ] **6. Rattachement des envois** — index (thread, fenêtre, entité), unique/unmatched, tests des 3 scènes. _Req : 2.2._
- [ ] **7. Diff + signal** — LCS mots, taux de reprise, catégories, pseudonymisation, persistance. _Req : 3.1._
- [ ] **8. Boucle few-shot** — promotion des envoyés-tels-quels dans la sélection de branche, validée par re-rejeu. _Req : 3.2._
- [ ] **9. Ensemencement + 3 profils scriptés** — boîte alimentée (canaris inclus), envois joués depuis la boîte, jamais depuis Noe. _Req : terrain._
- [ ] **10. `noe assisted` + squelette** — métriques, onglet, tests visuels avec `draft_failed`. _Req : 4.1, D22, D21._
- [ ] **11. GATE** — sur le terrain ensemencé : ≥ 5 brouillons créés dans de vrais threads ; les 3 profils produisent leurs diffs (tel quel / 20 % / réécriture) correctement catégorisés ; 1 expiration exécutée ; zéro envoi par Noe prouvé par le test structurel ET par l'historique Gmail (aucun envoi dont l'auteur est l'app) ; zéro canari dans les signaux persistés. → la spec 007 s'ouvre.
