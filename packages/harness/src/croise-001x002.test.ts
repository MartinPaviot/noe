import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Episode, episodeCapture, episodeValide } from '@noe/episode-spec';
import { describe, expect, it } from 'vitest';
import { juger } from './judge.js';
import { politiqueNulle, politiqueParfaite } from './policy.js';
import { chargerCorpus, codeSortie, EXIT_OK, rejouer } from './replay.js';
import { rapportTexte } from './report.js';

/**
 * Intégration croisée 001 × 002 (design 002, §7).
 *
 * Un épisode issu de la capture n'a pas encore d'état API : les connecteurs sont
 * le travail de la spec 003. Le harness doit le dire **explicitement** plutôt que
 * de rendre « accord sur zéro champ », qui serait un vert trompeur.
 */

describe('un episode de capture est valide au schema de la spec 001', () => {
  it('parse sans erreur', () => {
    expect(Episode.safeParse(episodeCapture()).success).toBe(true);
  });

  it('vaut grade B, avec le motif « entite non resolue »', () => {
    const ep = Episode.parse(episodeCapture());
    expect(ep.grade).toBe('B');
    expect(ep.grade_reason).toContain('entite non resolue');
  });

  it('passe la validation de redaction — les PII sont deja tokenisees', () => {
    // Le payload porte EMAIL_7f3a9c21, pas l adresse. Si la capture avait laisse
    // l adresse, le grade tomberait en C et le parse echouerait.
    expect(Episode.safeParse(episodeCapture()).success).toBe(true);
  });
});

describe('le juge dit « rien a juger » au lieu de mentir', () => {
  it('rend un verdict non_jugeable', () => {
    const v = juger(Episode.parse(episodeCapture()), []);
    expect(v.jugeable).toBe(false);
    expect(v.verdict).toBe('non_jugeable');
  });

  it('ne compte pas dans les statistiques', () => {
    expect(juger(Episode.parse(episodeCapture()), []).compte_dans_stats).toBe(false);
  });

  it('ne fabrique aucun champ', () => {
    expect(juger(Episode.parse(episodeCapture()), []).champs).toEqual([]);
  });

  it('un episode resolu, lui, reste jugeable — la distinction porte bien', () => {
    const v = juger(Episode.parse(episodeValide()), []);
    expect(v.jugeable).toBe(true);
    expect(v.verdict).toBe('desaccord');
  });
});

describe('rejeu d un corpus de capture pure', () => {
  it('sort en 0 et l annonce clairement, sans traiter le cas comme une erreur', async () => {
    const dossier = await mkdtemp(join(tmpdir(), 'noe-002-'));
    try {
      await writeFile(
        join(dossier, '001_capture.json'),
        JSON.stringify(episodeCapture(), null, 2),
        'utf8',
      );

      const { episodes, illisibles } = await chargerCorpus(dossier);
      expect(illisibles).toEqual([]);
      expect(episodes).toHaveLength(1);

      const rapport = await rejouer(dossier, politiqueParfaite(episodes));

      expect(rapport.agregat.n_total).toBe(1);
      expect(rapport.agregat.n_comptes).toBe(0);
      expect(rapport.agregat.n_non_jugeables).toBe(1);

      // Le point du §7 : ce n'est pas une erreur d'execution.
      expect(codeSortie(rapport)).toBe(EXIT_OK);

      const texte = rapportTexte(rapport);
      expect(texte).toContain('aucun etat API a juger');
    } finally {
      await rm(dossier, { recursive: true, force: true });
    }
  });

  it('la politique nulle ne change rien : il n y a toujours rien a juger', async () => {
    const dossier = await mkdtemp(join(tmpdir(), 'noe-002b-'));
    try {
      await writeFile(
        join(dossier, '001_capture.json'),
        JSON.stringify(episodeCapture(), null, 2),
        'utf8',
      );
      const rapport = await rejouer(dossier, politiqueNulle);
      expect(rapport.agregat.n_non_jugeables).toBe(1);
      expect(codeSortie(rapport)).toBe(EXIT_OK);
    } finally {
      await rm(dossier, { recursive: true, force: true });
    }
  });
});
