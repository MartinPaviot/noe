# Verdict du spike — capteur UIA

> **Pré-rempli automatiquement** depuis `spikes/capteur-uia/resultats/spike-{globale,focus}.json` le 2026-08-26.
> Les chiffres viennent de la mesure ; la décision reste à signer.
>
> Application cible : **Salesforce Lightning (org de demo)**

## 1. Question tranchée

Laquelle des deux stratégies d'abonnement UIA tient le budget d'empreinte tout en
produisant des signatures de ciblage stables — et avec quels paramètres de walker.

## 2. Les trois nombres

| Stratégie | Stabilité rôle+nom | Couverture actions d'état | CPU p95 (fenêtres 30 s) | RAM max | Observé/déclaré |
| --- | --- | --- | --- | --- | --- |
| **globale** | 37.0 % ❌ | 100.0 % ✅ | 4.39 % ✅ | 19.7 Mo | 187/15 |
| **focus** | 0.0 % ❌ | 0.0 % ❌ | 5.75 % ❌ | 19.3 Mo | 0/15 |

Seuils : CPU < 5 % (R7.1) · stabilité ≥ 90 % · couverture ≥ 100 %.

**Stabilité** = part des signatures `rôle|nom` d'actions d'état communes à
**toutes** les occurrences. Une signature qui n'apparaît que dans certaines
répétitions n'est pas un point d'ancrage fiable.

**Couverture** = actions d'état observées ÷ actions d'état déclarées par
l'opérateur à chaque occurrence.

## 3. Paramètres du walker

| Stratégie | Nœuds p50 | Nœuds p95 | Profondeur max | Durée p95 | Tronqués |
| --- | --- | --- | --- | --- | --- |
| **globale** | 0 | 0 | 0 | 0 ms | 0 % |
| **focus** | 1 | 228 | 9 | 429 ms | 0 % |

Budgets éprouvés : profondeur max **12**, nœuds max **1500**.

<!-- Si « tronqués » est élevé, le budget est trop serré pour cette application :
     remonter le plafond de noeuds et remesurer avant de conclure. -->

## 4. Recommandation du script

**Stratégie : globale filtrée** — seule à tenir le budget CPU.

> Ce n'est qu'une lecture mécanique des seuils. Si elle contredit ce que tu as
> observé pendant la session, c'est ton observation qui tranche : note pourquoi
> juste en dessous.

**Ce que je retiens :**
<!-- à remplir -->

## 5. Ce que ce spike n'affirme pas

- Une seule application cible, un seul poste, un seul opérateur.
- La stabilité est mesurée sur des occurrences **consécutives** : elle ne dit rien
  de la résistance à une mise à jour de l'application.
- La couverture dépend d'un comptage déclaratif, donc faillible.

## 6. Conséquences

- [ ] Inscrire la stratégie retenue dans `specs/002-capture-bornee/design.md` §2
- [ ] Inscrire les paramètres du walker dans le même §2
- [ ] Cocher la tâche 0 de `specs/002-capture-bornee/tasks.md`

---

**Date :** 2026-08-26  ·  **Signé :** <!-- ton nom -->
