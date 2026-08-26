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

---

# 2026-08-26 — Arbitrage des dix écarts prompt maître / existant

Le prompt maître a été déposé ce jour (22 569 octets). La comparaison avec ce
qui était déjà construit a produit dix écarts. Chacun est tranché ci-dessous,
**et le prompt maître a été amendé en conséquence** : il ne doit plus contenir
une seule ligne contredite par une décision postérieure.

> Principe retenu : quand un document et une décision divergent, on ne laisse
> pas le document mentir. On l'amende, daté, avec le motif ici.

---

## D1 — Le socle commercial reste. « Zéro backend en v0 » est amendé

**Écart.** Le prompt maître interdisait Supabase, Stripe et tout service déployé
avant le premier utilisateur externe. Or `noe-prod` (Supabase, ~10 $/mois),
un projet Vercel avec sa landing, et l'appariement Stripe existent déjà.

**Décision.** Le socle reste. **Le prompt maître est antérieur au pivot Product
Hunt**, décidé après sa rédaction : lancement vendable en 5-6 semaines, comptes
Elevay réutilisés, données isolées. C'est ce pivot qui a fait provisionner, via
le brief de session 0 — l'agent n'a pas dévié, il a suivi la consigne la plus
récente.

**Motif.** Dix dollars par mois coûtent moins cher que le re-setup complet, et
le projet est déjà isolé (base neuve, RLS forcée, zéro politique). Détruire pour
reconstruire dans cinq semaines serait du gaspillage discipliné, pas de la
discipline.

**Ce qui est annulé.** Les flux d'authentification Supabase (email + Google) :
ils n'ont aucune spec, et un non-objectif explicite les visait. Ils attendront la
spec commerciale.

**Amendement.** La section « Zéro backend en v0 » devient « Socle commercial PH,
données isolées, gates humains ».

---

## D2 — Tauri, définitivement. Les mentions d'extension Chrome sont rétrogradées

**Écart.** Contradiction **interne** au prompt maître : trois passages imposaient
« extension Chrome uniquement » (non-objectifs, structure du dépôt, phase 1),
tandis que l'invariant 2 disait « app desktop Tauri par défaut — extension
seulement si le spike révèle un déficit sémantique web ».

**Décision. Tauri + UIA.** L'invariant 2 est la formulation la plus récente et la
plus argumentée : il conditionne l'extension à un **constat**, pas à une
préférence. Les trois autres passages sont des résidus antérieurs.

**Motif.** Une extension ne voit que le navigateur, alors que la tâche observée
traverse plusieurs applications. Et le spike déjà construit mesure UIA — donc la
bonne chose.

**Amendement.** Les trois passages sont réécrits : app desktop Tauri + UIA par
défaut, extension rétrogradée au rang d'**adaptateur conditionnel**, activé
seulement si le spike constate un déficit sémantique web. L'invariant 3 perd sa
terminologie MV3 (« worker tué, onglet fermé ») au profit des causes réelles du
capteur desktop.

---

## D3 — `features.json` est supprimé, pas réparé

**Écart.** Le prompt maître interdisait de modifier `features.json` autrement
qu'en basculant `passes`. J'y avais ajouté des champs `preuve`, `spec`, `note`,
et modifié des `depends_on`. Violation nette d'un interdit de méthode.

**Décision.** Le fichier est **supprimé**. La méthode Kiro adoptée depuis fait
des `tasks.md` de chaque spec l'unique liste de vérité.

**Motif.** Deux listes de vérité concurrentes valent moins que zéro : elles
divergent, et on finit par ne plus savoir laquelle fait foi. Les `tasks.md` sont
plus riches — chaque case porte ses critères et ses requirements.

**L'interdit reste, transposé.** Il s'applique désormais aux cases des
`tasks.md` : on ne coche que du vérifié de bout en bout, on n'édite **jamais**
une tâche pour la faire passer.

**Amendement.** La section « Méthode de travail » est réécrite autour des specs
et de leurs `tasks.md`. `CLAUDE.md` les référence.

---

## D4 — `packages/` reste. L'exception `episode-spec` fait jurisprudence

**Écart.** Le prompt maître décrit `core/ ports/ adapters/ harness/ ui/` à la
racine, mais mentionne ailleurs « la spec du format d'épisode en MIT dans
**packages/episode-spec** ». Il se contredit.

**Décision.** `packages/` reste. L'exception nommée fait jurisprudence.

**Motif.** Le monorepo pnpm est en place, testé, publié, avec ses licences
séparées. Le renommer ne produirait aucune preuve nouvelle.

**Amendement.** Le bloc « Structure du dépôt » reflète l'arborescence réelle.

---

## D5 — Grade A : la confirmation API est ajoutée, avec un garde explicite

**Écart.** L'invariant 7 exige « bornes confirmées API » pour le grade A. Le
`gradeOf` implémenté ne le vérifie pas.

**Décision.** La condition est ajoutée, **avec un garde `non vérifiable sans
connecteur`** tant que la spec 003 n'a pas livré la fédération.

