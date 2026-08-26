# Spec 006 — L'Assiste et l'apprentissage silencieux · requirements

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

### Requirement 1 — La génération du brouillon
1.1. QUAND une occurrence clôturée appelle une réponse email (règle de détection : le déclencheur de l'épisode est un message entrant ET la tâche est configurée `assisted_email`), LE SYSTÈME DOIT créer un brouillon Gmail DANS le thread concerné, via l'API, sur le compte connecté.
1.2. LE brouillon DOIT être rédigé par la politique de la tâche (004 : template + règles + few-shot de la même branche) avec le contexte de l'épisode (entités, états, historique du thread) transmis en données délimitées (T4).
1.3. LE SYSTÈME NE DOIT JAMAIS envoyer : le scope OAuth demandé est minimal (lecture + drafts), AUCUN chemin de code n'appelle un endpoint d'envoi, et un test structurel le prouve (l'API cliente est enveloppée dans une façade sans méthode d'envoi — l'interdit est un type, pas une discipline).
1.4. SI la politique échoue (unparseable, LLM down), AUCUN brouillon n'est créé et l'occurrence est marquée `draft_failed` avec raison — jamais un brouillon vide ou générique.

### Requirement 2 — Le cycle de vie du brouillon
2.1. UN brouillon non envoyé après 7 jours DOIT être supprimé (API) et compté `expired`.
2.2. QUAND l'humain édite puis envoie (le brouillon ou SA propre réponse — scène 1), LA détection d'envoi (history Gmail, 003) DOIT rattacher le message envoyé à l'occurrence par (thread, fenêtre 72 h, entité) ; ambiguïté → `unmatched`, compté, jamais mal apparié.

### Requirement 3 — L'apprentissage silencieux
3.1. LE diff proposé/envoyé DOIT être calculé (niveau mot), pseudonymisé par le pipeline standard, et persisté comme signal : {occurrence, branche, taux de reprise, catégories d'écart (ton, contenu, structure — heuristiques simples), textes REDACTÉS}.
3.2. AUCUNE question NE DOIT être posée à l'humain au titre de l'Assisté — le mode entier est silencieux par définition ; les signaux nourrissent les few-shot (un envoyé-tel-quel devient un exemple de premier rang pour sa branche).
3.3. LES corps de messages DOIVENT être traités en mémoire et seuls les extraits REDACTÉS nécessaires au signal sont persistés (amendement déclaré du R6.3 de la 003 : l'Assisté a besoin du texte, il n'en garde que la forme pseudonymisée).

### Requirement 4 — Les métriques
4.1. PAR tâche assistée, LE SYSTÈME DOIT exposer (`noe assisted` + squelette) : brouillons créés, envoyés tels quels, repris (avec taux de reprise moyen), expirés, unmatched, temps brouillon→envoi (médiane) — les chiffres du futur bilan.

---
