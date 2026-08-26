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

type FichierCanaris = {
  marqueurs: { chaines: string[] };
  interdites: { chaines: string[] };
};

async function fichierCanaris(): Promise<FichierCanaris> {
  const brut = JSON.parse(
    await readFile(join(GOLDEN, 'canaris.json'), 'utf8'),
  ) as Partial<FichierCanaris>;
  const m = brut.marqueurs?.chaines;
  const i = brut.interdites?.chaines;
  if (!Array.isArray(m) || m.length === 0 || !Array.isArray(i) || i.length === 0) {
    throw new Error('canaris.json incomplet — le sweep serait sans objet');
  }
  return { marqueurs: { chaines: m }, interdites: { chaines: i } };
}

/** Tout ce que le sweep traque, marqueurs et formes PII confondus. */
async function canaris(): Promise<string[]> {
  const f = await fichierCanaris();
  return [...f.marqueurs.chaines, ...f.interdites.chaines];
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

describe('canaris — deux groupes, deux roles (R5.1)', () => {
  it('canaris.json declare des marqueurs et des formes interdites', async () => {
    const f = await fichierCanaris();
    expect(f.marqueurs.chaines).toContain('CANARY_PII_001');
    expect(f.interdites.chaines.length).toBeGreaterThanOrEqual(3);
  });

  it('les MARQUEURS sont bien dans le corpus — sinon le sweep ne prouverait rien', async () => {
    const source = await readFile(join(GOLDEN, '005_canaris.json'), 'utf8');
    for (const c of (await fichierCanaris()).marqueurs.chaines) {
      expect(source).toContain(c);
    }
  });

  it('les formes INTERDITES sont absentes du corpus — la redaction a fait son travail', async () => {
    const { episodes } = await chargerCorpus(GOLDEN);
    const serialise = JSON.stringify(episodes);
    for (const c of (await fichierCanaris()).interdites.chaines) {
      expect(serialise).not.toContain(c);
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
