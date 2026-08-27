import { readdir, readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * INVARIANT VI, rendu mécanique.
 *
 * « `@noe/core` est pur. Aucun I/O, aucun réseau, aucune horloge, aucun hasard
 * non injecté. C'est ce qui rend le rejeu déterministe possible. »
 *
 * C'était écrit dans `docs/invariants.md` et vérifié par personne. Au moment
 * d'écrire ce banc, **trois violations vivaient dans `client.ts`** — un
 * `setTimeout` en valeur par défaut et deux `Math.random` — et une quatrième
 * dans `close.ts`, un `ulid()` qui tire à la fois du hasard et une horloge.
 *
 * Aucune n'était malveillante ; chacune était une commodité. C'est précisément
 * pour ça qu'un invariant a besoin d'un banc : personne n'écrit une violation en
 * se disant qu'il en écrit une.
 *
 * ## Ce que le banc ne prétend pas faire
 *
 * Il lit du texte, pas un graphe d'appels. Un module qui appellerait une horloge
 * par un chemin détourné — une chaîne construite à l'exécution, un import
 * dynamique — lui échapperait. Il attrape ce qui arrive vraiment : la commodité
 * écrite en clair, dans un moment de fatigue.
 */

/** Les paquets que l'INVARIANT VI déclare purs. */
const PURS = ['packages/core/src', 'packages/episode-spec/src'];

/**
 * Ce qu'un paquet pur n'a pas le droit d'écrire.
 *
 * Les motifs sont assemblés au lieu d'être écrits en clair, pour que **ce
 * fichier ne se dénonce pas lui-même** — il est dans le dossier `scripts`, donc
 * hors du balayage, mais un jour quelqu'un le déplacera.
 */
const INTERDITS: readonly { readonly quoi: string; readonly motif: RegExp }[] = [
  { quoi: 'hasard non injecte', motif: new RegExp(`Math\\.${'random'}`) },
  { quoi: 'horloge', motif: new RegExp(`Date\\.${'now'}|new ${'Date'}\\(`) },
  { quoi: 'minuteur', motif: new RegExp(`\\b${'set'}(Timeout|Interval)\\b`) },
  { quoi: 'reseau', motif: new RegExp(`\\b${'fetch'}\\(|XMLHttpRequest`) },
  { quoi: 'systeme de fichiers ou process', motif: /\bnode:|(?<![A-Za-z])process\./ },
  { quoi: 'chargement dynamique', motif: /\brequire\(/ },
];

/**
 * Les seuls imports qu'un paquet pur a le droit de faire.
 *
 * Une liste blanche et pas une liste noire : le jour où quelqu'un ajoute une
 * bibliothèque qui tire un identifiant ou lit l'heure, c'est ici que ça
 * s'arrête. `ulid` en était une, et elle est partie.
 */
const IMPORTS_ADMIS = ['zod'];

/**
 * Retire les commentaires avant le balayage.
 *
 * Sans ça, le banc mord sur la prose qui EXPLIQUE pourquoi une horloge a été
 * retirée — ce qui s'est produit à son premier lancement, sur les commentaires
 * écrits une heure plus tôt. Un contrôle qui punit la documentation de ce qu'il
 * protège apprend à ne plus documenter.
 *
 * Le découpage est grossier : une chaîne contenant `//` sera tronquée. La
 * conséquence est bénigne — le motif interdit se trouve avant, dans
 * `fetch('https://…')` comme ailleurs — et la liste blanche d'imports ferme
 * l'autre chemin, celui d'une bibliothèque.
 */
function sansCommentaires(source: string): string[] {
  // Les blocs sont remplacés par des espaces plutôt que supprimés : les numéros
  // de ligne du rapport doivent rester ceux du fichier, sinon on envoie chercher
  // la faute au mauvais endroit.
  const sansBlocs = source.replace(/\/\*[\s\S]*?\*\//g, (bloc) => bloc.replace(/[^\n]/g, ' '));
  return sansBlocs.split('\n').map((l) => l.replace(/\/\/.*$/, ''));
}

async function fichiersDe(dossier: string): Promise<string[]> {
  const sortie: string[] = [];
  for (const e of await readdir(dossier, { withFileTypes: true })) {
    const p = join(dossier, e.name);
    if (e.isDirectory()) {
      sortie.push(...(await fichiersDe(p)));
    } else if (extname(e.name) === '.ts' && !e.name.endsWith('.test.ts')) {
      // Les bancs ont le droit d'être impurs : c'est même leur travail, ils
      // fabriquent le temps et le hasard que le module refuse de fabriquer.
      sortie.push(p);
    }
  }
  return sortie;
}

describe('INVARIANT VI — les paquets purs le restent', () => {
  it('aucune horloge, aucun hasard non injecte, aucun I/O', async () => {
    const fautes: string[] = [];
    for (const paquet of PURS) {
      for (const fichier of await fichiersDe(paquet)) {
        const contenu = await readFile(fichier, 'utf8');
        for (const [i, ligne] of sansCommentaires(contenu).entries()) {
          for (const { quoi, motif } of INTERDITS) {
            if (motif.test(ligne)) {
              fautes.push(`${fichier}:${i + 1} — ${quoi} : ${ligne.trim()}`);
            }
          }
        }
      }
    }
    expect(fautes, `l INVARIANT VI est viole :\n${fautes.join('\n')}`).toEqual([]);
  });

  it('n importe que ce que la liste blanche autorise', async () => {
    const fautes: string[] = [];
    for (const paquet of PURS) {
      for (const fichier of await fichiersDe(paquet)) {
        const contenu = await readFile(fichier, 'utf8');
        for (const m of sansCommentaires(contenu)
          .join('\n')
          .matchAll(/from\s+'([^']+)'/g)) {
          const specificateur = m[1] ?? '';
          const interne = specificateur.startsWith('.');
          if (!interne && !IMPORTS_ADMIS.includes(specificateur)) {
            fautes.push(`${fichier} importe « ${specificateur} »`);
          }
        }
      }
    }
    expect(fautes, `import hors liste blanche :\n${fautes.join('\n')}`).toEqual([]);
  });

  it('le balayage voit vraiment des fichiers — sinon il ne prouverait rien', async () => {
    // Un chemin qui change, un dossier renomme, et le banc passerait au vert en
    // ne lisant rien du tout. Un controle qui ne trouve pas sa matiere doit le
    // dire, pas se taire.
    for (const paquet of PURS) {
      const fichiers = await fichiersDe(paquet);
      expect(fichiers.length, `${paquet} : aucun fichier balaye`).toBeGreaterThan(3);
    }
  });

  it('sait dire non — les motifs attrapent bien ce qu ils visent', async () => {
    // Sans ce cas, une expression reguliere cassee rendrait le banc
    // definitivement vert. On lui donne donc a manger ce qu'il doit refuser.
    const temoins = [
      `const x = Math.${'random'}();`,
      `const t = Date.${'now'}();`,
      `${'set'}Timeout(f, 10);`,
      `await ${'fetch'}('https://exemple.invalid');`,
      `import { z } from 'node:fs';`,
      `${'require'}('ulid');`,
    ];
    for (const temoin of temoins) {
      const attrape = INTERDITS.some(({ motif }) => motif.test(temoin));
      expect(attrape, `aucun motif n attrape « ${temoin} »`).toBe(true);
    }
    // Et il ne doit pas mordre sur du code legitime.
    for (const innocent of [
      'const alea = options.alea;',
      'export type Horodatage = string;',
      'const processus = decrire(etat);',
    ]) {
      const attrape = INTERDITS.some(({ motif }) => motif.test(innocent));
      expect(attrape, `un motif mord sur « ${innocent} »`).toBe(false);
    }
  });
});
