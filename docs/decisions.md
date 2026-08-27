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

---

## 2026-08-26 — D14 : la garde du second facteur devient une règle permanente

**Statut :** proposée par l'agent, **accordée par l'opérateur** le 2026-08-26.

La doctrine D12 m'autorisait à conserver les graines TOTP des comptes que je
configure. J'avais posé une réserve : pas sur les comptes qui touchent à de
l'argent ou à l'entreprise. Elle est accordée et devient permanente.

**La règle.** Deux facteurs rangés au même endroit n'en font pas deux : ils en
font un seul, plus long. Là où la compromission d'un compte fait bouger de
l'argent ou engage la société, le second facteur reste hors de ma portée.

**Le test, en une question :** si ce compte était compromis, est-ce que de
l'argent bouge, ou est-ce que l'entreprise est engagée ?

| Périmètre | Graine TOTP |
| --- | --- |
| Orgs de démo, projets de test, comptes jetables | coffre DPAPI |
| Stripe, facturation Azure, Supabase production, banque, tout compte adossé à Elevay | téléphone de l'opérateur |

**Ce que ça ne bloque pas.** Je configure la double authentification, je l'active,
je prépare l'enrôlement jusqu'au QR code. C'est la *garde* de la graine que je ne
prends pas — pas l'opération.

---

## 2026-08-26 — D15 : le terrain de preuve reste ouvert (troisième demande)

> **CLOSE par D16** — la révision de la spec 003 a retiré ce prérequis. Conservée
> telle quelle : une demande qui s'est révélée mal posée mérite de rester lisible.

**Statut : SANS OBJET.** Ne bloque rien avant la tâche 12 de la spec 003.

Le terrain de preuve — le CRM réel qui porte la campagne — a été demandé trois
fois. Les trois messages contenaient un gabarit non rempli :

```
[réponds : ton outil réel, ou « Salesforce Developer Edition » ...]
[nom de ton CRM / "Elevay + Gmail, pas de CRM tiers" / ce qui est vrai]
```

**Ce n'est pas bloquant** et ne le devient qu'à la tâche 12 de la spec 003
(auto-cohérence sur épisode réel). D'ici là, tout se construit et se teste sur le
terrain de construction — l'org Salesforce Developer Edition de D13.

**L'action exacte attendue**, en un mot :

- soit le nom d'un CRM tiers (`HubSpot`, `Pipedrive`, `Salesforce`…) ;
- soit **`Elevay + Gmail`** s'il n'y a pas de CRM tiers — hypothèse la plus
  probable au vu de ce que je sais de la campagne, et parfaitement valide : les
  cinq critères de terrain n'exigent pas un CRM, ils exigent **deux systèmes de
  vérité avec API en lecture**. Elevay et Gmail les remplissent.

Tant que la réponse n'arrive pas, je réordonne et je continue ailleurs.

---

## 2026-08-26 — D16 : D15 est close, le terrain de preuve sort du chemin critique

**Statut : D15 SANS OBJET.** Le blocage a été levé par un changement de portée, pas
par une réponse.

La spec 003 a été révisée : **la validation sur usage réel n'est plus un
prérequis** de la spec ni du build. Tout se prouve sur le **terrain de
construction** — l'org de démo que j'ai créée (D13). Le dogfooding et les bêtas
arrivent à la phase de durcissement.

> « Le produit n'attend jamais après la vie de son constructeur. »

**Ce que ça change concrètement.**

| | Avant | Après |
| --- | --- | --- |
| Tâche 0 | 0a construction + 0b terrain de preuve, en attente de l'opérateur | une seule tâche, **aucune question en attente** |
| Jalon (R7.3) | une journée réelle de campagne, non scriptable | **jalon technique** : ≥ 5 épisodes par la vraie chaîne sur des parcours **variés** de l'org de démo |
| Tâche 12 | épisode réel, prérequis : comptes réels connectés | épisode capturé par la vraie chaîne sur l'org de démo |

**Ce que la révision préserve, et c'est le point.** Le jalon exige toujours la
**vraie chaîne** — app desktop + UIA + fédération, *jamais des fixtures* — et des
**parcours variés**, dont au moins deux qui s'écartent du script nominal. Ce n'est
pas un assouplissement de la preuve : c'est un déplacement de ce qu'elle prouve.

Et la spec l'écrit noir sur blanc : ces chiffres prouvent que **la boucle
fonctionne**, pas encore que **le produit apprend un humain**. La seconde preuve
appartient au durcissement. Un jalon qui sait ce qu'il ne prouve pas vaut mieux
qu'un jalon qui l'oublie.

**Ma demande répétée était donc mal posée.** Je réclamais une décision sur le
terrain de preuve comme si elle bloquait le build. Elle ne bloquait qu'un jalon
qui vient d'être redéfini. J'aurais pu le voir plus tôt : le harness, la capture
et la fédération se construisent tous contre une org de démo — c'est le jalon,
pas le chemin, qui exigeait du réel.

---

## 2026-08-26 — D18 : itération 2 du spike, périmètre élargi, grille de décision pré-enregistrée

**Accordée par l'opérateur.** Bornée à une journée.

### Ce que la v1 mesurait mal

Elle comparait les **noms accessibles bruts**. Or le produit ne compare jamais des
noms bruts : il les fait passer par la pseudonymisation et la normalisation avant
tout usage. Mesurer avant le pipeline, c'est mesurer un objet qui n'existe nulle
part dans le système réel.

### Les trois correctifs

**1. Stabilité POST-PIPELINE.** Chaque nom accessible traverse la normalisation
produit avant comparaison : pseudonymisation des fragments de données en tokens
stables, suppression des motifs volatils — horodatages relatifs, compteurs,
identifiants générés. On compare ce que le système compare.

**2. Signature enrichie.** `rôle + nom normalisé + région + position structurelle
dans l'arbre`. Le nom seul était un ancrage trop maigre ; la région et le chemin
de rôles ancrent l'élément dans sa structure.

**3. Réparation de « focus ».** Deux tentatives ont échoué : `TreeScope::Element`
sur la racine ne s'abonne qu'à la racine, puis `get_native_window_handle()` renvoie
0 sur les éléments internes de Chrome. **On ne peut pas conclure sans avoir vu
cette stratégie travailler** : le CPU global n'a aucune marge (4,39 % contre un
plafond de 5) et le walker coûte 429 ms au p95. « focus » est censée être
l'économe — c'est précisément l'hypothèse à éprouver.

