import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: [
      'packages/*/src/**/*.test.ts',
      'apps/*/src/**/*.test.ts',
      // L'extension n'a pas de dossier `src` : c'est un paquet MV3, dont les
      // fichiers doivent rester a la racine pour que le manifeste les designe
      // sans chemin. Le banc des motifs vit donc a cote d'eux.
      'apps/extension/*.test.ts',
      // Meme raison pour les outils de terrain : ce sont des scripts,
      // invoques par chemin, et les ranger sous `src` casserait les
      // commandes ecrites dans les specs et dans le journal.
      'apps/terrain/*.test.ts',
      'scripts/**/*.test.ts',
    ],
    environment: 'node',
    passWithNoTests: false,
  },
});
