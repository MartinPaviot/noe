import { defineConfig } from 'vite';

export default defineConfig({
  // Port fixe : tauri.conf.json y renvoie via devUrl.
  server: { port: 1420, strictPort: true },
  build: { outDir: 'dist', emptyOutDir: true },
  clearScreen: false,
});