### Grille de décision, appliquée sans redemander

| Stabilité post-pipeline | Décision |
| --- | --- |
| **≥ 90 %** | **VERT** — tâche 0 de la spec 002, puis déroulé |
| **60-89 %** | On construit, **chaîne de repli déclarée régime normal**, ciblage marqué *best-effort* |
| **< 60 %** | **Déficit sémantique web constaté** — déclenche le repli pré-décidé : extension Chrome comme adaptateur de capture pour les surfaces navigateur. Retour vers l'opérateur avec le plan. |

### Le rappel d'architecture qui cadre tout ça

> **La preuve vit sur le plan API. Ce spike mesure la qualité du film, pas celle
> du jugement.**

C'est ce qui rend la zone 60-89 % acceptable plutôt que tiède. Le juge compare des
**états API** avant/après ; les branches se calculent sur des **transitions de
champs**. Un ciblage UI imparfait dégrade la lisibilité du film — quel bouton a
été pressé — pas la validité du verdict. On perd en netteté, pas en vérité.

C'est aussi ce qui rend le seuil < 60 % franc : en dessous, le film devient trop
flou pour servir de contexte au copilote, et l'extension navigateur redevient le
bon outil pour les surfaces web.

---

## 2026-08-26 — D19 : le repli est TOTAL par classe de surface, pas partiel par échec

**Déclenché par** le verdict du spike UIA (zone < 60 %, D18).

**Décision.** L'extension Chrome MV3 devient l'adaptateur de capture de **toutes**
les surfaces navigateur ; UIA garde **toutes** les applications natives. La
frontière est la **classe de la fenêtre au premier plan**, pas la qualité observée
du signal.

**Pas de bascule dynamique UIA↔DOM sur une même surface.** C'était l'option
tentante — essayer UIA, retomber sur le DOM quand l'ancrage est mauvais — et
c'est celle qu'on écarte : une bascule conditionnelle crée un système à états dont
les bugs sont **invisibles**. Un épisode dégradé ne dirait pas s'il l'est parce
que la surface est difficile ou parce que la bascule a mal choisi. La partition
par classe, elle, se diagnostique : on sait toujours quel adaptateur parlait.

**Ce que ça ne coûte pas.** Le trait `CaptureSource` de la spec 002 absorbe le
changement sans rien casser en aval : `UiaSource` et `DomSource` implémentent la
même interface, et le pipeline — redaction, writer, assemblage, grades — ne bouge
pas d'une ligne. C'est exactement ce pour quoi le trait existait.

**Ce que ça coûte.** Un canal de plus : extension MV3 → native messaging → app
Tauri. Et l'invariant 18 du prompt maître s'applique enfin pour de bon :
`optional_host_permissions`, jamais `<all_urls>`.

---

## 2026-08-26 — D20 : on re-mesure avant de construire, et la mission ne s'arrête pas

**Décision.** Spike DOM d'une journée maximum, **même protocole** que le spike
UIA : 5 occurrences scriptées identiques sur l'org de démo, normalisation
post-pipeline avant comparaison. Ancrages testés : `data-*`, rôles ARIA
explicites, chemin structurel, nom accessible.

Trois nombres : stabilité post-pipeline des ancrages d'actions d'état, couverture
(100 % exigés), surcoût CPU **in-page** < 5 %.

### Grille, appliquée sans solliciter l'opérateur

| Stabilité post-pipeline | Décision |
| --- | --- |
| **≥ 90 %** | vert — amender la spec 002 (D19) et dérouler |
| **60-89 %** | on construit ; ciblage marqué *best-effort*, chaîne de repli déclarée régime normal |
| **< 60 %** | **on construit quand même** — film best-effort assumé, clés de branches en priorité sur les **transitions de champs** (plan API, qui est déjà leur définition), ciblage UI **corroboratif** ; note de portée écrite au jalon |

**Le point important est la troisième ligne.** Le spike UIA s'est arrêté sur un
« < 60 % » comme si c'était un mur. Ce n'en était pas un : la preuve vit sur le
plan API — le juge compare des états avant/après, les branches se calculent sur
des transitions de champs. **Aucune valeur de stabilité du film ne bloque la
mission.**

Le spike sert à savoir **sur quoi on roule**, jamais à s'arrêter. C'est la leçon
de méthode de cette journée, et elle vaut pour tous les spikes à venir.

---

## 2026-08-26 — D21 : tests visuels Playwright obligatoires, partout, pour toujours

**Décision.** Toute tâche qui produit ou modifie des pixels — UI de l'app,
landing, popup d'extension, rapports HTML — livre ses tests visuels **dans la
même tâche**. Une tâche UI sans test visuel **n'est pas terminable**.

Standard complet dans `docs/mission.md` §4. En résumé : `@playwright/test` +
`toHaveScreenshot`, baselines commitées, `maxDiffPixelRatio: 0.01`, viewport
1280×800, animations coupées, fontes embarquées. Couverture minimale par
surface : **nominal avec données, vide, erreur, chargement** — quatre baselines.

**Motif.** Un diff visuel non détecté est une régression qui voyage jusqu'à
l'utilisateur. Et une baseline régénérée dans un commit séparé du changement est
une baseline qui ne prouve plus rien : elle doit voyager **avec** la modification
qu'elle valide, sinon la revue ne voit jamais le avant/après.

**Distinction à tenir** : les tests visuels prouvent la **non-régression** ;
`docs/evidence/` documente l'**histoire** d'une opération. Deux besoins, deux
dossiers, on ne les mélange pas.

## 2026-08-26 — D22 : le 34,5 % d'UIA a été mesuré hors de son périmètre

**Constat, en amendant la spec 002.** Le spike UIA rend une stabilité
post-pipeline de **34,5 %**. Ce chiffre a servi à déclencher D19 — le repli
navigateur — et c'est un usage légitime. Mais il a été obtenu sur **Salesforce
Lightning, dans un navigateur**, c'est-à-dire exactement la classe de surface que
D19 **retire** au `UiaSource`.

**Décision.** Le 34,5 % est reclassé : il justifie le repli, il ne caractérise
pas le `UiaSource`. Après D19, UIA ne répond que des applications natives, et sa
stabilité sur ce périmètre-là **n'est pas mesurée à ce jour**. Aucun choix de
conception du `UiaSource` ne s'y adosse ; la tâche 13 de la spec 002 produira le
chiffre natif, sur une surface native.

