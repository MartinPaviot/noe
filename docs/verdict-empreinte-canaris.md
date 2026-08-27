# Verdict de l'empreinte et des canaris — tâches 12 et 13 de la spec 002

**2026-08-27.** Mesuré sur le poste, application réelle, épisode réel, extension
réelle. Aucun chiffre de ce document ne vient d'une estimation.

Rejouable :

```
pnpm --filter @noe/extension-banc page        # la page de demonstration
# lancer l'application, Ctrl+Alt+D pour ouvrir un episode
node scripts/mesurer-empreinte.mjs --fenetres 5
pnpm --filter @noe/extension-banc banc        # la charge, en parallele
# Ctrl+Alt+F pour clore
NOE_EPISODES="%APPDATA%\app.noe.desktop\episodes" pnpm test:only
```

## 1. L'empreinte (R7.1)

Cinq fenêtres de 30 s, sous une charge de **45 rechargements de page et 225
interactions en 2 min 30** — soit environ 90 gestes par minute, bien au-delà d'un
rythme humain de travail.

| fenêtre | CPU (% d'un cœur) | mémoire |
| --- | --- | --- |
| 1 | 2,65 % | 49 Mo |
| 2 | 2,57 % | 49 Mo |
| 3 | 2,78 % | 49 Mo |
| 4 | **3,35 %** | 49 Mo |
| 5 | 1,84 % | 49 Mo |

**Verdict : DANS LE BUDGET.** Pointe à 3,35 % pour un plafond de 5 %, 49 Mo pour
un plafond de 200.

### Ce que la première mesure a coûté, et appris

La première exécution annonçait **0,00 % à chaque fenêtre**. `Measure-Object`
refuse un `TimeSpan`, rendait une somme vide, et `[double]''` vaut 0 : le script
annonçait « DANS LE BUDGET » en n'ayant rien mesuré. La deuxième annonçait
**`NaN %`** — la machine est en français, `-f` écrit « 1234,5 », et `Number()`
n'en fait rien.

Deux verdicts faux de suite, tous deux plausibles à l'œil. Le script **refuse**
désormais de conclure quand toutes les fenêtres valent exactement zéro ou quand
une valeur n'est pas finie : un vert qui veut dire « je n'ai rien vu » est pire
qu'un rouge.

### Ce que la vraie mesure a montré

La première mesure honnête donnait **5,26 / 5,76 / 5,97 %** — au-dessus du
budget. La cause était directe : `surface_de()` faisait un `OpenProcess` +
`QueryFullProcessImageNameW` **par événement UIA**, sur un abonnement global
filtré qui en voit des centaines par minute. Un cache d'identifiants de
processus, vidé à chaque épisode, a ramené la pointe de 5,97 % à 3,35 %.

## 2. La dégradation ordonnée (R7.2)

Observée **en vrai**, pas seulement en test. La mesure hors budget a produit,
dans l'épisode `01M11JW9Y521CTYBCA0XQ3J5J2` :

```json
{ "kind": "degraded", "source": "system", "ts": "2026-08-27T12:32:55.620Z",
  "degraded": { "what": "snapshots", "from": "actifs", "to": "suspendus" } }
```

Trois fenêtres consécutives au-dessus du budget, un palier, un événement écrit.
La chaîne mesure → décision → journal → épisode tient de bout en bout.

Les trois paliers de R7.2 sont figés dans l'ordre de l'exigence et testés
unitairement : suspendre les snapshots, élargir l'antirebond, alerter. Chacun
**fait** quelque chose — un palier annoncé et non appliqué serait pire que pas de
palier, puisqu'on écrirait au journal une dégradation qui n'a pas eu lieu.

## 3. Un défaut que seule la capture réelle pouvait montrer

Le premier épisode réel comptait **1960 événements**, et ses `scope_fields`
étaient : `about:blank - Google Chrome`, `Barre d'adresse et de recherche`,
`about:blank - Utilisation de la mémoire - 19,4 Mo`.

**La partition de D19 n'était qu'une intention.** L'abonnement UIA est global
filtré : il voyait la chrome de Chrome comme le reste. Chaque geste dans une page
comptait donc **deux fois** — une fois par le DOM, une fois par UIA — et
l'interface du navigateur noyait le travail de l'opérateur.

La classe de surface est désormais **appliquée** : une source ne capture que sa
propre classe, et le croisement est refusé, compté au hors-périmètre. Le même
protocole donne maintenant :

```
540 evenements = 45 repetitions x 12 observations, exactement
scope_fields : ['Description', 'Statut de la piste', 'Enregistrer']
completude : explained 540, out_of_scope 860, gaps 0
```

Les 860 hors-périmètre sont les événements UIA sur `chrome.exe`, refusés et
comptés — pas racontés.

## 4. Les canaris sur capture réelle (R4.3)

Les quatre formes interdites de `canaris.json` saisies dans un vrai formulaire, à
travers un vrai navigateur, par **trois portes distinctes** : la valeur d'un
champ, le nom accessible d'un contrôle, le titre du document.

Épisode `01M11M6REWAEB6R6VAAY0N3XG2`, 26 événements, grade B :

```
episode.json   : 7918 octets — fuites : AUCUNE
journal.jsonl  : 5354 octets — fuites : AUCUNE
cibles : Rappeler EMAIL_blvbgfywcnrhm · Rappeler TEL_FR_hmyr6hfqnjiju
         Rappeler IBAN_ugfeynt5ehebe · Rappeler CARTE_lh5nuyhnllvge
```

Chaque canari est reconnu **par son type** et remplacé par son jeton. Les valeurs
de champ, elles, n'apparaissent nulle part : le capteur ne les lit jamais.

Le sweep étendu est un cas de test à part entière, sauté — bruyamment — quand
`NOE_EPISODES` est absent, parce qu'il exige un vrai bureau qu'aucune CI n'a. Il
**rougit** sur une fuite injectée : vérifié.

## 5. Ce que ces mesures n'affirment pas

- **La charge du banc n'est pas une SPA lourde.** R7.1 parle de Lightning ; le
  banc martèle une page à quatre racines shadow. Elle est plus rapide qu'un
  humain mais moins riche qu'un CRM, et les deux écarts jouent en sens inverse.
- **Une seule machine, un seul jour.** Rien sur un poste chargé, une batterie
  faible, ou un antivirus qui inspecte les tuyaux nommés.
- **Le p95 n'en est pas un.** Cinq points ne font pas une distribution ; le
  chiffre publié est le maximum, et le script le dit dans sa sortie.
- **Le troisième palier n'a pas été atteint en réel.** Seul le premier a été
  observé sur une vraie dégradation ; les deux autres ne sont vérifiés
  qu'unitairement.
- **Le cache d'identifiants de processus peut mentir.** Windows recycle les
  numéros ; la fenêtre est de quelques millisecondes et le cache est vidé à
  chaque épisode, mais le risque n'est pas nul.
