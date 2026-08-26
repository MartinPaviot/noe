import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { politiqueParfaite } from './policy.js';
import { chargerCorpus, rejouer } from './replay.js';
import { rapportJson } from './report.js';
import { genererCorpus } from './synth.js';

/**
 * Performance (R3.2) : < 60 s sur 50 épisodes.
 *
 * Le seuil est une **garde**, pas une mesure fine : il doit attraper une
 * régression d'ordre de grandeur (un appel réseau qui se glisse, un algorithme
 * quadratique) sans devenir instable sur une machine chargée.
 */

const N = 50;
const SEUIL_MS = 60_000;

describe('performance du rejeu (R3.2)', () => {
  it(
    `rejoue ${N} episodes en moins de ${SEUIL_MS / 1000} s`,
    async () => {
      const dossier = await mkdtemp(join(tmpdir(), 'noe-perf-'));
      try {
        await genererCorpus(dossier, N);

        const { episodes, illisibles } = await chargerCorpus(dossier);
        // Les illisibles d'abord : si le corpus ne parse pas, c'est ce message
        // qui dit pourquoi. L'inverse masquerait la cause derriere « attendu 50 ».
        expect(illisibles.map((i) => `${i.fichier}: ${i.erreur}`)).toEqual([]);
        expect(episodes).toHaveLength(N);

        const politique = politiqueParfaite(episodes);

        const debut = performance.now();
        const rapport = await rejouer(dossier, politique);
        const ecoule = performance.now() - debut;

        expect(rapport.agregat.n_comptes).toBe(N);
        expect(rapport.agregat.taux_accord).toBe(100);
        expect(ecoule).toBeLessThan(SEUIL_MS);
      } finally {
        await rm(dossier, { recursive: true, force: true });
      }
    },
    SEUIL_MS * 2,
  );

  it('reste deterministe a cette echelle', async () => {
    const dossier = await mkdtemp(join(tmpdir(), 'noe-perf-det-'));
    try {
      await genererCorpus(dossier, N);
      const { episodes } = await chargerCorpus(dossier);
      const politique = politiqueParfaite(episodes);

      const a = rapportJson(await rejouer(dossier, politique));
      const b = rapportJson(await rejouer(dossier, politique));
      expect(a).toBe(b);
    } finally {
      await rm(dossier, { recursive: true, force: true });
    }
  });
});
