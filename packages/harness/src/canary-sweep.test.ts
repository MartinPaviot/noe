import { mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { politiqueNulle, politiqueParfaite } from './policy.js';
import { chargerCorpus, rejouer } from './replay.js';
import { rapportJson, rapportTexte } from './report.js';

/**
 * Canary sweep (R5).
 *
 * Le corpus doré contient volontairement des chaînes témoins. Aucune ne doit
 * apparaître en clair dans une sortie du socle : rapport, log, artefact.
 *
 * Ce test est **inconditionnel** (R5.3) : il ne lit aucune variable
 * d'environnement, n'a aucun mode « skip », et ne peut être neutralisé que par un
 * diff visible en revue.
 */

const GOLDEN = 'packages/harness/golden';

async function canaris(): Promise<string[]> {
  const brut: unknown = JSON.parse(await readFile(join(GOLDEN, 'canaris.json'), 'utf8'));
  const chaines = (brut as { chaines?: unknown }).chaines;
  if (!Array.isArray(chaines) || chaines.length === 0) {
    throw new Error('canaris.json ne contient aucune chaine — le sweep serait sans objet');
  }
  return chaines as string[];
}

/** Balaye récursivement tous les fichiers d'un dossier. */
async function tousLesFichiers(dossier: string): Promise<string[]> {
  const out: string[] = [];
  for (const e of await readdir(dossier, { withFileTypes: true })) {
    const p = join(dossier, e.name);
    if (e.isDirectory()) out.push(...(await tousLesFichiers(p)));
    else out.push(p);
  }
  return out;
}

describe('canaris — le corpus les contient bien (R5.1)', () => {
  it('canaris.json declare des chaines temoins', async () => {
    const c = await canaris();
    expect(c.length).toBeGreaterThanOrEqual(4);
    expect(c).toContain('CANARY_PII_001');
  });

  it('au moins un episode dore les porte reellement — sinon le sweep ne prouve rien', async () => {
    const source = await readFile(join(GOLDEN, '005_canaris.json'), 'utf8');
    for (const c of await canaris()) {
      expect(source).toContain(c);
    }
  });
});

describe('canary sweep — aucune fuite en sortie (R5.2)', () => {
  it('aucun canari n apparait dans les sorties d un rejeu complet', async () => {
    const liste = await canaris();
    const sortie = await mkdtemp(join(tmpdir(), 'noe-sweep-'));

    try {
      const { episodes } = await chargerCorpus(GOLDEN);

      // On produit TOUTES les sorties du socle, avec les deux politiques.
      for (const politique of [politiqueParfaite(episodes), politiqueNulle]) {
        const rapport = await rejouer(GOLDEN, politique);
        await writeFile(join(sortie, `${politique.id}.json`), rapportJson(rapport), 'utf8');
        await writeFile(join(sortie, `${politique.id}.txt`), rapportTexte(rapport), 'utf8');
      }

      const fichiers = await tousLesFichiers(sortie);
      expect(fichiers.length).toBeGreaterThan(0);

      const fuites: string[] = [];
      for (const f of fichiers) {
        const contenu = await readFile(f, 'utf8');
        for (const c of liste) {
          if (contenu.includes(c)) fuites.push(`${f} contient « ${c} »`);
        }
      }

      expect(fuites).toEqual([]);
    } finally {
      await rm(sortie, { recursive: true, force: true });
    }
  });

  it('le sweep detecte bien une fuite — un test qui ne peut pas echouer ne prouve rien', async () => {
    const liste = await canaris();
    const sortie = await mkdtemp(join(tmpdir(), 'noe-sweep-neg-'));
    try {
      // Fuite injectee volontairement : le balayage doit la voir.
      await writeFile(join(sortie, 'fuite.txt'), `notes: ${liste[0]}\n`, 'utf8');

      const fuites: string[] = [];
      for (const f of await tousLesFichiers(sortie)) {
        const contenu = await readFile(f, 'utf8');
        for (const c of liste) if (contenu.includes(c)) fuites.push(c);
      }

      expect(fuites).toContain(liste[0]);
    } finally {
      await rm(sortie, { recursive: true, force: true });
    }
  });
});
