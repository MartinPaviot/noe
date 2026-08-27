# Spec 003 — La fédération et la boucle fermée · requirements

> Ouverte le 2026-08-27, au gate de la spec 002. Contenu verbatim de la spec
> scellée ; les amendements portent leur marque, comme en 002.

### Connecteur CRM · Lectures avant/après · Réconciliation · Grade A

Périmètre : brancher les deux systèmes de vérité du terrain en LECTURE, résoudre les entités candidates de la spec 002, produire les états avant/après, réconcilier les deux plans, et fermer la boucle — un épisode capturé sur du travail réel qui rejoue au vert dans le harness. **C'est la spec du jalon.** **Dépend de** : 001 verte, 002 tâches 1-5 et 8 vertes (capture bornée fonctionnelle). **Doctrine d'exécution** (docs/doctrine-execution.md) : tout ce qui est exécutable l'est par l'agent — création de l'org de démo, apps OAuth, connexions de SES comptes opérationnels — sous les quatre seules exceptions (captcha, SMS vers le téléphone de l'opérateur, > 30 €/mois, juridique/live). Le terrain de cette spec est le **terrain de construction** : l'org de démo créée et peuplée par l'agent — tout s'y développe, s'y teste et s'y prouve techniquement. La validation sur usage réel n'est PAS un prérequis de cette spec ni du build : elle arrive à la phase de durcissement, avec le dogfooding du fondateur et les bêta-testeurs — le produit n'attend jamais après la vie de son constructeur. **Hors périmètre explicite** : toute ÉCRITURE vers les systèmes (la promotion/exécution est une spec ultérieure — ce connecteur est en lecture seule, structurellement : le port de cette spec n'expose pas `write`), les webhooks temps réel (polling delta en v1), les brouillons Gmail (spec Assisté), le juge sémantique.

---

---

### Requirement 1 — Le terrain et la connexion
**User story** : en tant qu'opérateur, je connecte mes deux systèmes en quelques minutes, une fois, et je n'y pense plus.
1.1. LE CRM du terrain DOIT être fixé en tâche 0 (décision datée dans docs/decisions.md) ; le code NE DOIT JAMAIS encoder le CRM hors de son adaptateur — `terrain.json` (config) porte le choix, les scopes et les `scope_fields` par tâche.
1.2. LA connexion d'un système DOIT dérouler un OAuth PKCE via navigateur avec callback loopback local. L'agent connecte ses propres comptes opérationnels de bout en bout (org de démo, adresse opérationnelle). Le jour où un compte réel d'utilisateur se connecte (dogfooding, bêta), l'utilisateur tape ses identifiants lui-même (irréductible secrets) puis l'agent ou l'app termine. Les tokens DOIVENT être stockés via DPAPI et NE DOIVENT JAMAIS apparaître dans un fichier suivi, un log ou un export non enveloppé.
1.3. QUAND un access token expire, LE SYSTÈME DOIT le rafraîchir silencieusement ; SI le refresh échoue définitivement, ALORS l'état du connecteur passe à `reauth_required`, visible au tray, sans crash ni perte d'épisode (les lectures manquées deviennent des trous classés).

