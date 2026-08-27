# Gate de sortie de la spec 002 — verdict

**2026-08-27.** Deux occurrences réelles capturées sur le poste, une par classe de
surface. Rien ici n'est simulé : l'application tournait, les épisodes sont sur le
disque, le harness les a chargés.

## Les six critères, un par un

| critère | résultat |
| --- | --- |
| épisode dans `episodes/` | ✅ deux, produits par l'application |
| grade B, raison « entités non résolues » | ✅ « déclassé en B : 1 entité non résolue » sur les deux |
| chargé par `noe replay` | ✅ deux épisodes, deux verdicts |
| verdict hors-périmètre propre | ✅ `hors_perimetre 0` |
| zéro canari | ✅ sweep étendu vert sur les deux |
| empreinte dans le budget | ✅ 3,35 % CPU / 49 Mo (plafonds 5 % / 200 Mo) |

`[amendé D19]` **Les deux classes de surface sont couvertes**, comme la tâche
l'exige : « un seul des deux au vert ne ferme pas le gate ».

### L'occurrence navigateur — `DomSource`

```
540 evenements   sources { ui: 540 }
completude       { explained: 540, out_of_scope: 860, gaps: 0 }
grade            B — declasse en B : 1 entite non resolue
scope_fields     Description · Statut de la piste · Enregistrer
```

540 est exactement 45 répétitions × 12 observations. Les 860 hors-périmètre sont
les événements UIA sur le navigateur, refusés par la partition de D33 — comptés,
pas racontés.

### L'occurrence native — `UiaSource`

```
16 evenements    sources { ui: 16 }
completude       { explained: 16, out_of_scope: 1, gaps: 0 }
grade            B — declasse en B : 1 entite non resolue
actions          13 navigate · 2 input · 1 toggle
```

Explorateur de fichiers, piloté au clavier : barre d'adresse, dialogue de
propriétés, recherche, rafraîchissement. Les cibles sont de vrais contrôles
Windows.

### Le verdict du harness

```
~ 01M11KYX…  maj-crm-post-echange  grade B   [exclu des stats]
    ~ aucun etat API a juger (entites non resolues)
~ 01M11MSH…  maj-crm-post-echange  grade B   [exclu des stats]
    ~ aucun etat API a juger (entites non resolues)

episodes 2   comptes 0   exclus 2   non jugeables 2   accord 0/0
accord 0   desaccord 0   manque 0   excedent 0   hors_perimetre 0
```

**« Exclu des stats » est le bon résultat, pas un échec.** Sans connecteur d'API,
aucune entité n'est résolue, donc aucun état n'est comparable : le harness refuse
de compter un accord qu'il ne peut pas établir. C'est précisément le trou que la
spec 003 est faite pour fermer — elle fera passer ces mêmes épisodes au grade A
et rendra l'accord mesurable.

## Trois choses que ce gate ne prouve pas

- **Le poste, pas le monde.** Une machine, un jour, un système de fichiers en
  français. Rien sur un autre matériel ni sur une autre version de Windows.
- **Une page de démonstration n'est pas Salesforce**, et l'Explorateur n'est pas
  Outlook. Les deux surfaces sont réelles ; elles ne sont pas celles de la tâche
  de campagne réelle.
- **Aucun accord n'a été mesuré.** Le harness l'a explicitement refusé, faute
  d'états d'API. Tant que la spec 003 n'a pas fermé cette boucle, on sait
  capturer et on ne sait pas encore juger.

## Ce que les épisodes du gate contiennent, et pourquoi ils ne sont pas ici

Ils portent des noms de dossiers du poste de l'opérateur — pas des secrets, mais
pas non plus des choses à publier dans un dépôt public. Le verdict garde les
chiffres et les contrôles génériques ; les épisodes restent sur la machine.

C'est la règle 1 appliquée à nous-mêmes : « aucun contenu utilisateur ne quitte
jamais le poste » vaut aussi quand le contenu est ennuyeux.
