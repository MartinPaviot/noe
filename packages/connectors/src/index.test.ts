import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { SYSTEMES_ENVISAGES } from './index.js';

/**
 * Le banc de ce paquet tenait en un `expect` sur la longueur d'un tableau que
 * rien ne lisait. Il ne pouvait rien attraper — et il n'a rien attrape quand les
 * noms ont diverge de ceux des adaptateurs.
 */
describe('@noe/connectors', () => {
  it('nomme exactement les connecteurs que le terrain declare', async () => {
    // `docs/terrain.example.json` est le miroir committe du plan de terrain,
    // verifie des deux cotes. C'est lui la reference, pas ce fichier.
    const terrain = JSON.parse(await readFile('docs/terrain.example.json', 'utf8')) as {
      crm: string;
      mail?: string;
    };
    const attendus = [terrain.crm, terrain.mail].filter((n): n is string => n !== undefined);
    expect([...SYSTEMES_ENVISAGES].sort()).toEqual([...attendus].sort());
  });

  it('n annonce aucun connecteur que le terrain ne pourrait choisir', () => {
    // Le nom n'est pas decoratif : c'est lui que `terrain.json` porte, et c'est
    // par lui que le routeur trouve l'adaptateur. Un nom faux ici ne casse rien
    // tout de suite — il casse le jour ou quelqu'un s'y fie.
    for (const nom of SYSTEMES_ENVISAGES) {
      expect(nom).toMatch(/^[a-z][a-z0-9-]*$/);
    }
  });
});
