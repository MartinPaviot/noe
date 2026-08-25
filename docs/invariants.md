# Invariants de Noe

> Les cinq règles courtes vivent dans `CLAUDE.md`. Ce fichier les développe et
> ajoute les invariants d'architecture. Un invariant ne se négocie pas dans une
> session de feature : il se change explicitement, ici, avec une justification.

## I. Aucun contenu utilisateur ne quitte jamais le poste

**Ce que cela interdit.** Envoyer un épisode, un fragment d'épisode, un sujet de
courriel, un nom d'enregistrement ou un identifiant client vers un serveur que
nous contrôlons. Y compris pour déboguer. Y compris en cas d'incident.

**Ce que cela autorise.** Des compteurs anonymes et des codes d'erreur, si et
seulement si l'utilisateur a opté-in explicitement. Un appel à un fournisseur de
modèle reste possible pour l'inférence, mais c'est une frontière documentée dans
`docs/edition-boundary.md`, pas un adossement silencieux.

**Comment on le tient.** Le stockage vit dans `~/.noe`. La CI porte un lint
anti-contenu qui refuse toute migration créant une colonne serveur capable
d'accueillir du contenu. La revue de chaque connecteur vérifie le scope.

## II. Seul le juge mécanique promeut

Aucune feature ne passe `"passes": true` dans `features.json` sur une démo, une
impression ou un avis — y compris le mien. Le seul chemin est un verdict de
`noe judge`, reproductible, sur un corpus doré versionné.

**Corollaire.** Le moteur d'autonomie n'est jamais promis avant que le harness ne
l'ait prouvé. Il est annoncé comme upgrade « en rodage », et il est lancé quand
le juge le dit, pas quand le calendrier le dit.

## III. Une feature par session

Une session ouvre une feature de `features.json`, la mène de bout en bout, la
vérifie, commit à chaque vert, et s'arrête. Pas de feature commencée « en
avance ». Pas de refactor opportuniste qui déborde de la feature en cours.

## IV. Les épisodes sont immuables, les trous sont des événements

Un épisode capturé ne se corrige pas, ne se complète pas, ne se réécrit pas. Si
la capture a manqué quelque chose — permission refusée, quota, fenêtre trop
étroite, panne réseau — cela produit un **trou de capture** : un événement
typé, daté, motivé, stocké à côté des épisodes.

**Pourquoi.** Un corpus qui se rebouche tout seul ment au juge. Un trou visible
dégrade honnêtement un verdict ; un trou invisible le falsifie.

## V. Jamais un secret dans un fichier suivi

`.env` est local et ignoré. `.env.example` documente les noms de clés avec des
valeurs factices. `gitleaks` tourne sur l'historique complet à chaque push.
En cas de fuite : révoquer chez l'émetteur d'abord, réécrire l'historique ensuite.

## VI. Invariants d'architecture

- **`@noe/core` est pur.** Aucun I/O, aucun réseau, aucune horloge, aucun hasard
  non injecté. C'est ce qui rend le rejeu déterministe possible.
- **Le rejeu est hors ligne.** `noe replay` sur un corpus doré ne touche jamais
  le réseau. Si un chemin de code a besoin du réseau pour rejouer, c'est un bug
  d'architecture, pas une contrainte d'environnement.
- **Le format d'épisode est séparable du moteur.** `@noe/episode-spec` est sous
  licence MIT, le reste sous AGPL-3.0. Le format doit pouvoir être réimplémenté
  par un tiers, y compris un concurrent.
- **Les connecteurs sont en lecture bornée.** Scope minimal, fenêtre explicite,
  journal de lecture. Un connecteur qui demande un scope d'écriture est refusé
  en revue tant que la feature qui le justifie n'existe pas.

## VII. Invariants commerciaux

- **Le projet Supabase de Noe n'est jamais celui d'Elevay.** Isolation totale des
  données, même si l'organisation et la facturation sont partagées.
- **L'app OAuth vit sous un domaine neutre**, jamais sous le domaine Elevay :
  c'est la seule pièce réellement collante, elle se démêle mal après coup.
- **Stripe reste en mode test** jusqu'à un gate humain explicite. Le compte est
  celui d'Elevay, en production : aucune commande live n'est lancée sans
  confirmation, et jamais sans son équivalent dry-run d'abord.
- **La souscription Azure sponsorisée n'accueille aucun modèle Marketplace.**
  Les modèles partenaires y sont facturés sur carte bancaire, pas sur les
  crédits. Voir `docs/setup-checklist.md`, section B.
