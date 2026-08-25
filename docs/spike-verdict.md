# Verdict du spike — F01

> **Template. Non rempli.** La session 1 remplit ce fichier ; tant qu'il porte
> ce bandeau, `F01.passes` reste `false`.
>
> Règle : ce verdict s'appuie sur un rejeu chiffré et reproductible, pas sur une
> impression. Les critères de choix du terrain sont dans `docs/prompt-maitre-v0.md`.

## 1. Question tranchée

<!-- Une phrase. Quelle décision ce spike ferme-t-il ? -->

## 2. Choix du terrain

| Critère | Poids | Terrain A (Salesforce) | Terrain B (Google Workspace) |
| --- | --- | --- | --- |
| _critère 1 du prompt maître_ | | | |
| _critère 2_ | | | |
| _critère 3_ | | | |
| _critère 4_ | | | |
| _critère 5_ | | | |
| **Total** | | | |

**Terrain retenu :** <!-- A ou B --> — **motif :** <!-- une phrase -->

## 3. Étage exécution : protocole de comparaison

- **Corpus utilisé :** <!-- chemin, empreinte, nombre d'épisodes -->
- **Modèles comparés :** <!-- déploiements exacts, versions, dates -->
- **Métriques :** <!-- ce que le juge mesure, avec les seuils -->
- **Commande de reproduction :** <!-- la commande exacte, copiable -->

## 4. Résultats

| Modèle | Métrique 1 | Métrique 2 | Coût / épisode | Latence p50 | Latence p95 |
| --- | --- | --- | --- | --- | --- |
| | | | | | |

## 5. Verdict

**Étage exécution retenu :** <!-- modèle + déploiement -->

**Motif :** <!-- deux phrases maximum, appuyées sur le tableau ci-dessus -->

**Ce que ce verdict n'affirme pas :** <!-- les limites du spike, honnêtement -->

## 6. Conséquences

- [ ] `.env` mis à jour avec le déploiement retenu
- [ ] `features.json` : F01 passe à `true` **si et seulement si** le juge est vert
- [ ] Les features dépendantes sont réordonnées si le terrain a changé

---

**Date :** <!-- AAAA-MM-JJ -->  ·  **Signé :** <!-- nom -->
