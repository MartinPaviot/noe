import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { type Episode, load } from '@noe/episode-spec';
import { CLASSES, type Classe, juger, type VerdictEpisode } from './judge.js';
import { contexteDe, type Policy } from './policy.js';

export type EpisodeIllisible = { readonly fichier: string; readonly erreur: string };

/**
 * Fichiers qui cohabitent avec le corpus sans en faire partie. Les exclure
 * explicitement vaut mieux que de les laisser échouer au parse : un « illisible »
 * doit signaler un vrai problème, pas un voisin de dossier.
 */
const SIDECARS = new Set(['canaris.json']);

export type Agregat = {
  /** Épisodes chargés, tous grades confondus. */
  readonly n_total: number;
  /** Épisodes de grade A — les seuls qui comptent (R2.2). */
  readonly n_comptes: number;
  readonly n_exclus: number;
  /** Episodes sans etat API a juger — cas normal d une capture spec 002. */
  readonly n_non_jugeables: number;
  readonly n_accord: number;
  /** Taux d'accord sur les A, en pourcentage, arrondi au dixième. */
  readonly taux_accord: number;
  readonly par_classe: Readonly<Record<Classe, number>>;
  /** Champs les plus souvent en échec, du pire au moins pire. */
  readonly champs_en_echec: readonly { readonly champ: string; readonly n: number }[];
};

export type RapportRejeu = {
  readonly politique: string;
  readonly corpus: string;
  readonly episodes: readonly VerdictEpisode[];
  readonly illisibles: readonly EpisodeIllisible[];
  readonly agregat: Agregat;
};

/** Codes de sortie (R3.5). */
export const EXIT_OK = 0;
export const EXIT_VERDICT_NON_CONFORME = 1;
export const EXIT_ERREUR = 2;

/**
 * Charge un corpus depuis un dossier. Ordre lexicographique — c'est la première
 * condition du déterminisme (R3.3).
 *
 * Un fichier illisible ne tue pas le corpus (§9) : il est signalé et le rejeu
 * continue. Un corpus ne meurt pas d'un fichier.
 */
export async function chargerCorpus(
  dossier: string,
): Promise<{ episodes: Episode[]; illisibles: EpisodeIllisible[] }> {
  const entrees = await readdir(dossier, { withFileTypes: true });
  const fichiers = entrees
    .filter((e) => e.isFile() && e.name.endsWith('.json') && !SIDECARS.has(e.name))
    .map((e) => e.name)
    .sort();

  const episodes: Episode[] = [];
  const illisibles: EpisodeIllisible[] = [];

  for (const nom of fichiers) {
    try {
      const brut: unknown = JSON.parse(await readFile(join(dossier, nom), 'utf8'));
      episodes.push(load(brut));
    } catch (e) {
      illisibles.push({ fichier: nom, erreur: e instanceof Error ? e.message : String(e) });
    }
  }

  return { episodes, illisibles };
}

/** Agrège les verdicts. Seuls les grades A pèsent dans les statistiques (R2.2). */
export function agreger(verdicts: readonly VerdictEpisode[]): Agregat {
  const comptes = verdicts.filter((v) => v.compte_dans_stats);
  const accord = comptes.filter((v) => v.verdict === 'accord').length;

  const parClasse = Object.fromEntries(CLASSES.map((c) => [c, 0])) as Record<Classe, number>;
  const echecsParChamp = new Map<string, number>();

  for (const v of comptes) {
    for (const c of v.champs) {
      parClasse[c.classe] += 1;
      if (c.classe === 'desaccord' || c.classe === 'manque' || c.classe === 'excedent') {
        echecsParChamp.set(c.champ, (echecsParChamp.get(c.champ) ?? 0) + 1);
      }
    }
  }

  const champsEnEchec = [...echecsParChamp.entries()]
    // Tri stable : par nombre décroissant, puis par nom — sinon le rapport
    // changerait d'un rejeu à l'autre et casserait le test de déterminisme.
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([champ, n]) => ({ champ, n }));

  return {
    n_total: verdicts.length,
    n_comptes: comptes.length,
    n_exclus: verdicts.length - comptes.length,
    n_non_jugeables: verdicts.filter((v) => !v.jugeable).length,
    n_accord: accord,
    taux_accord: comptes.length === 0 ? 0 : Math.round((accord / comptes.length) * 1000) / 10,
    par_classe: parClasse,
    champs_en_echec: champsEnEchec,
  };
}

/**
 * Rejoue un corpus (R3.1). Aucun appel réseau : ni ici, ni dans la politique —
 * le type `ReplayContext` ne le permet pas.
 */
export async function rejouer(dossier: string, politique: Policy): Promise<RapportRejeu> {
  const { episodes, illisibles } = await chargerCorpus(dossier);

  const verdicts: VerdictEpisode[] = [];
  for (const ep of episodes) {
    const calls = await politique.propose(contexteDe(ep));
    verdicts.push(juger(ep, calls));
  }

  return {
    politique: politique.id,
    corpus: dossier.replace(/\\/g, '/'),
    episodes: verdicts,
    illisibles,
    agregat: agreger(verdicts),
  };
}

/**
 * Code de sortie d'un rejeu (R3.5).
 *
 * `2` seulement si RIEN n'a pu être lu — un corpus partiellement illisible reste
 * jugeable, et le rapport dit lesquels manquent.
 */
export function codeSortie(rapport: RapportRejeu): number {
  // Seule une absence TOTALE de lecture est une erreur d execution. Un corpus
  // entierement non jugeable (capture spec 002 avant connecteurs) est un etat
  // legitime : rien ne contredit rien.
  if (rapport.episodes.length === 0) return EXIT_ERREUR;
  return rapport.agregat.n_accord === rapport.agregat.n_comptes
    ? EXIT_OK
    : EXIT_VERDICT_NON_CONFORME;
}
