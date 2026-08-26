# Journal des décisions

> Une décision par entrée, datée, avec son motif et ses conséquences. On note ce
> qui a été **tranché**, pas ce qui a été fait — le code dit ce qui a été fait.
>
> Règle actée (spec 002, §1bis) : **toute spec qui touche le schéma d'une spec
> antérieure le déclare ici**. Sinon les documents divergent en silence.

---

## 2026-08-26 — `Gap.cause` gagne `pause` et `timeout`

**Spec :** 002 · **Touche :** `@noe/episode-spec` (spec 001)

La spec 002 introduit deux causes de trou que la spec 001 ne connaissait pas :

- **`pause`** — l'opérateur suspend la capture (R5.2). À la reprise, un trou est
  écrit avec ses bornes. Une pause n'est pas une absence d'événements : c'est une
  fenêtre pendant laquelle on *sait* n'avoir rien vu. La distinction compte pour
  qui lit le corpus plus tard.
- **`timeout`** — clôture automatique à 60 minutes (R1.3), protection contre la
  borne de fin oubliée.

**Amendement direct**, sans bump de `schema_v` : rien n'a shippé, aucun épisode
n'existe hors du corpus doré. Ajouter une valeur à une énumération est de toute
façon rétro-compatible en lecture — les épisodes existants restent valides.

---

## 2026-08-26 — « Redaction validée » cesse d'être un placeholder

**Spec :** 002 (R4.6) · **Touche :** `gradeOf` dans `@noe/episode-spec` (spec 001)

La spec 001 exigeait « redaction validée » pour le grade A sans jamais définir ce
que ça voulait dire. J'avais donc implémenté la seule chose vérifiable à ce
moment-là : une clé d'entité non vide. C'était structurel, et faible.

La spec 002 tranche : **scan de la bibliothèque de motifs sur l'épisode
entièrement sérialisé, zéro correspondance exigée.** C'est maintenant ce que
`gradeOf` applique, et le déclassement porte le détail (`2×EMAIL, 1×IBAN`).

**Conséquence immédiate et voulue : l'épisode doré (e) a été refusé.**
`005_canaris.json` portait un courriel, un téléphone et un IBAN en clair, tout en
étant déclaré grade A. Sous R4.6 il vaut C — et le validateur a raison : un
épisode réellement capturé porterait des tokens, pas les valeurs.

L'épisode a donc été réécrit tel qu'une capture conforme le produirait
(`EMAIL_7f3a9c21`, `TEL_FR_4b81e0d2`, `IBAN_e1c07a45`). Il est plus utile ainsi :
il montre à quoi ressemble une redaction réussie.

### Les canaris se scindent en deux groupes

Le sweep de la spec 001 avait besoin que le corpus contienne ses canaris. R4.6
interdit désormais que le corpus contienne des PII. Les deux exigences ne sont pas
contradictoires, elles portent sur des objets différents :

| Groupe | Rôle | Présence dans le corpus |
| --- | --- | --- |
| `marqueurs` | `CANARY_PII_001` — ne matche aucun motif PII, voyage donc jusqu'au juge et prouve qu'une valeur d'épisode ne ressort jamais en clair dans un rapport | **présents**, à dessein |
| `interdites` | formes PII réelles (courriel, téléphone, IBAN, carte) | **absentes**, et le test l'exige |

---

## 2026-08-26 — La bibliothèque de motifs vit dans `episode-spec`, pas dans le capteur

**Spec :** 002 · **Risque adressé :** divergence entre deux implémentations

Le capteur de la spec 002 est en Rust, le validateur de grade en TypeScript. Les
deux doivent appliquer **exactement** la même bibliothèque : si elles divergent,
le capteur redacte selon un jeu de motifs et le juge valide selon un autre — et la
fuite passe entre les deux.

Les motifs sont donc déclarés en `episode-spec` sous forme de **chaînes** plutôt
que de littéraux `RegExp`, précisément pour que l'adaptateur Rust puisse les
consommer telles quelles. Avant la tâche 3 de la spec 002, il faudra générer un
miroir JSON et un test de synchronisation — sans quoi la promesse ci-dessus n'est
qu'une intention.

---

## 2026-08-26 — Les rapports du harness n'émettent aucune valeur en clair

**Spec :** 001 · **Motif :** invariant I appliqué à la frontière de sortie

Un rapport de rejeu affichait `propose=` et `observe=` avec les valeurs réelles.
Sur l'épisode (e), le canary sweep échouait — **à juste titre** : un rapport qui
imprime `notes=RIB FR76…` fait sortir du contenu du processus.

Les chaînes sortent désormais en empreinte `sha256:` tronquée. L'égalité reste
visible, ce qu'un diff exige ; la valeur ne sort jamais. Les nombres et booléens
passent en clair : ils sont structurels et leur cardinalité les rend
inexploitables.

---

## 2026-08-26 — `pnpm test` recompile d'abord

**Motif :** un piège rencontré en vrai

Les tests du harness importent `@noe/episode-spec`, qui résout vers `dist/`. Après
une modification de `src/` sans rebuild, la suite entière tourne contre du code
mort — et passe au vert alors que le changement testé n'est pas chargé. C'est
arrivé pendant l'implémentation de R4.6 : 99 tests verts sur un validateur qui
n'existait pas encore dans le build.

`pnpm test` fait donc `pnpm build && vitest run`. `pnpm test:only` reste
disponible pour les boucles rapides, en connaissance de cause.