### Requirement 2 — La résolution des entités
**User story** : en tant que corpus, mes entités pointent vers de vrais enregistrements, ou disent honnêtement qu'elles ne savent pas.
2.1. QUAND un épisode contient des entités candidates (spec 002), LE SYSTÈME DOIT tenter leur résolution en `api_refs` par clés fortes uniquement : identifiant système exact, email exact (comparé en tokens HMAC des deux côtés — voir R6.2), domaine + nom exact.
2.2. SI la résolution est ambiguë (0 ou ≥ 2 candidats), ALORS l'entité DOIT rester non résolue avec la raison précise (`not_found` | `ambiguous:n` | `blocked:<cause>`) — LE SYSTÈME NE DOIT JAMAIS deviner. **Amendé le 2026-08-27 par D36** : l'énumération n'avait que deux raisons, ce qui obligeait un adaptateur qui prend un `403` à répondre `not_found` — c'est-à-dire à affirmer une absence qu'il n'a pas constatée. La troisième raison élargit l'exigence d'honnêteté, elle ne la relâche pas.
2.3. QUAND une entité est résolue, LE SYSTÈME DOIT enregistrer `resolved:{by, at}` (la clé qui a tranché, l'horodatage).

### Requirement 3 — Les lectures avant/après
**User story** : en tant que juge, je dispose d'un état de référence dont je connais la provenance et les limites.
3.1. QUAND une entité est résolue PENDANT un épisode ouvert, LE SYSTÈME DOIT lire immédiatement son état (`state_before`), restreint aux `scope_fields` de la tâche plus les champs observés changés.
3.2. QUAND l'épisode se clôt, LE SYSTÈME DOIT relire chaque entité résolue (`state_after`).
3.3. SI l'historique du système montre une écriture sur l'entité ANTÉRIEURE à la première lecture (dans la fenêtre de l'épisode), ALORS LE SYSTÈME DOIT reconstituer le `state_before` via cet historique et marquer l'état `reconstituted:true` ; SI la reconstitution est impossible (champ non historisé), ALORS le champ est marqué `unknown_before` et exclu du verdict avec raison — jamais silencieusement compté.
3.4. LES états DOIVENT être des objets plats champ→valeur normalisés (mêmes règles que le juge, spec 001 §5) et passés au pipeline de redaction AVANT persistance (R6).

### Requirement 4 — La réconciliation et le bilan de complétude
**User story** : en tant que système, je me vérifie moi-même : chaque changement du monde est expliqué, hors périmètre, ou déclaré trou.
4.1. PENDANT un épisode (+ marge de 60 s après clôture), LE SYSTÈME DOIT collecter les changements API des entités résolues (polling delta) et joindre chaque changement aux événements UI par fenêtre de 30 s + même entité.
4.2. CHAQUE changement API DOIT finir dans exactement une colonne : `expliqué` (joint à une action UI), `hors_périmètre` (champ hors scope, ou acteur ≠ opérateur quand le système l'expose), `trou` (aucune action UI jointe, dans une fenêtre de gap ou pas).
4.3. LE bilan DOIT être écrit dans `episode.completeness` et agrégé par jour ; LE taux d'expliqué agrégé est LA métrique de santé, exposée par `noe health`.
4.4. LES changements API survenant HORS de tout épisode NE DOIVENT PAS être collectés en v1 (périmètre N3 strict — l'ambiant API viendra avec le N1).

### Requirement 5 — La robustesse
**User story** : en tant que process, une API distante en colère ne me fait ni crasher ni mentir.
5.1. TOUTE requête DOIT passer par le client commun : backoff exponentiel + jitter sur 429/5xx (plafond 5 tentatives), respect des en-têtes Retry-After, refresh sur 401, timeout par requête.
5.2. CHAQUE erreur définitive DOIT être classée : `retryable_exhausted` → trou avec cause ; `permission` → hors_périmètre avec raison ; `not_found` → résolution échouée — un connecteur NE DOIT JAMAIS faire crasher le process ni bloquer la clôture d'un épisode (les lectures manquantes déclassent le grade, elles n'empêchent rien).
5.3. LE SYSTÈME DOIT respecter un budget d'appels par épisode (config, défaut 30) et le journaliser ; dépassement → arrêt des lectures + trou déclaré, jamais de tempête de requêtes.

### Requirement 6 — La confidentialité de la fédération
**User story** : en tant qu'utilisateur, brancher mes systèmes n'élargit pas ce qui touche mon disque en clair.
6.1. LES états et payloads fédérés DOIVENT passer le MÊME pipeline de redaction que la capture (regex → HMAC) avant persistance ; les canaris sont étendus : un canari planté dans un champ du CRM de démo NE DOIT JAMAIS apparaître en clair dans un épisode, un log ou un rapport.
6.2. POUR comparer sans exposer, LES valeurs d'identification lues des APIs (emails…) DOIVENT être tokenisées à la volée et comparées en tokens — la valeur claire ne DOIT vivre qu'en mémoire, jamais persistée.
6.3. LES corps de messages (Gmail) NE DOIVENT PAS être persistés dans cette spec : métadonnées et en-têtes seulement (le contexte pour brouillons arrive avec la spec Assisté, avec ses propres règles).

### Requirement 7 — Le grade A et la boucle fermée (le jalon)
**User story** : en tant que projet, je sais enfin si ma thèse tient : un épisode réel rejoue et se juge.
7.1. QUAND un épisode clôturé a toutes ses entités résolues avec `state_before`/`state_after` présents, séquence sans trou et redaction validée, LE SYSTÈME DOIT le régrader A (règles 001 R2.1 inchangées, recalculées).
7.2. QUAND `noe replay` tourne sur un épisode A réel avec la `perfectPolicy`, LE VERDICT DOIT être `accord` — c'est le test d'auto-cohérence bout en bout (capture + fédération + juge alignés).
7.3. LE JALON de la spec — technique, sur le terrain de construction : ≥ 5 épisodes clôturés capturés par la VRAIE chaîne (app desktop + UIA + fédération, jamais des fixtures) sur des parcours réalistes VARIÉS de l'org de démo (dont au moins 2 parcours non identiques au script nominal : variante de branche, interruption), ≥ 3 de grade A, complétude agrégée ≥ 85 %, zéro canari, rejeu des A au vert. Chiffres archivés dans `docs/milestones/boucle-fermee.md`. **Note de portée, écrite au jalon** : ces chiffres prouvent que la boucle FONCTIONNE, pas encore que le produit apprend un humain — cette seconde preuve appartient à la phase de durcissement (dogfooding + bêtas), et le rapport du jalon le dit explicitement.
