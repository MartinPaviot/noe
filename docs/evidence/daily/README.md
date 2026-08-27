# L'evidence quotidienne

> « Le fondateur doit pouvoir **voir** l'avancement, pas le lire. »
> — `docs/decisions.md`, D26

Une image par jour de build, ici, nommée `AAAA-MM-JJ-<sujet>.png`.

```
pnpm evidence
```

## Pourquoi ce dossier existe

Huit specs de tests verts ne montrent rien. Le harness, le capteur, la redaction,
le writer : tout se vérifie par des chiffres et des lignes de journal. C'est
rigoureux et c'est invisible — et un fondateur qui ne voit rien pendant des mois
ne peut corriger le tir qu'au moment où c'est le plus cher.

## Ce que l'image contient, et pourquoi

**Aujourd'hui**, il n'existe aucune fenêtre produit : la tâche 8bis de la spec
002 la fera naître. L'image montre donc ce que le produit possède réellement —
ses **trois icônes de barre d'état** — plus l'état vérifié de son avancement.
C'est peu. C'est mieux qu'une case cochée sans image.

**À partir de la tâche 8bis**, l'image devient une capture du **squelette
traversant** : la liste des épisodes réels, leur grade, leur complétude, leur
timeline. Elle grandit ensuite à chaque spec.

## Deux règles qui la gardent honnête

**Tout est lu dans le dépôt, jamais recopié.** Le nombre de tâches, leurs
titres, les derniers arbitrages, le compte des tests : le script les extrait de
`tasks.md`, `decisions.md` et des sources Rust. Un chiffre saisi à la main
devient faux au troisième jour, et une preuve fausse est pire qu'une preuve
absente.

**Jamais l'écran, seulement le produit.** Ce dépôt est public. Une capture plein
écran y publierait le bureau de l'opérateur — courriels ouverts, noms de clients
dans une barre des tâches. Ce serait exactement la fuite que la première règle du
projet interdit, commise par l'outil censé la prévenir. Le script ne sait
composer que des pixels appartenant au produit ; il n'a aucun moyen de
photographier autre chose.

## Quand

À la clôture de chaque session, et au minimum une fois par jour calendaire où des
commits sont poussés. Produite par script, jamais à la main : une preuve visuelle
qui dépend de la discipline de quelqu'un cesse d'exister en trois semaines.
