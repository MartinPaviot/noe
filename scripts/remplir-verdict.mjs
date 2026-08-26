#!/usr/bin/env node
/**
 * Pré-remplit `docs/spike-verdict.md` depuis `spikes/capteur-uia/resultats/spike.json`.
 *
 * Le script propose une recommandation ; il ne signe pas le verdict. La ligne
 * « Signé » reste vide à dessein — un verdict est une décision, pas une sortie
 * de programme.
 */
import { readFileSync, writeFileSync } from 'node:fs';

const SRC = 'spikes/capteur-uia/resultats/spike.json';
const DEST = 'docs/spike-verdict.md';

/**
 * Seuils du PROMPT MAITRE, section « Jours 1-2 » — pas des seuils d'ingenieur :
 *   (a) >= 90 % des elements interagis produisent un couple role+nom stable
 *   (b) 100 % des actions qui changent l'etat apparaissent dans le flux
 *   (c) surcout CPU soutenu < 5 %
 * Ils avaient ete assouplis a 80/90 par erreur. Alignes (decisions.md, D8).
 */
const SEUIL_CPU_PCT = 5;
const SEUIL_STABILITE_PCT = 90;
const SEUIL_COUVERTURE_PCT = 100;

let brut;
try {
  brut = JSON.parse(readFileSync(SRC, 'utf8'));
} catch {
  console.error(`Introuvable : ${SRC}\nLancez d'abord le binaire du spike.`);
  process.exit(2);
}

const n = (v, d = 1) => (typeof v === 'number' ? v.toFixed(d) : '—');
const ok = (v, seuil, sens = '<') =>
  typeof v !== 'number' ? '?' : (sens === '<' ? v < seuil : v >= seuil) ? '✅' : '❌';

const phases = brut.phases ?? [];
const parNom = Object.fromEntries(phases.map((p) => [p.strategie, p]));
const globale = parNom.globale;
const focus = parNom.focus;

/** Recommande la stratégie : le CPU tranche, la stabilité départage. */
function recommander() {
  if (!globale || !focus) return { choix: '—', motif: 'une phase manque' };
  const gOk = globale.cpu_p95_fenetres_30s < SEUIL_CPU_PCT;
  const fOk = focus.cpu_p95_fenetres_30s < SEUIL_CPU_PCT;
  if (gOk && !fOk) return { choix: 'globale filtrée', motif: 'seule à tenir le budget CPU' };
  if (fOk && !gOk) return { choix: 'par conteneur au focus', motif: 'seule à tenir le budget CPU' };
  if (!gOk && !fOk) {
    return {
      choix: 'aucune en l état',
      motif: `les deux depassent ${SEUIL_CPU_PCT} % de CPU p95 — degradation obligatoire (R7.2)`,
    };
  }
  const ecart = focus.stabilite_signature_pct - globale.stabilite_signature_pct;
  if (Math.abs(ecart) < 5) {
    return {
      choix: 'globale filtrée',
      motif: 'budget tenu par les deux, stabilite equivalente : la plus simple gagne',
    };
  }
  return ecart > 0
    ? { choix: 'par conteneur au focus', motif: `signatures plus stables de ${n(ecart)} points` }
    : { choix: 'globale filtrée', motif: `signatures plus stables de ${n(-ecart)} points` };
}

const reco = recommander();

const ligne = (p) =>
  p
    ? `| **${p.strategie}** | ${n(p.stabilite_signature_pct)} % ${ok(p.stabilite_signature_pct, SEUIL_STABILITE_PCT, '>')} | ${n(p.couverture_etat_pct)} % ${ok(p.couverture_etat_pct, SEUIL_COUVERTURE_PCT, '>')} | ${n(p.cpu_p95_fenetres_30s, 2)} % ${ok(p.cpu_p95_fenetres_30s, SEUIL_CPU_PCT)} | ${n(p.ram_max_mo)} Mo | ${p.actions_etat}/${p.actions_etat_declarees} |`
    : '| — | — | — | — | — | — |';

const walker = (p) =>
  p
    ? `| **${p.strategie}** | ${p.walker_noeuds_p50} | ${p.walker_noeuds_p95} | ${p.walker_profondeur_max} | ${p.walker_duree_p95_ms} ms | ${n(p.walker_tronques_pct, 0)} % |`
    : '| — | — | — | — | — | — |';

const doc = `# Verdict du spike — capteur UIA

> **Pré-rempli automatiquement** depuis \`${SRC}\` le ${new Date().toISOString().slice(0, 10)}.
> Les chiffres viennent de la mesure ; la décision reste à signer.
>
> Application cible : **${brut.application_cible || '(non renseignée)'}**

## 1. Question tranchée

Laquelle des deux stratégies d'abonnement UIA tient le budget d'empreinte tout en
produisant des signatures de ciblage stables — et avec quels paramètres de walker.

## 2. Les trois nombres

| Stratégie | Stabilité rôle+nom | Couverture actions d'état | CPU p95 (fenêtres 30 s) | RAM max | Observé/déclaré |
| --- | --- | --- | --- | --- | --- |
${ligne(globale)}
${ligne(focus)}

Seuils : CPU < ${SEUIL_CPU_PCT} % (R7.1) · stabilité ≥ ${SEUIL_STABILITE_PCT} % · couverture ≥ ${SEUIL_COUVERTURE_PCT} %.

**Stabilité** = part des signatures \`rôle|nom\` d'actions d'état communes à
**toutes** les occurrences. Une signature qui n'apparaît que dans certaines
répétitions n'est pas un point d'ancrage fiable.

**Couverture** = actions d'état observées ÷ actions d'état déclarées par
l'opérateur à chaque occurrence.

## 3. Paramètres du walker

| Stratégie | Nœuds p50 | Nœuds p95 | Profondeur max | Durée p95 | Tronqués |
| --- | --- | --- | --- | --- | --- |
${walker(globale)}
${walker(focus)}

Budgets éprouvés : profondeur max **12**, nœuds max **1500**.

<!-- Si « tronqués » est élevé, le budget est trop serré pour cette application :
     remonter le plafond de noeuds et remesurer avant de conclure. -->

## 4. Recommandation du script

**Stratégie : ${reco.choix}** — ${reco.motif}.

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

- [ ] Inscrire la stratégie retenue dans \`specs/002-capture-bornee/design.md\` §2
- [ ] Inscrire les paramètres du walker dans le même §2
- [ ] Cocher la tâche 0 de \`specs/002-capture-bornee/tasks.md\`

---

**Date :** ${new Date().toISOString().slice(0, 10)}  ·  **Signé :** <!-- ton nom -->
`;

writeFileSync(DEST, doc, 'utf8');
console.log(`${DEST} pre-rempli.`);
for (const p of phases) {
  console.log(
    `  ${p.strategie.padEnd(8)} stabilite ${n(p.stabilite_signature_pct)} %  couverture ${n(p.couverture_etat_pct)} %  CPU p95 ${n(p.cpu_p95_fenetres_30s, 2)} %`,
  );
}
console.log(`\n  recommandation : ${reco.choix} — ${reco.motif}`);
