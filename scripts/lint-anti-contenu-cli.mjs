#!/usr/bin/env node
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { analyser } from './lint-anti-contenu.mjs';

const DOSSIER = 'supabase/migrations';

let fichiers = [];
try {
  fichiers = readdirSync(DOSSIER)
    .filter((f) => f.endsWith('.sql'))
    .sort();
} catch {
  console.log('lint anti-contenu : aucune migration, rien a verifier.');
  process.exit(0);
}

if (fichiers.length === 0) {
  console.log('lint anti-contenu : aucune migration, rien a verifier.');
  process.exit(0);
}

const toutes = [];
for (const f of fichiers) {
  toutes.push(...analyser(readFileSync(join(DOSSIER, f), 'utf8'), f));
}

if (toutes.length === 0) {
  console.log(
    `lint anti-contenu : ${fichiers.length} migration(s) verifiee(s), aucune colonne a contenu.`,
  );
  process.exit(0);
}

console.error(`\nlint anti-contenu : ${toutes.length} violation(s) de l'INVARIANT I\n`);
for (const v of toutes) {
  console.error(`  ${v.fichier}:${v.ligne}  colonne « ${v.colonne} »`);
  console.error(`    ${v.motif}`);
  console.error(`    > ${v.source}\n`);
}
console.error(
  'Aucun contenu utilisateur ne doit pouvoir etre stocke cote serveur.\n' +
    'Si cette colonne est legitime, ajoutez sur la ligne precedente :\n' +
    '    -- noe:contenu-autorise <raison>\n',
);
process.exit(1);
