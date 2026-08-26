import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { load } from '@noe/episode-spec';
import { describe, expect, it } from 'vitest';
import { politiqueNulle, politiqueParfaite } from './policy.js';
import {
  chargerCorpus,
  codeSortie,
  EXIT_ERREUR,
  EXIT_OK,
  EXIT_VERDICT_NON_CONFORME,
  rejouer,
} from './replay.js';
import { rapportJson } from './report.js';

const GOLDEN = 'packages/harness/golden';

async function corpusDore() {
  const { episodes } = await chargerCorpus(GOLDEN);
  return episodes;
}

describe('chargement du corpus dore (R6.1, R6.2)', () => {
  it('charge les 5 episodes et valide chacun au schema', async () => {
    const { episodes, illisibles } = await chargerCorpus(GOLDEN);
    expect(episodes).toHaveLength(5);
    expect(illisibles).toHaveLength(0);
  });

  it('couvre les cas exiges : 4 grades A et 1 grade B', async () => {
    const episodes = await corpusDore();
    expect(episodes.filter((e) => e.grade === 'A')).toHaveLength(4);
    expect(episodes.filter((e) => e.grade === 'B')).toHaveLength(1);
  });

  it('charge dans un ordre stable — premiere condition du determinisme', async () => {
    const a = (await corpusDore()).map((e) => e.id);
    const b = (await corpusDore()).map((e) => e.id);
    expect(a).toEqual(b);
    expect(a).toEqual([...a].sort());
  });

  it('ignore canaris.json, qui n est pas un episode', async () => {
    const { illisibles } = await chargerCorpus(GOLDEN);
    expect(illisibles.map((i) => i.fichier)).not.toContain('canaris.json');
  });
});

describe('auto-coherence du socle (R6.3)', () => {
  it('la politique parfaite donne 100 % d accord sur les grades A', async () => {
    const rapport = await rejouer(GOLDEN, politiqueParfaite(await corpusDore()));
    expect(rapport.agregat.n_comptes).toBe(4);
    expect(rapport.agregat.n_accord).toBe(4);
    expect(rapport.agregat.taux_accord).toBe(100);
    expect(codeSortie(rapport)).toBe(EXIT_OK);
  });

  it('la politique nulle donne 0 % et tout en manque — le juge detecte l inaction', async () => {
    const rapport = await rejouer(GOLDEN, politiqueNulle);
    expect(rapport.agregat.n_accord).toBe(0);
    expect(rapport.agregat.taux_accord).toBe(0);
    expect(rapport.agregat.par_classe.manque).toBeGreaterThan(0);
    expect(rapport.agregat.par_classe.desaccord).toBe(0);
    expect(rapport.agregat.par_classe.excedent).toBe(0);
    expect(codeSortie(rapport)).toBe(EXIT_VERDICT_NON_CONFORME);
  });
});

describe('exclusion des grades non-A (R2.2)', () => {
  it('l episode a trou est lisible mais hors des agregats', async () => {
    const rapport = await rejouer(GOLDEN, politiqueParfaite(await corpusDore()));
    expect(rapport.agregat.n_total).toBe(5);
    expect(rapport.agregat.n_comptes).toBe(4);
    expect(rapport.agregat.n_exclus).toBe(1);

    const trou = rapport.episodes.find((e) => e.grade === 'B');
    expect(trou).toBeDefined();
    expect(trou?.compte_dans_stats).toBe(false);
  });
});

describe('champ hors perimetre (R4.2)', () => {
  it('l episode (d) est en accord malgre un champ hors scope', async () => {
    const rapport = await rejouer(GOLDEN, politiqueParfaite(await corpusDore()));
    const ep = rapport.episodes.find((e) => e.totaux.hors_perimetre > 0);
    expect(ep).toBeDefined();
    expect(ep?.verdict).toBe('accord');
    expect(ep?.champs.some((c) => c.champ === 'derniere_connexion')).toBe(true);
  });
});

describe('determinisme (R3.3)', () => {
  it('trois rejeux produisent des sorties strictement identiques', async () => {
    const corpus = await corpusDore();
    const sorties: string[] = [];
    for (let i = 0; i < 3; i++) {
      sorties.push(rapportJson(await rejouer(GOLDEN, politiqueParfaite(corpus))));
    }
    expect(sorties[0]).toBe(sorties[1]);
    expect(sorties[1]).toBe(sorties[2]);
    // Aucun horodatage dans le corps : c'est ce qui rend l'egalite possible.
    expect(sorties[0]).not.toMatch(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z/);
  });
});

describe('migration d un episode legacy (R1.5)', () => {
  it('la fixture v0 migre et parse vert', async () => {
    const brut: unknown = JSON.parse(
      await readFile(join(GOLDEN, 'legacy', 'episode_v0.json'), 'utf8'),
    );
    const ep = load(brut);
    expect(ep.schema_v).toBe(1);
    expect(ep.scope_fields).toEqual(['prochaine_action', 'statut']);
    expect(ep.grade_reason.length).toBeGreaterThan(0);
  });
});

describe('fichiers illisibles — un corpus ne meurt pas d un fichier (§9)', () => {
  it('signale l illisible et juge quand meme les autres', async () => {
    const dossier = await mkdtemp(join(tmpdir(), 'noe-corpus-'));
    try {
      const bon: unknown = JSON.parse(await readFile(join(GOLDEN, '001_nominal.json'), 'utf8'));
      await writeFile(join(dossier, '001_bon.json'), JSON.stringify(bon), 'utf8');
      await writeFile(join(dossier, '002_casse.json'), '{ ceci n est pas du json', 'utf8');

      const rapport = await rejouer(dossier, politiqueParfaite([load(bon)]));
      expect(rapport.episodes).toHaveLength(1);
      expect(rapport.illisibles).toHaveLength(1);
      expect(rapport.illisibles[0]?.fichier).toBe('002_casse.json');
      expect(codeSortie(rapport)).toBe(EXIT_OK);
    } finally {
      await rm(dossier, { recursive: true, force: true });
    }
  });

  it('sort en erreur seulement si aucun episode n est lisible', async () => {
    const dossier = await mkdtemp(join(tmpdir(), 'noe-corpus-'));
    try {
      await writeFile(join(dossier, 'a.json'), 'casse', 'utf8');
      const rapport = await rejouer(dossier, politiqueNulle);
      expect(rapport.episodes).toHaveLength(0);
      expect(codeSortie(rapport)).toBe(EXIT_ERREUR);
    } finally {
      await rm(dossier, { recursive: true, force: true });
    }
  });
});
