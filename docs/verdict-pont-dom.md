# Verdict du pont DOM — tâches 6b, 6c, 6d de la spec 002

**2026-08-27.** Mesuré sur un vrai Chrome 151, une vraie extension MV3, un vrai
hôte de native messaging et un vrai tuyau nommé. Ce qui est écrit ici a été
observé **au bout du tuyau**, pas dans la page : entre les deux il y a le service
worker, l'hôte et le protocole de Chrome, et c'est là que les défauts se logent.

Rejouable en une commande :

```
pnpm --filter @noe/extension-banc page      # la page de demonstration
node apps/desktop/src-tauri/target/debug/noe-banc-pont.exe 220
pnpm --filter @noe/extension-banc banc      # cinq repetitions
```

## 1. Ce que le banc rend

```
60 observations, 5 repetitions
  rep 1: 12 obs, 2 changement(s) de valeur
  rep 2: 12 obs, 2 changement(s) de valeur
  rep 3: 12 obs, 2 changement(s) de valeur
  rep 4: 12 obs, 2 changement(s) de valeur
  rep 5: 12 obs, 2 changement(s) de valeur

signatures distinctes : 1
stabilite : 100 %
changements de valeur : 10 / 10 attendus
```

**Le trou du spike est fermé.** Le spike DOM avait refusé de conclure sur les
changements de valeur : il n'en avait observé aucun, sa saisie n'atteignait pas
le champ visé, et il l'avait écrit noir sur blanc — « la capture des changements
de valeur reste donc à démontrer de bout en bout ». Elle l'est : dix sur dix, sur
un `textarea` enfermé à **deux niveaux de racine shadow**, et un `select` au même
endroit.

Les ancrages obtenus, identiques aux cinq répétitions :

| genre | ancrage | région |
| --- | --- | --- |
| focus, invocation | `button|Ajouter une note` | — |
| focus, invocation | `button|Modifier` | Details de la piste |
| focus, changement_valeur | `textbox|Description` | Details de la piste |
| changement_valeur | `combobox|Statut de la piste` | Details de la piste |
| focus, invocation | `button|Enregistrer` | Details de la piste |
| soumission | `form|Enregistrer` | Details de la piste |
| focus, invocation | `button|Contrôle tardif` | — |

`data-aura-rendered-by` **n'apparaît nulle part** : la liste blanche sémantique
tient, et le poison d'ancrage désigné par le spike reste dehors.

Le « contrôle tardif » est créé 400 ms après le chargement, dans une racine
shadow neuve. Sa présence prouve le **rebalayage sur mutation** : sans lui, il
n'existerait pas pour le capteur.

## 2. Six défauts que seule la mesure a montrés

Aucun n'aurait été trouvé en relisant le code.

1. **Chaque clic et chaque focus arrivaient en double.** Le document et chaque
   racine shadow sont branchés, et un `click` est `composed: true` : le même
   geste réveillait plusieurs écouteurs. L'épisode aurait compté deux actions là
   où l'opérateur en a fait une. Corrigé par un marqueur posé sur l'objet `Event`
   lui-même — le seul repère fiable, puisque c'est le même objet qui se propage.
2. **La région et le chemin étaient vides** pour tout contrôle vivant dans un
   composant. `parentElement` rend `null` à la racine d'une racine shadow, et
   `closest` s'y arrête : il faut sauter jusqu'à l'hôte.
3. **Un `<form>` anonyme se nommait d'après son unique bouton.** La région
   « Enregistrer » d'un bouton « Enregistrer » ne situe rien. On ne s'arrête plus
   qu'à un conteneur portant un nom **explicite**.
4. **Cliquer dans une zone de saisie produisait une `invocation`** en plus du
   focus, c'est-à-dire une action que l'opérateur n'a pas faite.
5. **Le serveur de tuyau ne servait qu'une connexion à la fois.** Chrome
   redémarre le service worker quand il veut, et à chaque relance il redémarre
   l'hôte ; le nouveau se présentait pendant que l'ancien tenait la connexion, ne
   trouvait personne, et la capture navigateur s'arrêtait pour le reste de
   l'épisode, en silence. Une instance est désormais en écoute en permanence, un
   fil par connexion, et le suiveur de numérotation est partagé.
6. **Le banc lui-même ne pilotait rien.** `Number(argv[i + 1] || 5)` : quand
   l'option est absente, `indexOf` rend `-1`, `argv[0]` est le chemin de Node —
   une chaîne vraie — et le `||` ne se déclenche jamais. `REPETITIONS` valait
   `NaN`, la boucle ne tournait pas une fois, et le banc annonçait « PILOTAGE
   TERMINE » sans avoir rien piloté. L'absence d'observations s'est lue pendant
   une heure comme un défaut de capture qui n'existait pas.

## 3. Trois pièges d'outillage, pour la prochaine fois

- **`--load-extension` est ignoré depuis Chrome 137.** La voie qui reste est la
  méthode CDP `Extensions.loadUnpacked`, qui exige
  `--enable-unsafe-extension-debugging`.
- **Elle n'est exposée qu'à une session CDP de niveau navigateur.** Un contexte
  persistant de Playwright n'en offre pas (`ctx.browser()` y est nul) et la
  méthode répond « Method not available », ce qui se lit comme une absence de
  support alors que c'est une absence de session.
- **Une extension ainsi chargée ne survit pas au redémarrage** du navigateur, et
  **un script de contenu ne s'injecte pas dans un onglet déjà ouvert**. D'où un
  banc qui lance Chrome lui-même, sur un profil neuf, charge, puis navigue.

Corollaire, qui a coûté deux diagnostics faux : `page.evaluate` s'exécute dans le
monde **principal**, et `Runtime.consoleAPICalled` sur une session de page ne
rapporte pas la console du monde isolé. Un capteur qui tourne parfaitement y
paraît absent. La preuve n'est pas dans la page — elle est au bout du tuyau.

## 4. Ce que ce banc n'affirme pas

- **Une page de démonstration n'est pas Salesforce.** Quatre racines shadow
  contre 270, un formulaire contre une fiche complète, aucun re-rendu d'Aura
  pendant la mesure. La stabilité de 100 % vaut pour ce qui a été mesuré ici, et
  le spike garde le dernier mot sur l'org réelle.
- **L'empreinte du transport n'est pas mesurée.** Le spike a chiffré le calcul
  d'ancrage à 0,02 % ; le coût réel est le balayage et le transport, et c'est la
  tâche 13 qui doit le dire.
- **Le pont n'a pas tourné pendant un épisode réel.** `DomSource` est branchée
  dans l'application et testée, mais l'aller-retour complet capture → journal →
  épisode assemblé reste à faire au gate de la spec.
- **Rien sur Edge ni Firefox.** L'hôte est déclaré sous
  `HKCU\Software\Google\Chrome\NativeMessagingHosts` et nulle part ailleurs.