**Ce qui reste valide du spike UIA**, parce que ce n'est pas dépendant de la
surface : la stratégie d'abonnement (**globale filtrée**, seule sous le budget
CPU — 3,16 % contre 8,48 %) et les paramètres du walker (profondeur 12, 1500
nœuds, debounce 300 ms). Ces valeurs sont inscrites en design §2 par la tâche 0,
avec la réserve des 21 % de snapshots tronqués.

**Pourquoi ça compte.** Un nombre survit à son contexte et finit par être cité
pour autre chose que ce qu'il mesure. Celui-ci aurait servi, dans six mois, à
« démontrer » qu'UIA ancre mal — sur un périmètre où il n'a jamais été essayé.

## 2026-08-26 — D23 : la spec 002 existe enfin en fichiers

**Constat.** Le texte de la spec 002 n'avait jamais été écrit dans le dépôt : il
vivait uniquement dans un message. On travaillait dessus depuis des sessions en
la citant de mémoire, et son triptyque était introuvable dans `specs/`.

**Décision.** Le texte de l'opérateur est extrait de la transcription et déposé
en `specs/002-capture-bornee/{requirements,design,tasks}.md`, **découpé sans
reformulation**. Les ajouts postérieurs portent tous un marqueur `[amendé D19]`
ou `[amendé D20]` et **ne suppriment aucune ligne d'origine** — même règle que
pour le prompt maître.

**Ce que ça change.** Les cases de `tasks.md` deviennent cochables, donc la
progression de la spec devient vérifiable au lieu d'être racontée. Tâches 0 et
0bis cochées ; tâche 6 dédoublée en 6a (natif) / 6b (web), plus 6c (changement de
valeur, le trou laissé par le spike DOM) et 6d (frontière de source).

## 2026-08-26 — D24 : la bibliothèque de motifs passe en v2, un numéro fuyait

**Spec :** 002 · **Tâche :** 3 · **Impact inter-specs :** oui, `VERSION_MOTIFS`

**Constat.** En construisant les vecteurs de test partagés — ceux-là même que la
décision « la bibliothèque vit dans `episode-spec` » exigeait avant la tâche 3 —
un numéro de téléphone français est passé en clair :

```
« +33 6 12 34 56 78 »   →   AUCUNE DÉTECTION
```

Le trou vivait **exactement entre deux motifs**. `TEL_FR` s'écrivait
`(?:\+33|0)[1-9]…`, donc exigeait le chiffre collé à l'indicatif. `TEL_INTL`
s'écrivait `\+(?!33)…`, donc excluait explicitement l'indicatif français. Un
numéro écrit avec un espace après `+33` n'était réclamé par personne — et c'est
la graphie la plus courante d'un mobile français à l'international.

**Décision.** `TEL_FR` devient `(?:\+33[ .-]?|0)[1-9](?:[ .-]?\d{2}){4}` et
`VERSION_MOTIFS` passe à **2**. Neuf graphies réellement écrites par des humains
sont désormais énumérées dans un test, avec cinq contre-exemples qui ne doivent
jamais déclencher (dates ISO, versions, montants, codes postaux, durées
relatives).

**Pourquoi la version bouge.** Le champ existait précisément pour ça : « un
corpus jugé sous v1 reste interprétable même si v2 durcit les motifs ». Aucun
corpus réel n'existe encore, donc le coût est nul aujourd'hui — mais l'habitude
de bumper se prend maintenant, pas le jour où il y aura des données à
réinterpréter. Le test qui épinglait `VERSION_MOTIFS` à 1 a rougi et forcé cette
entrée : c'est son travail, il l'a fait.

**Ce que ça dit de la méthode.** Ce défaut n'a été trouvé ni par relecture ni par
les 79 tests existants — dont plusieurs testaient déjà `TEL_FR` — mais par
l'énumération systématique des **formes d'entrée** qu'exigeait la construction
des vecteurs partagés. Les tests précédents vérifiaient les graphies auxquelles
l'auteur avait pensé. Un tableau de graphies réelles vérifie celles auxquelles il
n'a pas pensé.

**Non traité ici, volontairement.** Les bancs de mesure (`spikes/capteur-uia`,
`spikes/dom`) embarquent une copie de l'ancien motif dans leur normalisation de
noms. Ils ne sont pas le produit, leurs verdicts sont déjà consignés, et les
modifier invaliderait des mesures publiées. Ils restent en l'état, datés.

## 2026-08-26 — D25 : la bibliothèque de motifs tient dans le sous-ensemble commun aux trois moteurs

**Spec :** 002 · **Tâche :** 3 · **Impact inter-specs :** oui, `VERSION_MOTIFS` → 3

**Constat.** La bibliothèque est déclarée en **chaînes** depuis le début, avec une
promesse explicite : « pour que l'adaptateur Rust puisse la consommer telle
quelle ». En écrivant cet adaptateur, le moteur a refusé net :

```
\+(?!33)\d{1,3}…
  ^^^
error: look-around, including look-ahead and look-behind, is not supported
```

Le crate `regex` de Rust ne connaît **ni anticipation ni rétrospection** — c'est
un choix de conception assumé du moteur, qui garantit en échange un temps
d'exécution linéaire. La promesse était donc fausse depuis le premier jour, et
seule l'écriture du consommateur pouvait le révéler.

**Décision.** La bibliothèque se restreint au **sous-ensemble commun** aux trois
moteurs qui doivent la lire : JavaScript (validateur et `DomSource`), Rust
(`UiaSource`), et le moteur d'expressions régulières du navigateur. Concrètement :

1. `TEL_INTL` perd son `(?!33)` et devient `\+\d{1,3}…` ;
2. chaque motif porte une **`priorite`** ; l'exclusion française passe par
   l'arbitrage de chevauchement, où `TEL_FR` (40) gagne sur `TEL_INTL` (50) ;
3. un test refuse tout motif contenant `(?=`, `(?!`, `(?<=` ou `(?<!`.