**Motif.** L'exigence est juste, mais elle n'est pas vérifiable aujourd'hui :
aucun connecteur ne lit d'API. L'écrire sans garde produirait des grades C
partout ; l'omettre laisserait un invariant non tenu. Le garde dit exactement où
on en est, et le regrade se fera à la fédération.

---

## D6 — Canaris d'injection : différés à la spec 004

**Écart.** L'invariant 16 exige des canaris d'injection à chaque build, au même
rang que les canaris PII. Seuls les canaris PII existent.

**Décision.** Différés à la **spec 004 (politique)**, décision datée.

**Motif.** Le test de non-obéissance suppose quelque chose qui puisse obéir. Avec
des stubs (`politiqueParfaite`, `politiqueNulle`), il n'y a **aucun chemin
d'exécution** qu'une instruction adverse pourrait détourner : le test passerait
au vert sans rien prouver — le pire des tests. Ils entrent au harness le jour où
la politique LLM existe.

---

## D7 — `init.sh` : remplacé par le rituel `CLAUDE.md` + `pnpm verify`

**Écart.** La méthode exigeait un `init.sh` lancé à chaque session.

**Décision.** Pas de script. Le rituel est `pnpm verify` (lint, lint SQL,
typecheck, build, 129 tests) plus le rejeu du corpus doré, tous deux documentés
dans `CLAUDE.md` et exécutés en CI.

**Motif.** `init.sh` devait « build l'extension et lancer l'org de démo » :
l'extension n'existe plus (D2) et l'org de démo pas encore. Un script wrapper
autour d'une commande existante n'ajoute qu'un endroit de plus où se tromper.

---

## D8 — Seuils du spike alignés sur le prompt maître

**Écart.** Le prompt maître fixe (a) ≥ 90 % de stabilité rôle+nom, (b) **100 %**
des actions d'état, (c) CPU < 5 %. Le script de verdict utilisait 80 % et 90 %.

**Décision.** Alignement immédiat sur 90 / 100 / 5. Aucune discussion : mes
seuils étaient simplement plus mous que la consigne écrite.

---

## D9 — NER : regex + HMAC en v1, NER avant tout utilisateur externe

**Écart.** L'invariant 5 impose « blocage catégoriel → NER local → extraction
d'attributs → hash salé ». La spec 002 fait regex + HMAC et diffère le NER.

**Décision.** La divergence, **déjà déclarée par la spec 002**, est consignée :
regex + HMAC en v1, NER **avant tout utilisateur externe**.

**Motif.** À n=1, sur le poste du sujet, les canaris surveillent et la
bibliothèque de motifs couvre les quatre familles PII. Un NER local ajoute un
modèle, une latence et une surface de bug pour un gain non mesuré. Il devient
indispensable dès qu'un tiers est observé.

**Amendement.** L'invariant 5 porte l'ordre réel et la date d'échéance du NER.

---

## D10 — `events.jsonl` en clair en local, export chiffré

**Écart.** La structure du dépôt annonçait « events.jsonl (chiffré, rotation) » ;
la spec 002 écrit du JSONL en clair.

**Décision.** **Clair en local.** C'est le disque de l'utilisateur, sous sa
session, protégée par son ouverture de session. L'**export**, lui, est chiffré
(spec 002, R6.1) — c'est là que les données quittent le poste, donc là que le
chiffrement compte.

**Motif.** Chiffrer at-rest sans sortir la clé du même disque protège contre un
attaquant qui a déjà le disque *et pas* la session — un cas étroit. La clé HMAC,
elle, est bien protégée par DPAPI : c'est elle qui déverrouille les jointures de
tout le corpus.

**Le chiffrement at-rest reste un item de durcissement**, pas un abandon.

**Amendement.** Le bloc structure dit « clair en local, export chiffré ».

---

## 2026-08-26 — D11 : l'irréductible « mes gestes sont la donnée » ne couvre pas les bancs d'essai

**Corrige :** `docs/doctrine-execution.md`, irréductible #1.

**L'erreur.** J'avais écrit que les gestes de l'opérateur sont irréductibles
« quand ils SONT la donnée mesurée », et j'en avais conclu que les occurrences du
spike devaient être humaines. La conclusion ne suit pas de la prémisse.

**La distinction.** Deux choses différentes portent le mot « occurrence » :

| | Ce qu'on mesure | Qui doit agir |
| --- | --- | --- |
| **Corpus d'épisodes** | le **comportement** à apprendre : quels champs l'opérateur touche, dans quel ordre, avec quelles règles implicites | **l'opérateur, irréductiblement.** Un script rejouerait mes hypothèses sur son travail, pas son travail. |
| **Banc d'essai du capteur** | le **capteur** face à une application : stabilité des signatures, couverture des événements, coût CPU | **le script, et c'est mieux.** |

