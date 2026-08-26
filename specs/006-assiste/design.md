# Spec 006 — L'Assiste et l'apprentissage silencieux · design

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

**Détection & génération** : à la clôture (hook 003), si tâche `assisted_email` et déclencheur = message entrant → assemblage du contexte (thread via metadata + corps chargés en mémoire seulement, entités résolues, états) → politique → `DraftFacade.createInThread(thread_id, body)`. La façade enveloppe le client Gmail : `{ createDraft, updateDraft, deleteDraft, listDrafts }` — pas de `send` dans le type (R1.3) ; le test structurel vérifie par réflexion qu'aucun symbole d'envoi n'est atteignable depuis le module.

**Rattachement** : index des occurrences ouvertes par (thread_id, entité) ; à chaque envoi détecté dans la fenêtre : match unique → diff ; multiple → `unmatched`. Diff : tokenisation mots, LCS, taux de reprise = 1 − (mots conservés / mots proposés) ; catégories par heuristiques (salutation/politesse = ton ; chiffres/entités = contenu ; ordre des paragraphes = structure).

**Ensemencement du terrain** : l'agent joue les deux rôles — un script alimente la boîte opérationnelle de messages entrants réalistes (avec canaris), les occurrences se capturent sur l'org de démo + le thread, l'agent « répond » (édite le brouillon puis l'envoie depuis la boîte, PAS depuis Noe) pour produire les diffs. Trois profils scriptés : envoi tel quel, reprise légère (20 %), réécriture totale (scène 1).

**Boucle few-shot** : un signal `envoyé tel quel` promeut l'épisode dans la sélection few-shot de sa branche (récence + succès) — l'Assisté s'améliore en mangeant ses propres réussites, sans cérémonie.

**Squelette traversant (D22)** : onglet « Assisté » — métriques par tâche, liste des brouillons avec statut et taux de reprise, chaque ligne cliquable vers l'occurrence. Tests visuels : 4 états, plus l'état `draft_failed` visible.

**Impact inter-specs** : R6.3 (003) amendé comme dit en 3.3 (decisions.md) ; nouveau type `AssistSignal` dans episode-spec ; le hook de clôture (003) gagne un point d'extension propre (le shadow 004 et l'Assisté 006 s'y branchent tous deux — pas de duplication).

---
