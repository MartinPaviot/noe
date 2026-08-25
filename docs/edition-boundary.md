# La frontière d'édition

> Ce document répond à une seule question : **qu'est-ce que Noe a le droit de
> modifier dans le monde réel, et sous quelle condition ?**
> Il fait foi. Un chemin de code qui le contredit est un bug de sécurité.

## Les trois régimes

| Régime | Noe peut | Noe ne peut pas | Statut |
| --- | --- | --- | --- |
| **Observation** | Lire le système de vérité dans un scope borné, capturer des épisodes, produire des bilans locaux | Écrire quoi que ce soit, où que ce soit | Régime par défaut, actif dès F05 |
| **Assisté** | Tout ce qui précède, plus rédiger des brouillons stockés localement et les présenter | Envoyer, publier, ou écrire dans le système de vérité sans un geste humain explicite | Cible du lancement Product Hunt (F09) |
| **Autonome** | Tout ce qui précède, plus agir dans une enveloppe prouvée par le juge | Sortir de l'enveloppe que le juge a validée | **Non livré.** Upgrade « en rodage », lancé quand le harness l'a prouvé — jamais promis avant |

## La règle du geste humain

En mode Assisté, un brouillon devient une action réelle uniquement par un geste
humain **explicite, informé et révocable** :

- **Explicite** : un clic sur une action nommée (« Envoyer ce brouillon »), jamais
  un consentement implicite, jamais un défaut coché, jamais un délai qui expire.
- **Informé** : l'humain voit le contenu exact qui partira, et vers qui, avant de
  valider. Pas de résumé, pas de troncature sur l'écran de validation.
- **Révocable** : tant que le geste n'a pas eu lieu, le brouillon reste local et
  supprimable sans trace résiduelle.

**Conséquence sur le code.** Aucune fonction d'écriture ou d'envoi ne doit être
atteignable depuis une boucle automatique, un planificateur, une reprise sur
erreur ou un retry. Le seul appelant légitime est un gestionnaire d'événement
d'interface déclenché par l'utilisateur.

## Ce qui compte comme « écriture »

Est une écriture, et tombe donc sous la règle du geste humain :

- Envoyer un courriel, y compris à l'utilisateur lui-même.
- Créer, modifier ou supprimer un enregistrement dans le système de vérité.
- Créer un brouillon **côté serveur tiers** (un brouillon Gmail est déjà une
  écriture chez Google — il ne compte pas comme un brouillon local).
- Poser un fichier ailleurs que dans `~/.noe`.
- Déclencher un webhook ou une automatisation tierce.

N'est pas une écriture : écrire dans `~/.noe`, produire un bilan local, afficher
une proposition à l'écran.

## Le passage à l'Autonome

Il n'aura pas lieu par décision de calendrier. Les conditions, toutes nécessaires :

1. Un corpus doré couvrant la classe d'actions visée.
2. Un verdict `noe judge` vert et reproductible sur ce corpus.
3. Une enveloppe d'action explicite : ce qui est permis, ce qui est interdit,
   ce qui déclenche un arrêt.
4. Un journal complet et un chemin d'annulation pour chaque action autonome.
5. Un opt-in par classe d'actions — jamais un interrupteur global.

Tant que les cinq ne sont pas réunies, la communication publique dit « en
rodage » et rien d'autre.
