/**
 * Tests visuels du squelette traversant (D21, D26).
 *
 * Le déterminisme des pixels n'est pas négociable : une baseline qui bouge sans
 * raison finit par être régénérée à l'aveugle, et le contrôle ne prouve plus
 * rien. D'où le viewport fixe, les animations coupées, et le `webServer` qui
 * sert le build plutôt que le serveur de développement.
 *
 * **Sur Windows, et pas sur Linux.** La mission prévoyait un runner Linux pour
 * l'UI web. Mais cette vue est celle d'une application Windows, et sa pile de
 * polices l'est aussi : `Segoe UI` n'existe pas sur un runner Linux, les
 * baselines y seraient donc différentes de ce que l'opérateur voit. Les
 * comparer sur la plateforme cible est plus fidèle que de les comparer là où
 * c'est commode.
 */
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/visuel',
  snapshotPathTemplate: '{testDir}/__screenshots__/{arg}{ext}',
  // **Un seul worker, pas de parallelisme.**
  //
  // En parallele, cinq contextes de navigateur contre un meme serveur de
  // previsualisation donnaient des resultats differents d une execution a
  // l autre — 4, puis 1, puis 5 tests rapportes. Un controle visuel dont le
  // resultat varie ne prouve rien, et D21 en fait un rouge. En serie, les cinq
  // passent en 4,7 s : le parallelisme ne rachetait meme pas son instabilite.
  fullyParallel: false,
  workers: 1,
  // Une baseline qui ne passe qu'au deuxième essai cache un vrai
  // non-déterminisme : on ne réessaie pas.
  retries: 0,
  reporter: process.env['CI'] ? 'line' : 'list',

  use: {
    baseURL: 'http://localhost:4173',
    viewport: { width: 1280, height: 800 },
    // Rendu identique d'une machine à l'autre, dans la limite des polices.
    deviceScaleFactor: 1,
  },

  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.01,
      // Coupe toute animation ou transition CSS pendant la capture.
      animations: 'disabled',
      caret: 'hide',
    },
  },

  webServer: {
    command: 'pnpm exec vite preview --port 4173 --strictPort',
    // `localhost` et non `127.0.0.1` : sur Windows, le premier résout d'abord
    // en IPv6, et le serveur de prévisualisation n'écoute que là. L'adresse
    // numérique donnait une attente de soixante secondes puis un échec.
    url: 'http://localhost:4173',
    reuseExistingServer: !process.env['CI'],
    timeout: 60_000,
  },
});