**L'arbitrage était nécessaire de toute façon.** Les vecteurs partagés l'avaient
déjà montré avant même le portage : `FR7630006000011234567890189` déclenche
`IBAN` **et** `TEL_FR`, parce qu'un IBAN contient une suite de chiffres qui
ressemble à un numéro. Sans règle, le même texte aurait produit un jeton
différent selon l'ordre d'évaluation du moteur — donc deux entités là où il n'y
en a qu'une, donc une jointure perdue. La règle est gloutonne et déterministe :
priorité croissante, puis longueur décroissante, puis position.

**Deux autres divergences de moteur, réglées au même endroit.**

- **`\d` n'a pas le même sens.** En JavaScript il est ASCII ; en Rust il est
  Unicode par défaut, donc `١٢٣٤` compterait comme des chiffres d'un côté et pas
  de l'autre. Le compilateur Rust est construit avec `unicode(false)`. Un test
  vérifie qu'une carte en chiffres arabes-indiens n'est détectée nulle part.
- **Les index ne se comptent pas pareil.** TypeScript compte en unités UTF-16,
  Rust en octets. Les vecteurs partagés sont donc **contraints à l'ASCII**, et le
  générateur refuse une entrée non-ASCII plutôt que de produire des positions
  incomparables sans le dire.

**Ce que la vérification croisée prouve, et ce qu'elle ne prouve pas.** Elle ne
compare pas des chaînes de motifs — deux moteurs peuvent lire la même chaîne
différemment, c'est tout l'objet de cette entrée. Elle compare les **sorties** sur
17 entrées communes, détection **et** arbitrage. Elle ne dit rien des entrées
qu'on n'a pas pensé à écrire.

**Le troisième moteur reste à brancher.** Le `DomSource` (tâche 6b) devra lire le
même `motifs.json` et passer les mêmes vecteurs. Tant que ce n'est pas fait, la
bibliothèque est vérifiée sur deux implémentations, pas trois — et la tâche 6b
le porte explicitement.

## 2026-08-26 — D26 : squelette traversant permanent, et une preuve visible chaque jour

**Spec :** 002 et toutes les suivantes · **Portée :** permanente
**Numérotation :** l'arbitrage a été demandé sous le nom « D22 ». Ce numéro
désignait déjà la reclassification du 34,5 % d'UIA, plus tôt le même jour ; il
devient **D26** pour que le journal reste sans ambiguïté.

### La décision

**1. Un squelette traversant, dès la fin de la tâche 8 de la spec 002.**

Une fenêtre Tauri minimale — **une seule vue** — qui liste les épisodes réels
capturés avec leur **grade**, leur **complétude** et leur **timeline
d'événements**, branchée sur les **vraies données** du poste, sous **tests
visuels Playwright dès le premier écran** (D21).

**2. Elle grandit à chaque spec, au lieu d'attendre la 008.** La 003 y ajoute les
états avant/après, la 004 la file priorisée, la 005 les branches promues, et
ainsi de suite. La 008 cesse d'être « construire l'UI » pour devenir « finir
l'UI ».

**3. Une capture d'écran de l'état courant, à chaque fin de journée de build,
dans `docs/evidence/daily/`.** L'opérateur doit pouvoir **voir** l'avancement,
pas le lire.

### Pourquoi ça change quelque chose

Le plan actuel produit des mois de travail que personne ne peut regarder. Le
harness, le capteur, la redaction, le writer : tout se vérifie par des tests
verts et des lignes de journal. C'est rigoureux et c'est **invisible**. Un
fondateur qui ne voit rien pendant huit specs n'a aucun moyen de corriger le tir
avant qu'il soit cher de le faire — et un désaccord sur ce qu'on construit se
découvre alors au moment le plus coûteux.

Un squelette traversant retourne la charge de la preuve : à partir de la tâche 8,
**tout ce qui est capturé est visible**. Une régression de complétude, un grade
qui tombe, une timeline qui se troue : ça se voit d'un coup d'œil, là où il
fallait lire un rapport.

### Ce que ça coûte, et pourquoi c'est acceptable

Une vue de plus à maintenir à chaque spec, et des baselines visuelles à
régénérer. C'est réel. Mais le coût de la 008 telle qu'elle était prévue — tout
l'UI d'un coup, sur des données qu'on n'aura jamais regardées avant — est plus
élevé, et il arrive tard.

### Trois précisions qui éviteront une confusion

**a. « Vraies données » et tests visuels ne s'opposent pas.** L'application lit
le dossier d'épisodes du poste. Les tests visuels, eux, tournent sur des
**fixtures versionnées** (mission §4), sans capture ni réseau — sans quoi les
baselines dépendraient de ce que l'opérateur a capturé la veille, et le contrôle
serait ingérable. Les deux existent, ils ne servent pas à la même chose.

**b. Le premier écran sera presque vide, et c'est normal.** La tâche 8 produit le
premier épisode assemblé : le squelette affichera un ou deux éléments. L'état
« jour 1 vide » est justement l'une des quatre baselines exigées par D21 — la
pauvreté du premier écran n'est pas un défaut à masquer, c'est un état à tester.

**c. « Journée de build » se définit, sinon la règle ne s'applique jamais.** Le
travail se fait en sessions, pas en journées. La règle retenue : **une capture à
la clôture de chaque session, et au minimum une par jour calendaire où des
commits ont été poussés.** Le fichier est nommé `AAAA-MM-JJ-<sujet>.png`.

**d. La capture est produite par un script, jamais à la main.** Une preuve
visuelle qui dépend de la discipline de quelqu'un cesse d'exister en trois
semaines. Elle est donc automatisée, et le rendu natif Tauri se capture en
session de développement — pas en CI, où il n'y a pas d'affichage (mission §4).

**e. Jamais l'écran, seulement le produit.** Le dépôt est **public**. Une capture
plein écran y publierait le bureau de l'opérateur — courriels ouverts, noms de
clients dans une barre des tâches, fenêtres d'un autre travail. Ce serait
exactement la fuite que la première règle du projet interdit, commise par
l'outil censé la prévenir.

La capture vise donc **la fenêtre de l'application**, jamais le bureau. Tant
qu'aucune fenêtre n'existe, elle compose les **pixels que le produit possède
déjà** — les trois icônes de barre d'état — sur un fond neutre. Le jour où le
squelette naît, elle capture cette fenêtre-là et rien d'autre.

Cette contrainte n'est pas une réserve tatillonne : un dépôt public reçoit une
capture par jour, et il suffit d'une seule mauvaise pour que ce soit
irréversible.

