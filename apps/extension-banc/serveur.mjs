/**
 * Sert la page de démonstration en HTTP.
 *
 * Pas en `file://` : un script de contenu ne s'injecte PAS dans une page locale
 * tant que « Autoriser l'accès aux URL de fichier » n'est pas cochée pour
 * l'extension. Le banc mesurerait alors l'absence de permission au lieu de la
 * capture — c'est le premier faux négatif qu'il a produit.
 */

import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env['NOE_PAGE'] ?? 4180);

createServer((_req, res) => {
  res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  res.end(readFileSync(join(ICI, 'page-demo.html'), 'utf8'));
}).listen(PORT, '127.0.0.1', () => console.log(`page sur http://127.0.0.1:${PORT}/`));
