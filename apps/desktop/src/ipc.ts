/**
 * La frontière entre la vue et ce qui lui donne des données.
 *
 * En production, elle appelle Tauri et lit les **vrais** épisodes du poste. Sous
 * test visuel, elle lit des **fixtures versionnées** — c'est la seule façon
 * d'avoir des baselines stables : si les captures d'écran dépendaient de ce que
 * l'opérateur a capturé hier, aucune ne resterait valide plus d'une journée.
 *
 * Les deux ne s'opposent pas, elles ne servent pas à la même chose. L'une
 * montre la réalité, l'autre prouve la non-régression.
 */

export type ResumeEpisode = {
  readonly id: string;
  readonly task_slug: string;
  readonly t0: string;
  readonly t1: string;
  readonly grade: string;
  readonly grade_reason: string;
  readonly actions: number;
  readonly trous: number;
  readonly completude_pct: number;
  readonly scope_fields: readonly string[];
};

export type PointFrise = {
  readonly seq: number;
  readonly ts: string;
  readonly genre: 'action' | 'trou';
  readonly quoi: string;
  readonly cible: string;
  readonly region: string | null;
};

export type DetailEpisode = {
  readonly resume: ResumeEpisode;
  readonly frise: readonly PointFrise[];
};

type FenetreTauri = {
  readonly __TAURI__?: {
    readonly core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> };
  };
};

const tauri = () => (globalThis as unknown as FenetreTauri).__TAURI__?.core;

/**
 * L'état simulé demandé par un test visuel, via `?etat=`.
 *
 * Les quatre états exigés par D21 ne peuvent pas se produire à la demande sur de
 * vraies données : un disque plein d'épisodes ne sait pas être vide. Le
 * paramètre les rend atteignables, et il n'existe que hors Tauri — en
 * production, `tauri()` répond et rien ne le consulte.
 */
function etatSimule(): string | null {
  try {
    return new URLSearchParams(globalThis.location?.search ?? '').get('etat');
  } catch {
    return null;
  }
}

const jamais = <T>(): Promise<T> => new Promise<T>(() => {});

async function fixture<T>(nom: string): Promise<T> {
  const reponse = await fetch(`/fixtures/${nom}.json`);
  if (!reponse.ok) throw new Error(`fixture ${nom} introuvable`);
  return (await reponse.json()) as T;
}

export async function listerEpisodes(): Promise<readonly ResumeEpisode[]> {
  const noyau = tauri();
  if (noyau) return noyau.invoke<ResumeEpisode[]>('lister_episodes');

  switch (etatSimule()) {
    case 'vide':
      return [];
    case 'erreur':
      throw new Error('le dossier d episodes est illisible');
    // Une promesse qui ne se résout jamais : c'est exactement ce qu'un
    // chargement lent est, et ça rend l'état capturable sans minuterie.
    case 'chargement':
      return jamais<readonly ResumeEpisode[]>();
    default:
      return fixture<ResumeEpisode[]>('episodes');
  }
}

export async function detailEpisode(id: string): Promise<DetailEpisode | null> {
  const noyau = tauri();
  if (noyau) return noyau.invoke<DetailEpisode | null>('detail_episode', { id });

  const details = await fixture<Record<string, DetailEpisode>>('details');
  return details[id] ?? null;
}