**Pourquoi le script est *préférable* ici, pas seulement toléré.** Le spike compare
deux stratégies d'abonnement. Si l'humain rejoue ses cinq occurrences en
« globale » puis cinq en « focus », il introduit sa propre variance entre les deux
phases — et l'écart mesuré mélange l'effet de la stratégie avec l'effet de sa
fatigue, de son apprentissage, de son humeur. **Un script rejoue exactement la
même chose des deux côtés.** La comparabilité, qui est tout l'objet du spike, ne
tient qu'à cette reproductibilité.

**Ce que ça change.** Les occurrences du spike sortent de la liste des
irréductibles. Elles y étaient à tort : je m'étais interdit une automatisation qui
produit une mesure *meilleure*, pas seulement plus rapide.

**Ce que ça ne change pas.** Le corpus d'épisodes de la spec 002 reste
irréductiblement humain. Le jour où on capture pour apprendre — et non plus pour
mesurer le capteur — c'est l'opérateur qui travaille, sans script.

**Obligation de transparence.** Tout verdict issu d'occurrences scriptées doit le
dire, en toutes lettres, dans le verdict lui-même : *« occurrences scriptées
Playwright — banc capteur, pas donnée comportementale »*. Un chiffre dont on
ignore la provenance est un chiffre qui finira par être mal lu.

---

## 2026-08-26 — D12 : autonomie d'exécution étendue

**Remplace** la doctrine du même jour. Détail complet dans
`docs/doctrine-execution.md`.

**Identité opérationnelle.** Adresse `contact+<projet>@elevay.app`, dont je lis
les courriels via le MCP Gmail. Coffre local `~/.noe/coffre/`, chiffré **DPAPI**,
où je génère et stocke les identifiants des comptes que je crée. TOTP activé
partout où c'est proposé.

**Budget pré-autorisé : ≤ 30 €/mois cumulés.** Je souscris, je journalise, je
continue. Engagement en cours : Supabase `noe-prod`, ~10 $/mois. Reste ~20 €.

**Permission permanente** sur les créations de comptes, apps OAuth,
configurations, déploiements, Playwright. J'annonce, je ne demande pas.

**Quatre irréductibles**, remontés en une ligne actionnable : captcha
infranchissable après 3 tentatives · vérification SMS vers le téléphone de
l'opérateur · dépense hors budget · engagement juridique (banque, signature
légale, Stripe live).

**Règle anti-échouage.** Un blocage met l'item en attente avec l'action préparée,
je réordonne, je continue. Immobilisation totale seulement si tout est bloqué ou
sur gate facturable/irréversible.

### La réserve que je maintiens sur les graines TOTP

Pour les comptes **jetables** que je crée de bout en bout, garder la graine TOTP
à côté du mot de passe est sans conséquence.

Pour les comptes qui **touchent à de l'argent ou à l'entreprise** — Stripe,
facturation Azure, Supabase de production — **je n'y co-loge pas le second
facteur.** Deux facteurs rangés au même endroit n'en font pas deux : ils en font
un seul, plus long. Je configure, j'active, je prépare ; c'est la *garde* de la
graine que je ne prends pas, sur ce périmètre précis.

---

## 2026-08-26 — D13 : le terrain de construction est une org Salesforce Developer Edition

**Contexte.** Deux placeholders du brief sont restés vides — l'outil réel de
traitement des leads n'a pas été nommé. La consigne prévoyait ce cas : à défaut,
Salesforce Developer Edition.

**Décision.** Le **terrain de construction** est une org Salesforce Developer
Edition, créée par l'agent avec son identité opérationnelle.

```
org          orgfarm-7d442f390a-dev-ed.develop.my.salesforce.com
utilisateur  contact+noespike.09cd56be5bda@agentforce.com
courriel     contact+noespike@elevay.app
identifiants coffre DPAPI ~/.noe/coffre/salesforce-de.dpapi
coût         0 € — édition gratuite, expire après 45 j d'inactivité
```

**Motif du choix.** Lightning est le **pire cas** d'arbre d'accessibilité — c'est
précisément le risque que le spike doit chiffrer. Mesurer sur une application
légère aurait produit un chiffre rassurant et sans valeur.

**Le terrain de preuve reste à fixer par l'opérateur** : le CRM réel de sa
campagne. Demandé en une ligne, sans attendre dessus.

### Exposition acceptée, et déclarée

Le mot de passe de cette org **est apparu en clair dans la conversation** :
l'automatisation d'un formulaire exige de transmettre la valeur, et l'outil
recopie le code exécuté. J'ai réduit ce que je pouvais — la réponse à la question
de sécurité a été générée **dans la page** et n'existe nulle part, puisque je
contrôle la boîte mail et peux toujours réinitialiser.

L'exposition est acceptée pour **cette org précise** : jetable, gratuite, ne
contenant que des données fictives que je crée moi-même. Elle ne le serait pas
pour un compte adossé à de l'argent ou à l'entreprise — et le coffre DPAPI existe
justement pour que ce cas-là ne se présente pas.

### Note d'obligation

Le spike tournera sur des **occurrences scriptées Playwright**. Le verdict devra
le dire en toutes lettres : *banc capteur, pas donnée comportementale* (D11).
