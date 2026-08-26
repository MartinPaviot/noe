import { createHash } from 'node:crypto';
import type { Valeur } from './judge.js';
import type { RapportRejeu } from './replay.js';

/**
 * INVARIANT I, au point exact où les données quittent le processus.
 *
 * Un rapport ne doit jamais contenir de contenu utilisateur. Les chaînes en
 * sortent donc sous forme d'empreinte : on voit toujours si deux valeurs sont
 * égales — ce qu'un diff exige — sans jamais voir la valeur.
 *
 * Les nombres et booléens passent en clair : ils sont structurels (un compteur,
 * un drapeau) et leur cardinalité les rend inexploitables comme fuite.
 */
export function empreinte(v: Valeur): string | number | boolean | null {
  if (v === null) return null;
  if (typeof v === 'number' || typeof v === 'boolean') return v;
  const h = createHash('sha256').update(v, 'utf8').digest('hex').slice(0, 10);
  return `sha256:${h}`;
}

/** Applique l'empreinte à toutes les valeurs de champ d'un rapport. */
function rediger(rapport: RapportRejeu): RapportRejeu {
  return {
    ...rapport,
    episodes: rapport.episodes.map((ep) => ({
      ...ep,
      champs: ep.champs.map((c) => ({
        ...c,
        propose: empreinte(c.propose) as Valeur,
        observe: empreinte(c.observe) as Valeur,
      })),
    })),
  };
}

/**
 * Rapports (R4.4). Deux formats : `json` pour la machine, texte pour l'humain.
 *
 * Le JSON est **stable** : clés triées, aucun horodatage dans le corps. L'en-tête
 * `genere_le` est isolé, pour que la comparaison octet à octet du test de
 * déterminisme (R3.3) puisse l'exclure sans exclure autre chose.
 */

/** Sérialisation à clés triées : deux objets équivalents donnent le même texte. */
export function stringifyStable(valeur: unknown): string {
  const trier = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(trier);
    if (v !== null && typeof v === 'object') {
      const o = v as Record<string, unknown>;
      return Object.fromEntries(
        Object.keys(o)
          .sort()
          .map((k) => [k, trier(o[k])]),
      );
    }
    return v;
  };
  return JSON.stringify(trier(valeur), null, 2);
}

/** Corps du rapport, sans aucun horodatage. C'est lui que le déterminisme couvre. */
export function rapportJson(rapport: RapportRejeu): string {
  return stringifyStable(rediger(rapport));
}

function afficher(v: Valeur): string {
  const e = empreinte(v);
  if (e === null) return '∅';
  return String(e);
}

const SYMBOLE: Record<string, string> = {
  accord: '✓',
  desaccord: '✗',
  manque: '−',
  excedent: '+',
  hors_perimetre: '·',
};

/** Rapport texte, en colonnes. Destiné à être lu, pas parsé. */
export function rapportTexte(rapport: RapportRejeu): string {
  const l: string[] = [];
  const a = rapport.agregat;

  l.push(`corpus    ${rapport.corpus}`);
  l.push(`politique ${rapport.politique}`);
  l.push('');

  for (const ep of rapport.episodes) {
    const marque = ep.verdict === 'accord' ? '✓' : ep.verdict === 'non_jugeable' ? '~' : '✗';
    const exclu = ep.compte_dans_stats ? '' : '   [exclu des stats]';
    l.push(`${marque} ${ep.episode_id}  ${ep.task_slug}  grade ${ep.grade}${exclu}`);
    if (!ep.jugeable) l.push('    ~ aucun etat API a juger (entites non resolues)');

    for (const c of ep.champs) {
      if (c.classe === 'accord') continue;
      const s = SYMBOLE[c.classe] ?? '?';
      l.push(
        `    ${s} ${c.champ.padEnd(20)} ${c.classe.padEnd(15)} propose=${afficher(c.propose).padEnd(24)} observe=${afficher(c.observe)}`,
      );
    }
  }

  if (rapport.illisibles.length > 0) {
    l.push('');
    l.push(`illisibles (${rapport.illisibles.length})`);
    for (const i of rapport.illisibles) l.push(`    ! ${i.fichier} — ${i.erreur}`);
  }

  l.push('');
  l.push('─'.repeat(64));
  l.push(
    `episodes ${a.n_total}   comptes ${a.n_comptes}   exclus ${a.n_exclus}   non jugeables ${a.n_non_jugeables}   accord ${a.n_accord}/${a.n_comptes}  (${a.taux_accord} %)`,
  );
  l.push(
    `accord ${a.par_classe.accord}   desaccord ${a.par_classe.desaccord}   manque ${a.par_classe.manque}   excedent ${a.par_classe.excedent}   hors_perimetre ${a.par_classe.hors_perimetre}`,
  );

  if (a.champs_en_echec.length > 0) {
    l.push('');
    l.push('champs en echec');
    for (const c of a.champs_en_echec.slice(0, 10)) {
      l.push(`    ${String(c.n).padStart(4)}  ${c.champ}`);
    }
  }

  return `${l.join('\n')}\n`;
}
