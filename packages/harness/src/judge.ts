import type { Episode, FlatState, ToolCall } from '@noe/episode-spec';

/**
 * Le juge mécanique. Aucune sémantique, aucun modèle (R4.5) : une comparaison
 * d'états normalisée, et rien d'autre. « Ça marche » devient une mesure.
 */

export type Valeur = string | number | boolean | null;

/** Une date « parsable » doit d'abord ressembler à une date. Date.parse est trop laxiste. */
const RESSEMBLE_A_UNE_DATE =
  /^(\d{4}-\d{2}-\d{2}([T ]|$)|\d{1,2}\/\d{1,2}\/\d{4}|[A-Za-z]{3,9}\s+\d{1,2},?\s+\d{4})/;

const EST_NUMERIQUE = /^[+-]?\d+(\.\d+)?$/;

/**
 * Normalisation avant comparaison (R4.1).
 *
 * `null` ≡ champ absent ≡ chaîne vide ; espaces rognés ; CRLF unifiés ; dates en
 * ISO-8601 UTC ; nombres comparés en valeur, jamais en chaîne.
 *
 * L'ordre compte : « 42 » est un nombre, pas une date. Tester le numérique en
 * premier évite qu'un moteur JS laxiste n'en fasse un horodatage.
 */
export function normalize(v: unknown): Valeur {
  if (v === null || v === undefined) return null;
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return Number.isFinite(v) ? v : null;
  if (typeof v !== 'string') return null;

  const s = v.replace(/\r\n/g, '\n').trim();
  if (s === '') return null;

  if (EST_NUMERIQUE.test(s)) return Number(s);

  if (RESSEMBLE_A_UNE_DATE.test(s)) {
    const t = Date.parse(s);
    if (!Number.isNaN(t)) return new Date(t).toISOString();
  }

  return s;
}

/** Deux valeurs sont équivalentes si elles le sont après normalisation. */
export function equivalent(a: unknown, b: unknown): boolean {
  return normalize(a) === normalize(b);
}

// ---------------------------------------------------------------------------
// Classement
// ---------------------------------------------------------------------------

export const CLASSES = ['accord', 'desaccord', 'manque', 'excedent', 'hors_perimetre'] as const;

export type Classe = (typeof CLASSES)[number];

export type ChampJuge = {
  readonly cible: string;
  readonly champ: string;
  readonly classe: Classe;
  readonly propose: Valeur;
  readonly observe: Valeur;
};

export type VerdictEpisode = {
  readonly episode_id: string;
  readonly task_slug: string;
  readonly grade: 'A' | 'B' | 'C';
  /** Seuls les grades A comptent dans les agrégats (R2.2). */
  readonly compte_dans_stats: boolean;
  readonly verdict: 'accord' | 'desaccord';
  readonly champs: readonly ChampJuge[];
  readonly totaux: Readonly<Record<Classe, number>>;
};

/** Identifiant stable d'une cible d'écriture. Sert de clé de rapprochement. */
function cleCible(connector: string, object: string, id: string): string {
  return `${connector}/${object}/${id}`;
}

/**
 * Le diff réellement observé : pour chaque entité, les champs dont la valeur
 * normalisée a changé entre `state_before` et `state_after`.
 *
 * Une entité non résolue (état manquant) ne produit aucun diff : c'est déjà ce
 * qui l'a fait déclasser en B, on ne l'invente pas ici.
 */
export function diffObserve(ep: Episode): Map<string, Map<string, Valeur>> {
  const parCible = new Map<string, Map<string, Valeur>>();

  for (const entite of ep.entities) {
    const avant: FlatState = entite.state_before ?? {};
    const apres: FlatState | undefined = entite.state_after;
    if (apres === undefined || entite.state_before === undefined) continue;

    const champs = [...new Set([...Object.keys(avant), ...Object.keys(apres)])].sort();
    const modifies = new Map<string, Valeur>();
    for (const champ of champs) {
      if (!equivalent(avant[champ], apres[champ])) {
        modifies.set(champ, normalize(apres[champ]));
      }
    }
    if (modifies.size === 0) continue;

    for (const ref of entite.api_refs) {
      parCible.set(cleCible(ref.connector, ref.object, ref.id), modifies);
    }
  }

  return parCible;
}

/** Les écritures proposées, regroupées par cible. */
function proposeParCible(calls: readonly ToolCall[]): Map<string, Map<string, Valeur>> {
  const parCible = new Map<string, Map<string, Valeur>>();
  for (const c of calls) {
    const cle = cleCible(c.connector, c.object, c.object_id);
    const existant = parCible.get(cle) ?? new Map<string, Valeur>();
    for (const champ of Object.keys(c.fields).sort()) {
      existant.set(champ, normalize(c.fields[champ]));
    }
    parCible.set(cle, existant);
  }
  return parCible;
}

/**
 * Rend le verdict d'un épisode (R4.2, R4.3).
 *
 * Un champ hors `scope_fields` est compté à part et **ne pèse jamais** sur le
 * verdict : la tâche n'a pas à répondre de ce qu'elle n'est pas censée toucher.
 */
export function juger(ep: Episode, calls: readonly ToolCall[]): VerdictEpisode {
  const observes = diffObserve(ep);
  const proposes = proposeParCible(calls);
  const perimetre = new Set(ep.scope_fields);

  const cibles = [...new Set([...observes.keys(), ...proposes.keys()])].sort();
  const champs: ChampJuge[] = [];

  for (const cible of cibles) {
    const obs = observes.get(cible) ?? new Map<string, Valeur>();
    const pro = proposes.get(cible) ?? new Map<string, Valeur>();
    const tous = [...new Set([...obs.keys(), ...pro.keys()])].sort();

    for (const champ of tous) {
      const aObserve = obs.has(champ);
      const aPropose = pro.has(champ);
      const observe = obs.get(champ) ?? null;
      const propose = pro.get(champ) ?? null;

      if (!perimetre.has(champ)) {
        champs.push({ cible, champ, classe: 'hors_perimetre', propose, observe });
        continue;
      }

      let classe: Classe;
      if (aObserve && aPropose) classe = propose === observe ? 'accord' : 'desaccord';
      else if (aObserve) classe = 'manque';
      else classe = 'excedent';

      champs.push({ cible, champ, classe, propose, observe });
    }
  }

  const totaux = Object.fromEntries(CLASSES.map((c) => [c, 0])) as Record<Classe, number>;
  for (const c of champs) totaux[c.classe] += 1;

  const echecs = totaux.desaccord + totaux.manque + totaux.excedent;

  return {
    episode_id: ep.id,
    task_slug: ep.task_slug,
    grade: ep.grade,
    compte_dans_stats: ep.grade === 'A',
    verdict: echecs === 0 ? 'accord' : 'desaccord',
    champs,
    totaux,
  };
}
