# Politique de securite

## Le postulat de Noe

Noe lit du travail reel : des courriels, des enregistrements CRM, des documents.
Ce sont les donnees les plus sensibles d'une entreprise. L'architecture part donc
d'une contrainte non negociable : **aucun contenu utilisateur ne quitte le poste.**

Concretement :

- La capture, le stockage des episodes et le rejeu sont **locaux** (`~/.noe`).
- La telemetrie est **opt-in** et ne transporte que des compteurs et des codes
  d'erreur — jamais un fragment de contenu, jamais un identifiant d'enregistrement.
- Les connecteurs sont en **lecture bornee** : un scope minimal, une fenetre
  temporelle explicite, et un journal de tout ce qui a ete lu.
- Une CI dediee (`lint anti-contenu`) refuse toute migration ou tout schema qui
  creerait une colonne capable d'accueillir du contenu utilisateur cote serveur.

## Secrets

Aucun secret n'est commite. `.env` est ignore par git ; `.env.example` documente
les noms de variables avec des valeurs factices. La CI execute `gitleaks` sur
l'historique complet a chaque push et chaque pull request, et echoue au premier
hit.

Si un secret a ete pousse par accident : le revoquer d'abord chez l'emetteur,
reecrire l'historique ensuite. Jamais l'inverse.

## Signaler une vulnerabilite

Ouvrez un avis de securite prive via l'onglet *Security* du depot GitHub
(« Report a vulnerability »). Merci de ne pas ouvrir d'issue publique.
Delai de premiere reponse vise : 72 heures.

## Perimetre

| Dans le perimetre | Hors perimetre |
| --- | --- |
| Fuite de contenu utilisateur hors du poste | Vulnerabilites des systemes de verite tiers |
| Contournement du juge mecanique | Deni de service sur un poste local |
| Falsification d'une cle de licence | Ingenierie sociale du proprietaire du poste |
| Escalade via un connecteur | Bugs d'affichage sans impact de confidentialite |