### Avant que le squelette existe

Il n'y a pas encore de pixels produit. Jusqu'à la tâche 8, l'evidence
quotidienne montre ce qui existe réellement : les **trois états de l'icône de
barre d'état** et l'état de démarrage de l'application. C'est peu, c'est vrai, et
c'est mieux qu'une case cochée sans image.

## 2026-08-27 — D27 : la détection du collage attend un arbitrage, la copie non

**Spec :** 002 · **Tâches :** 6a, 7 · **TRANCHÉ le 2026-08-27 — option 1**

**Le constat.** R2.3 fait du copier-coller apparié l'un des cinq déclencheurs de
snapshot. La logique d'appariement est livrée et testée — y compris son point
délicat, l'interdiction absolue de lire un presse-papiers dont la copie vient
d'ailleurs, que l'implémentation garantit par construction (numéro de séquence,
jamais de contenu).

Il reste à savoir **comment le système d'exploitation nous dit qu'un collage a eu
lieu**. Et il n'y a pas de bonne réponse gratuite.

**Ce qui est fait sans rien demander : la copie.** `GetClipboardSequenceNumber`
change à chaque écriture dans le presse-papiers. On l'interroge au battement ;
c'est un entier, il ne révèle aucun contenu, et il ne demande aucune capacité
particulière. Une copie survenue pendant l'épisode est donc détectable
proprement.

**Ce qui coince : le collage.** Windows n'émet aucun événement « l'utilisateur a
collé ». Les trois voies possibles :

| Voie | Ce qu'elle coûte |
| --- | --- |
| **Hook clavier bas niveau** (`WH_KEYBOARD_LL`) | fiable, et donne au produit la capacité de voir **toutes les frappes du poste** |
| Sondage de `GetAsyncKeyState` | pas de hook, mais un sondage à 50 ms en permanence, et des collages manqués |
| Heuristique sur `Text_TextChanged` | aucune capacité nouvelle, mais des faux positifs et des faux négatifs |

