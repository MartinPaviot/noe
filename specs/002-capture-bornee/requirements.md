# Spec 002 — La capture bornée (N3) · requirements

**Statut :** approuvé, **amendée** le 2026-08-26 (D19 + verdict du spike DOM).

> Le texte de cette spec est celui de l'opérateur, découpé en triptyque sans
> reformulation. Les ajouts postérieurs sont marqués `[amendé D19]` ou
> `[amendé D20]` et ne suppriment aucune ligne d'origine.

Périmètre : capturer une occurrence de tâche réelle, bornée à la main, et en produire un épisode valide, redacté et rejouable structurellement. **Dépend de** : spec 001 verte (le schéma et le harness existent). **Nourrie par** : le verdict du spike (deux points de design marqués `[SPIKE]`, fixés en tâche 0). **Hors périmètre explicite** : N1 ambiant (spec ultérieure), connecteurs et états API (spec 003 — donc les épisodes de cette spec plafonnent au grade B, entités non résolues : c'est attendu et testé comme tel ; la boucle complète ferme en 003), NER par modèle (différé : regex + pseudonymisation couvrent la v1, les canaris surveillent, le NER arrive avant tout utilisateur externe), toute UI au-delà du menu tray.

---

### Requirement 1 — Les bornes et le cycle de vie
**User story** : en tant qu'opérateur, je délimite moi-même ce qui est observé en profondeur, pour que la capture soit un consentement actif et jamais une ambiance subie.
1.1. QUAND l'opérateur presse le hotkey global de début, LE SYSTÈME DOIT ouvrir un épisode (id ULID, t0, task_slug = la tâche active sélectionnée dans le sous-menu tray, persistée dans la config ; SI aucune tâche active, ALORS le hotkey DOIT notifier « choisir une tâche » et ne rien capturer) et démarrer la capture ; QUAND il presse le hotkey de fin, LE SYSTÈME DOIT clôturer, assembler et valider l'épisode au schéma de la spec 001.
1.2. SI aucun épisode n'est ouvert, ALORS LE SYSTÈME NE DOIT capturer aucun événement N3, d'aucune sorte.
1.3. QUAND un épisode dépasse 60 minutes, LE SYSTÈME DOIT le clôturer automatiquement avec un événement `gap{cause:"timeout"}` final et notifier discrètement (protection contre la borne oubliée).
1.4. QUAND l'épisode est clôturé, LE SYSTÈME DOIT le passer par `load()` du harness (validation + grade + raison) avant de le considérer persisté ; un épisode invalide DOIT être conservé en quarantaine avec son erreur, jamais silencieusement jeté.

### Requirement 2 — La capture UIA et les snapshots
**User story** : en tant que système, j'enregistre ce que l'opérateur fait au niveau sémantique (rôles, noms, actions), pour que les épisodes décrivent le travail et non des pixels.
2.1. PENDANT un épisode ouvert, LE SYSTÈME DOIT s'abonner aux événements UI Automation du conteneur actif (focus, invocation, changement de valeur, changement de structure) et les traduire en événements `ui_action` du schéma.
2.2. CHAQUE cible DOIT être identifiée par rôle + nom accessible + région ; les selectors CSS/XPath et les coordonnées écran NE DOIVENT JAMAIS être utilisés comme identifiants.
2.3. QUAND survient l'un des 5 déclencheurs (soumission ; saisie suivie de 2 s d'inactivité ; bascule d'application avec retour < 60 s ; copier-coller apparié par hash — l'appariement est RESTREINT aux copies ET collages survenus pendant l'épisode sur des surfaces activées ; un collage dont la copie vient d'ailleurs est enregistré `paste{paired:false}` et LE SYSTÈME NE DOIT JAMAIS lire ni hasher le contenu du presse-papiers d'origine externe (un gestionnaire de mots de passe peut y vivre) ; pause > 10 s puis action), LE SYSTÈME DOIT persister un snapshot canonisé du conteneur actif, ≤ 50 Ko après canonisation.
2.4. SI un élément ne peut pas être résolu (rôle/nom absents), LE SYSTÈME DOIT enregistrer l'événement avec `target` dégradé marqué `unresolved:true` et l'incrémenter dans un compteur de santé — jamais d'événement muet.

`[amendé D19]` **R1 à R7 valent pour les DEUX sources.** Le titre de ce
requirement dit « UIA » parce qu'il n'y en avait qu'une ; il faut lire « la
capture », quelle que soit la source. Aucun critère n'est relâché pour le
navigateur : un `DomSource` qui ne tiendrait pas R2.2, R2.4 ou R7.1 ne serait
pas terminable.

Trois critères s'ajoutent, propres à la source navigateur :

2.5. `[amendé D20]` PENDANT un épisode ouvert sur une surface navigateur, LE
SYSTÈME DOIT identifier chaque cible par **rôle ARIA + nom accessible normalisé
+ chemin structurel**, et NE DOIT ajouter d'attribut `data-*` à l'ancrage que
s'il figure dans la **liste blanche sémantique**. Un attribut produit par le
moteur de rendu (`data-aura-rendered-by` et semblables) NE DOIT JAMAIS entrer
dans un ancrage.

2.6. `[amendé D20]` LE SYSTÈME DOIT émettre chaque observation **au fil de
l'eau** vers le processus hôte. AUCUN état de capture NE DOIT dépendre de la
survie du document : une navigation ne perd aucun événement déjà observé.

2.7. `[amendé D20]` LE SYSTÈME DOIT brancher sa capture sur **chaque racine
shadow** de la surface, et rebrancher après mutation du document. SI un
changement de valeur survient dans une racine non branchée, ALORS il est perdu
en silence — ce qui est un défaut de capture, donc un `gap`, jamais un
non-événement.

### Requirement 3 — Le flux fiable
**User story** : en tant que corpus, je suis alimenté par un flux dont chaque perte est déclarée, pour que mes statistiques soient auditables.
3.1. CHAQUE événement DOIT porter un `seq` strictement croissant par épisode ; l'écriture DOIT être en append JSONL avec flush ≤ 5 s ou ≤ 100 événements.
3.2. QUAND le process est tué ou crashe pendant une capture, LE SYSTÈME DOIT au redémarrage détecter l'épisode orphelin, le clôturer avec `gap{cause:"crash"}` aux bornes détectées, et le passer au pipeline de clôture normal.
3.3. QUAND la machine sort de veille pendant un épisode, LE SYSTÈME DOIT insérer `gap{cause:"sleep"}` avec les bornes.
3.4. LE SYSTÈME NE DOIT JAMAIS perdre un événement silencieusement : toute discontinuité de `seq` détectée à la relecture DOIT produire `gap{cause:"seq_break"}`.

### Requirement 4 — Redaction et pseudonymisation
**User story** : en tant qu'utilisateur observé, aucune donnée sensible ne touche le disque en clair, et j'en ai la preuve mécanique.
4.1. AVANT toute persistance, LE SYSTÈME DOIT appliquer : (1) les regex déterministes (email, téléphone FR/intl, IBAN, n° de carte) ; (2) la pseudonymisation par HMAC-SHA256 (clé 256 bits générée à l'installation, stockée via DPAPI) produisant des tokens stables `TYPE_hash8` (ex. `EMAIL_7f3a9c21`).
4.2. LE MÊME input DOIT produire LE MÊME token pour toute la vie de l'installation (jointures preservées) ; deux inputs différents NE DOIVENT JAMAIS produire le même token (test de collision sur corpus).
4.3. LES canaris PII DOIVENT être actifs sur la capture réelle : une session de test qui saisit les chaînes canari DOIT produire un épisode où aucun canari n'apparaît en clair (extension du sweep de la spec 001 aux sorties de capture).
4.4. LA clé HMAC NE DOIT JAMAIS apparaître en clair dans un fichier, un log ou une sortie — la seule exception est sa forme ENVELOPPÉE dans l'export (R6.2).
4.5. LA redaction DOIT s'appliquer à TOUT texte accessible persisté : `payload`, valeurs de snapshots, ET `target.name`, titres de fenêtres, noms de région — les noms accessibles sont le premier vecteur de PII du monde réel (« Email de Jean Dupont — jean@… » comme titre). Le HMAC déterministe préserve le ciblage : même nom → même token, l'égalité de cible tient.
4.6. LE validateur de redaction (le « redaction validée » du grade A, spec 001) est DÉFINI ainsi : scan de la bibliothèque de patterns sur l'épisode entièrement sérialisé — zéro match exigé, sinon déclassement avec raison.

### Requirement 5 — Les contrôles de l'opérateur
**User story** : en tant qu'opérateur, je peux suspendre, effacer et constater, à tout instant, sans justification.
5.1. L'icône tray DOIT exposer 3 états visuels (observe / pause / question en attente) et le menu : pause, panique, ouvrir le dossier de données, quitter.
5.2. QUAND la pause est active, LE SYSTÈME NE DOIT rien écrire jusqu'à reprise explicite ; QUAND la reprise survient pendant un épisode ouvert, LE SYSTÈME DOIT écrire `gap{cause:"pause"}` avec les bornes — une pause n'est jamais une perte silencieuse (cohérence avec 3.4).
5.3. QUAND l'opérateur déclenche la panique (choix 5/15/60 minutes), LE SYSTÈME DOIT effacer irréversiblement les épisodes ENTIERS (clos ou non) intersectant la fenêtre — jamais de découpe partielle d'un épisode clos, l'immutabilité de la spec 001 l'interdit — ainsi que les événements, snapshots et dérivés associés ; SI un épisode est ouvert, ALORS il est avorté intégralement. Sans justification demandée, volume effacé confirmé.
5.4. LA liste blanche des surfaces DOIT être vide à l'installation ; LE SYSTÈME NE DOIT capturer que sur les surfaces explicitement activées par l'opérateur.

### Requirement 6 — L'export
**User story** : en tant qu'utilisateur, la perte de mon poste ne détruit pas mon corpus.
6.1. QUAND l'opérateur demande `noe export`, LE SYSTÈME DOIT produire une archive chiffrée (mot de passe fourni à l'export) contenant épisodes, événements, quarantaine et manifeste (versions, compteurs), relisible par `noe import --verify`.
6.2. LE manifeste DOIT contenir la clé HMAC ENVELOPPÉE par le mot de passe d'export, et `noe import` DOIT l'installer (DPAPI) sur la machine cible — sans elle, les futures captures du même identifiant produiraient d'autres tokens et toutes les jointures du corpus casseraient au premier changement de machine.

### Requirement 7 — L'empreinte
**User story** : en tant qu'opérateur, je ne sais pas que la capture tourne, sinon je la désinstalle.
7.1. PENDANT une capture active sur une SPA lourde, LE SYSTÈME DOIT rester < 5 % CPU soutenu (fenêtre 30 s) et < 200 Mo RAM, mesurés par le script de mesure fourni.
7.2. QUAND le budget est dépassé 3 fenêtres consécutives, LE SYSTÈME DOIT dégrader dans l'ordre (suspendre les snapshots, élargir le debounce, alerter) plutôt que de laisser chauffer — et CHAQUE dégradation DOIT être écrite dans le flux comme événement système `degraded{what, from, to}` : une qualité qui baisse en silence biaise les statistiques en silence.

---
