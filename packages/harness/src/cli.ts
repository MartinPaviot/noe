#!/usr/bin/env node
import { AIDE } from './index.js';

/** Point d'entree du CLI. En session 0 il affiche l'aide et sort proprement. */
function main(argv: readonly string[]): number {
  const commande = argv[0];
  if (commande !== undefined && commande !== '--help' && commande !== '-h') {
    process.stderr.write(`noe: commande inconnue « ${commande} »\n\n`);
    process.stdout.write(AIDE);
    return 2;
  }
  process.stdout.write(AIDE);
  return 0;
}

process.exitCode = main(process.argv.slice(2));