**Pourquoi je ne tranche pas seul.** Le hook clavier est techniquement le bon
choix et je l'aurais pris sans hésiter sur un autre produit. Ici, il ajoute au
binaire une capacité de **journalisation de frappe à l'échelle du système** — la
chose exacte contre laquelle la promesse de Noe est écrite. Même utilisé
honnêtement (installé pendant l'épisode seulement, ne lisant que l'état des
modificateurs, n'enregistrant aucune touche), il déplace ce que le produit *peut*
faire, et pas seulement ce qu'il fait. C'est le genre de frontière qui se
franchit une fois et ne se referme jamais.

Ce n'est ni un captcha, ni une dépense, ni un engagement juridique : ça ne rentre
dans aucune des quatre exceptions du protocole. Je le pose donc **sans bloquer** —
le reste de la tâche 7 est livré, et le déclencheur copier-coller est le seul des
cinq à rester en attente.

**La ligne attendue**, au choix :

1. « hook clavier, installé pendant l'épisode seulement » → je l'implémente, avec
   le test qui prouve qu'aucune touche n'est lue ;
2. « pas de hook » → j'implémente l'heuristique `Text_TextChanged` et j'assume
   ses faux négatifs, documentés ;
3. « laisse tomber ce déclencheur » → quatre déclencheurs sur cinq, noté au
   jalon.

En attendant, la copie est détectée, le collage ne l'est pas, et le journal le
dit plutôt que de faire comme si.

### Réponse de l'opérateur, 2026-08-27

> « hook clavier, installé pendant l'épisode seulement »

**Option 1 retenue.** Le hook `WH_KEYBOARD_LL` est posé à l'ouverture de
l'épisode et retiré à sa clôture. Hors épisode, il n'existe pas — ce qui rejoint
R1.2 : sans épisode ouvert, aucune capture, d'aucune sorte.

**Les garanties qui accompagnent la capacité.** Puisqu'on s'octroie de voir
toutes les frappes du poste, ce qu'on en fait doit être vérifiable :

1. **Aucune touche n'est enregistrée.** La procédure de hook ne fait qu'une
   chose : comparer le code de touche aux quatre combinaisons qui l'intéressent,
   et incrémenter un compteur. Elle n'écrit rien, ne transmet rien, ne garde rien.
2. **La décision est pure et testée** — `geste_de(vk, ctrl, shift)` vit hors du
   code Windows, et une table de tests énumère ce qui compte comme copie, comme
   collage, et surtout **tout ce qui ne compte pour rien**.
3. **Le hook ne vit que pendant l'épisode**, et sa pose comme sa dépose sont
   journalisées : l'opérateur peut vérifier dans le journal qu'il n'a pas traîné.
4. **Quatre combinaisons, pas une de plus** : `Ctrl+C`, `Ctrl+X`, `Ctrl+V`,
   `Maj+Inser`. Tout le reste traverse sans être regardé.

**Ce que le hook N'a PAS eu le droit de simplifier.** On aurait pu détecter les
copies uniquement par le numéro de séquence du presse-papiers, sans hook. Mais ce
numéro change aussi quand une AUTRE application écrit — un gestionnaire de mots
de passe, précisément. Lire le contenu sur ce seul signal violerait R2.3. Le hook
sert donc aussi à savoir que **c'est l'opérateur** qui a copié, avant toute
lecture.

**Reste ouvert, et rattaché à la tâche 9** : le filtre « surfaces activées » de
R5.4. Aujourd'hui la copie est lue dès qu'un `Ctrl+C` est observé pendant un
épisode ; il manque la vérification que la fenêtre au premier plan fait partie
des surfaces autorisées.

## 2026-08-27 — D28 : hors périmètre, on compte, on ne raconte pas

**Contexte.** La tâche 9 de la spec 002 pose R5.4 : « LE SYSTÈME NE DOIT capturer
que sur les surfaces explicitement activées par l'opérateur ». L'exigence dit ce
qu'il ne faut pas capturer. Elle ne dit pas ce qu'il faut écrire *à la place* —
et la règle 4 du projet, elle, interdit les trous silencieux.

Les deux se tirent dessus. Si un épisode ne dit rien du temps passé hors des
surfaces observées, il se présente comme continu alors qu'il ne l'est pas : un
champ se retrouve rempli d'une valeur venue de nulle part, et le rejeu ne peut
pas l'expliquer. Mais si l'épisode déclare « à 10 h 03, l'opérateur a fait
quelque chose dans une application non observée », il vient d'observer ce que la
liste blanche existe pour ne pas observer.

**Arbitrage.** L'épisode porte le **nombre** d'actions refusées, et rien d'autre.

- Le journal écrit une entrée `hors_perimetre { combien }` par **plage
  contiguë** — pas une par action. Dix minutes dans une application non activée
  produiraient des milliers de lignes disant chacune la même chose.
- L'entrée va au journal, pas en mémoire : après un crash, un épisode réassemblé
  doit encore dire qu'il n'a pas tout vu.
- Le compte remonte dans `completeness.out_of_scope`, un champ que la spec 001
  avait défini et que rien n'alimentait. Il n'y avait pas besoin d'une huitième
  cause de trou : le format savait déjà nommer ça.
- Ni horodatage individuel, ni nom d'application, ni nature du geste. Le nombre
  dit « je n'ai pas tout vu » ; les bornes diraient « voici quand il était
  ailleurs », ce qui est précisément la surveillance que la liste blanche évite.

**Trois exceptions, et leurs raisons.**

1. **La veille et le réveil** ne sont pas gouvernés par la liste blanche. Ce sont
   des faits de la machine, pas d'une application. Les refuser ferait disparaître
   les trous de veille que R3.3 exige — un trou perdu, exactement ce que la règle
   4 interdit.
2. **La bascule d'application entre toujours**, mais sa destination est remplacée
   par la constante `hors-perimetre` quand elle sort du périmètre. Ce que le
   journal a le droit de savoir : l'opérateur a quitté la surface observée. Ce
   qu'il n'a pas à savoir : où il est allé. La refuser tout court coûterait le
   déclencheur « bascule avec retour » — sans l'aller, jamais de retour.
3. **Deux applications non observées portent le même nom.** Sinon le journal
   reconstitue par recoupement ce que la liste blanche lui interdit de nommer :
   « il alterne entre deux applications » en dit déjà trop. Effet de bord voulu :
   un détour par deux applications au lieu d'une ne casse plus le déclencheur de
   retour, alors qu'il le cassait avant.

**Ce que cette tâche a corrigé au passage.** `Moteur::traiter()` ne consultait
jamais `pause_depuis`. Le journal écrivait un trou disant « rien n'a été capturé
ici » pendant que les événements continuaient d'entrer. Le journal mentait — un
défaut introduit en tâche 5 et invisible parce qu'aucun test ne vérifiait
l'absence d'écriture, seulement la présence du trou.

**Reste ouvert.** `completeness.out_of_scope` n'entre pas dans le grade. Un
épisode de deux actions avec quarante refus est aujourd'hui gradé comme un
épisode de deux actions sans refus. Le seuil de grade appartient à la spec 001 et
il est miroité en dix vecteurs ; le modifier est une décision de format, pas une
correction. À trancher au gate de la 002.

---

## 2026-08-27 — D29 : la bibliothèque de motifs passe en v4, et le juge cesse de s'auto-valider

**Contexte.** Une revue adverse (trente agents, cinq angles, réfutation
contradictoire) a rendu quinze trouvailles confirmées sur le capteur. La
première touche la règle 1.

**Trois graphies fuyaient**, vérifiées par exécution sur `motifs.json` v3 :

```
"Rappeler au +33 (0)6 12 34 56 78"  -> AUCUN
"0033 6 12 34 56 78"                -> AUCUN
"06<U+00A0>12<U+00A0>34<U+00A0>56<U+00A0>78" -> AUCUN
"+33 6 12 34 56 78"                 -> TEL_FR   (contrôle)
```

`+33 (0)X` est la graphie d'affichage standard en France : en-têtes de courriel,
signatures, cartes de visite — donc titres de fenêtre et noms accessibles. Après
`+33`, la parenthèse cassait la branche ; le `0` de `(0)` était suivi de `)` là
où le motif attend `[1-9]`.

L'insécable est de la même famille mais se corrige ailleurs : les motifs sont
compilés en ASCII des deux côtés — `unicode(false)` en Rust, `\d` ASCII en
JavaScript — précisément pour que les deux moteurs lisent la même chaîne de la
même façon. Le prix de cette garantie, c'est qu'un `U+00A0` n'est pas un
séparateur reconnu. Word, les signatures et beaucoup de champs de CRM en
produisent.

**Détail qui compte : le générateur de vecteurs interdisait de tester la
classe.** Il refusait tout vecteur non-ASCII, au motif que les index TypeScript
(UTF-16) et Rust (octets) ne seraient pas comparables. La règle protégeait la
comparabilité en supprimant le cas à comparer. Elle porte désormais sur la forme
**normalisée**, qui est ASCII dès que l'entrée ne portait que des blancs
exotiques.

**Décision.**

1. `TEL_FR` accepte `(0)` et `0033`. La normalisation du jeton ôte l'indicatif
   *puis* le zéro de conduite, dans cet ordre : sans ça, « +33 (0)6 … » et
   « +33 6 … » donneraient deux jetons pour le même numéro, donc une jointure
   perdue.
2. Une normalisation des blancs (`normaliser_blancs` / `normaliserBlancs`)
   s'applique **avant** toute recherche, des deux côtés. Les bornes rendues
   portent sur le texte normalisé ; qui remplace normalise d'abord.
3. **Le juge R4.6 ne s'appuie plus uniquement sur la bibliothèque qu'il
   valide.**

**Sur ce troisième point, qui est le vrai sujet.** R4.6 validait la redaction en
cherchant des PII avec `MOTIFS_PII` — la bibliothèque même qui avait servi à
redacter. Un juge adossé à ce qu'il contrôle est aveugle par construction : tout
trou de motif passe deux fois, à l'écriture puis à la validation, et l'épisode
ressort gradé « rédaction validée ». C'est arrivé trois fois : D24, puis les deux
graphies ci-dessus. La règle 2 dit que seul le juge mécanique promeut ; un juge
qui ne peut pas voir la classe d'erreur la plus fréquente ne promeut rien, il
tamponne.

Le filet (`MOTIFS_COMPACT`) applique un motif au texte **compacté** — tout ce qui
n'est ni alphanumérique ni `+` est ôté. N'importe quelle graphie d'un numéro
français s'y réduit à la même suite de chiffres, y compris celles que personne
n'a encore imaginées.

Deux garde-fous sur le filet, tous deux nécessaires :

- **Il ne redacte jamais.** Un filet qui remplacerait pseudonymiserait des
  montants et des références, et abîmerait des données que la spec 003 doit
  pouvoir comparer. Il refuse de valider, ce qui est le bon sens de l'erreur.
- **Il s'applique champ par champ, jamais sur l'épisode sérialisé.** En
  compactant un JSON entier, les chiffres de deux champs voisins se colleraient
  et fabriqueraient des numéros que personne n'a écrits. Un faux positif ici
  déclasse un épisode honnête sans recours.

Le jour où le filet parle seul, c'est la bibliothèque qu'il faut corriger — pas
lui qu'il faut taire.

## 2026-08-27 — D30 : un seul chemin de clôture, une seule origine de temps

Trois trouvailles de la revue adverse qui n'en font qu'une : le capteur avait
**deux chemins de clôture**, et le second était appauvri.

**Ce que la clôture automatique de R1.3 ne faisait pas.** `verifier_timeout`
posait `clos` et poussait `Gap{timeout}` + `ClotureAuto` dans le tampon mémoire.
Puis `clore()` commençait par `if self.clos { return; }` — donc `Journal::clore()`
n'était jamais atteint : ni vidage du tampon, ni `sync_all`, ni retrait du
marqueur `.ouvert`. Le journal n'a pas de `Drop`. La branche du battement se
contentait ensuite d'arrêter la session et de notifier : aucun assemblage,
aucune quarantaine, et ni l'abonnement UIA ni le hook clavier relâchés.

Une heure de travail ne produisait donc aucun `episode.json`, la vue ne le
listait pas — son commentaire disait même « son absence de la liste est le
signal », ce qui est exactement le trou silencieux que la règle 4 interdit — et
la capture continuait de tourner sur un épisode qui n'existait plus.

**Décision.** `clore_episode(app, cause)` est l'unique chemin ; `arreter()` n'en
est qu'un appel. La cause ne change que le message. Et `Moteur::clore()` devient
réentrante : `clos` dit que plus rien n'entre, `journal_clos` dit que le fichier
est fermé. Les confondre coûtait un épisode entier.

**La reprise après crash n'était pas branchée non plus.** `journal::orphelins` et
`clore_orphelin` n'avaient d'autre appelant que le binaire de banc, et `main.rs`
ne lit pas `argv`. Le kill-test validait la fonction, pas son branchement. Après
un vrai crash, le dossier restait marqué `.ouvert` indéfiniment.

Et `clore_orphelin` s'arrêtait au `gap{crash}` : la seconde moitié de R3.2 — « le
passer au pipeline de clôture normal » — n'existait nulle part, faute de savoir
quoi assembler. `assembler` a besoin du `task_slug` et de la borne murale
d'ouverture, qui ne vivaient que dans la `Session`, donc en mémoire, donc perdues
au crash.

**Le marqueur porte désormais l'identité de l'épisode** — `{episode_id,
task_slug, t0_mural_ms}` — écrite avant la première ligne du journal, comme le
marqueur qu'elle remplace. Un marqueur d'une version antérieure reste lisible
comme marqueur : l'épisode est clos et signalé, mais pas assemblé. On ne devine
pas une tâche.

**Deux origines de temps monotone dans le même journal.** La boucle UIA datait
ses événements depuis un `Instant::now()` pris à l'abonnement ; le moteur, la
veille, la pause et le presse-papiers datent depuis le lancement du processus.
Sur une application ouverte depuis dix minutes, une frappe arrivait à
`monotone_ms = 300` alors que le moteur en était à 600 000 : le délai
d'inactivité de 2 s partait après 1 s, sur chaque frappe isolée, chacune coûtant
une photo de 50 Ko.

La source porte maintenant l'horloge du processus. Et le moteur **rebase** tout
ce qui entre au journal sur son propre `t0` : l'assemblage documentait attendre
« un instant monotone depuis l'ouverture », les deux ne coïncidaient que dans les
tests, qui ouvrent le moteur à `t0 = 0`. Sans rebasage, tous les gaps d'un
épisode ouvert tard ressortaient horodatés à `t1`, écrasés par le `min` de
l'assemblage.

Aucun test ne pouvait le voir. Deux tests ouvrent désormais le moteur à un `t0`
non nul — c'est le seul moyen de rendre la classe détectable.

---

## 2026-08-27 — D31 : le jeton passe à 65 bits, et change d'alphabet

**Ce qui l'a déclenché.** Le banc de non-collision a rougi : une collision sur
dix mille valeurs. Il tire une clé neuve à chaque exécution — il a fini par
tomber sur le cas.

Le commentaire de `LONGUEUR_CONDENSAT` annonçait un risque « sensible autour de
2^16 ≈ 65 000 entités ». Le calcul était faux d'un ordre de grandeur utile : le
paradoxe des anniversaires donne **1,2 % de chance de collision sur 10 000
valeurs** en 32 bits, et 29 % sur 50 000.

**Pourquoi ça compte plus qu'une jointure perdue.** Une collision n'en fait pas
perdre une : elle en **invente** une. Deux personnes différentes reçoivent le même
pseudonyme et fusionnent dans le graphe d'entités. C'est l'erreur que la
normalisation refuse explicitement de commettre — « mieux vaut deux jetons pour
une entité qu'un jeton pour deux entités » — commise à l'autre bout de la chaîne.

**Décision : treize caractères base32, soit 65 bits.** La probabilité de
collision sur dix mille tombe sous 3 × 10⁻¹⁵.

**Base32 et non hexadécimal**, et ce second point n'est pas cosmétique. Un
condensat hexadécimal de seize caractères a environ une chance sur cent soixante
de contenir une suite de dix chiffres — donc de ressembler à un numéro pour les
motifs de la v4 et pour le filet du juge. Avec des centaines de jetons par
corpus, ça déclasserait un épisode honnête toutes les quelques dizaines de
jetons, sans recours.

L'alphabet RFC 4648 minuscule (`a-z2-7`) ne contient ni `0` ni `1`. Aucun jeton
ne peut donc commencer une graphie téléphonique française, qui exige `0`, `+33`
ou `0033`. La garantie est structurelle, pas probabiliste, et un test l'écrit
comme telle.

## 2026-08-27 — D32 : la liste blanche gouverne aussi les photos

Dernière trouvaille de la revue adverse, et la plus dérangeante des quatre qui
touchent R5.4 : **la liste blanche gardait les actions et laissait passer les
photos.**

`photographier_actif` prenait ce que `get_focused_element()` rendait, sans jamais
demander sur quoi le focus se trouvait. Aucun paramètre de surface, ni dans cette
fonction, ni dans `Moteur::photographier`, ni dans `snapshot::construire`.

**Le chemin n'a rien de théorique.** Un `Focus` venu d'une application non
activée est refusé par `admissible()` **avant** le `match` de `traiter` — donc
`derniere_saisie` n'est jamais remis à zéro. Deux secondes plus tard,
`verifier_inactivite` déclenche `SaisiePuisInactivite` et l'arbre est descendu
sur l'application où l'opérateur se trouve alors : jusqu'à 1500 nœuds, avec leurs
rôles, leurs noms accessibles **et** leurs `ValueValue`. La rédaction ne rattrape
que les motifs ; un libellé ou un contenu de champ hors motif reste en clair.

Plus large que le seul cas d'inactivité : les événements sont drainés une fois par
seconde, donc les quatre autres déclencheurs photographient eux aussi la fenêtre
focalisée jusqu'à une seconde après le geste.

**Décision.** La liste blanche voyage avec la demande de photo, parce que seul le
fil UIA peut savoir sur quoi le focus se trouve à l'instant de la photo — et que
c'est cet instant-là qui compte, pas celui du déclencheur. La vérification a lieu
**avant** de descendre dans l'arbre.

**Trois issues et non deux.** « Pas de photo » ne disait pas pourquoi, et les
deux raisons n'ont pas le même sens : un bureau muet est un incident technique,
un focus hors périmètre est une règle qui s'applique. Les confondre empêcherait
de savoir si R5.4 tient. Les refus sont comptés et dits à la clôture — un épisode
dont la moitié des photos manquent ne se lit pas comme un épisode complet.

Le déclencheur, lui, reste consigné. C'est la doctrine d'avant : mieux vaut un
déclencheur sans photo qu'une photo qu'on n'avait pas le droit de prendre.

## 2026-08-27 — D33 : la partition des sources n'était qu'une intention

D19 écrivait la règle : `UiaSource` prend toutes les applications natives,
`DomSource` toutes les surfaces navigateur, et **pas de bascule dynamique sur une
même surface**. Le code la respectait dans son intention et pas dans son effet :
l'abonnement UIA est **global filtré**, il voit le navigateur comme le reste.

**La première capture réelle l'a montré, et pas un seul test.** Un épisode de
1960 événements dont les `scope_fields` étaient « about:blank - Google Chrome »,
« Barre d'adresse et de recherche », « about:blank - Utilisation de la mémoire —
19,4 Mo ». Le travail de l'opérateur était noyé dans la chrome du navigateur, et
chaque geste dans une page comptait **deux fois** — une par le DOM, une par UIA.

La conséquence dépasse le bruit. La spec 004 mesure l'accord entre ce qu'un agent
ferait et ce que l'humain a fait ; un dénominateur doublé sur toutes les surfaces
navigateur aurait faussé cette mesure d'un facteur deux, sans que rien ne le
signale.

**Décision.** `surfaces::classe()` range chaque surface, et `Moteur::admissible`
exige que la source corresponde à la classe. Le croisement est refusé et compté
au hors-périmètre : ce n'est pas une perte, c'est l'autre source qui a la charge.

Deux points de méthode, parce qu'ils se reproduiront :

- **La liste des navigateurs est une liste, pas une heuristique.** Deviner « c'est
  sûrement un navigateur » sur un nom de processus se tromperait dans les deux
  sens, et les deux coûtent.
- **Une surface inconnue est native.** L'erreur n'est pas symétrique : ranger un
  navigateur inconnu en natif fait capturer sa chrome, ce qui est du bruit qu'on
  voit ; ranger une application native en navigateur la rendrait invisible, ce qui
  est une perte qu'on ne voit pas.

Après correction, le même protocole donne exactement 540 événements pour 45
répétitions de 12 observations, et des `scope_fields` réduits aux vrais champs :
« Description », « Statut de la piste », « Enregistrer ».

## 2026-08-27 — D34 : l'INVARIANT 7 s'allume, et il fallait un test pour s'en apercevoir

D5 avait ajouté la confirmation API au grade A, puis l'avait **neutralisée** par
une constante, `CONFIRMATION_API_VERIFIABLE = false`, avec une raison solide :
aucun connecteur n'existait, et exiger des `api_refs` que rien ne pouvait
produire aurait interdit le grade A à tout le corpus. Un invariant qu'on ne peut
pas satisfaire ne protège de rien — il se contourne.

La spec 003 fournit le connecteur. L'exigence redevient atteignable, et R7.1 la
réclame : un épisode n'est A que si toutes ses entités pointent vers de vrais
enregistrements. Un A sans `api_refs`, c'est un épisode qui affirme avoir tout
expliqué sans avoir rien vérifié.

**Décision.** La constante passe à `true`, des deux côtés du miroir — capteur
Rust et harness TypeScript. Elles doivent basculer ensemble : un capteur qui
graderait A ce que le harness grade B produirait des épisodes que le juge refuse,
sans que personne comprenne pourquoi.

**Ce que le basculement a appris.** Il n'a rien fait rougir. Trois cent
vingt-deux tests Rust, deux cent quarante-cinq TypeScript, et pas un ne
distinguait un épisode avec `api_refs` d'un épisode sans. L'invariant était
décoratif depuis sa création : on pouvait le mettre à `true` ou à `false` sans
qu'aucun banc ne s'en aperçoive.

Il est maintenant gardé par un test de **comportement** des deux côtés — le même
épisode, `api_refs` vidées, tombe en B avec sa raison. Pas par une assertion sur
la constante : clippy refuse une assertion dont il connaît déjà l'issue, et il a
raison de la refuser. Un test qui ne peut pas échouer ne garde rien.

C'est la troisième fois de la journée qu'un garde-fou s'avère décoratif —
après le compteur de preuve qui annonçait 148 tests sur 247, et le script
d'empreinte qui rendait « DANS LE BUDGET » en n'ayant rien mesuré. La leçon se
répète : **un contrôle doit être vu échouer au moins une fois.**
