/**
 * Installe le pont DOM : manifeste d'hôte de native messaging + clé de registre.
 *
 * Chrome ne parle pas à un processus déjà lancé. Pour joindre l'application Noe,
 * il faut lui déclarer un **hôte** : un exécutable qu'il démarre lui-même, décrit
 * par un manifeste JSON dont le chemin est inscrit sous
 * `HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\<nom>`.
 *
 * Deux choses seulement sont écrites hors du dépôt, et les deux dans le profil de
 * l'utilisateur courant :
 *
 * 1. le manifeste, sous `%APPDATA%\\app.noe.desktop\\` ;
 * 2. une clé de registre sous `HKCU`, qui pointe dessus.
 *
 * `--desinstaller` défait exactement ces deux-là. `--verifier` ne touche à rien
 * et dit ce qui est en place — c'est ce que la CI peut lancer.
 *
 * ## L'identifiant d'extension
 *
 * `allowed_origins` doit nommer l'extension autorisée à parler à l'hôte. Sans
 * cette liste, n'importe quelle extension du navigateur pourrait ouvrir le pont
 * et écrire de faux épisodes.
 *
 * Pour une extension **non empaquetée**, Chrome dérive l'identifiant du chemin
 * absolu : SHA-256 du chemin, seize premiers octets, chaque quartet projeté sur
 * `a`-`p`. On le calcule donc au lieu de le demander, et on l'affiche pour qu'il
 * soit comparable à ce que `chrome://extensions` montre.
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const RACINE = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const NOM_HOTE = 'app.noe.pont';
const IDENTIFIANT_APPLICATION = 'app.noe.desktop';
const CLE_REGISTRE = `HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\${NOM_HOTE}`;

const EXTENSION = join(RACINE, 'apps', 'extension');
const DOSSIER_DONNEES = join(
  process.env['APPDATA'] ?? join(process.env['TEMP'] ?? '.', 'noe'),
  IDENTIFIANT_APPLICATION,
);
const MANIFESTE = join(DOSSIER_DONNEES, 'pont-dom.json');

/**
 * L'identifiant que Chrome donnera à l'extension non empaquetée de ce chemin.
 *
 * Sur Windows, Chrome hache la représentation **UTF-16LE** du chemin absolu.
 * Le mapping `0..15 → a..p` est celui de `crx_file::id_util`.
 */
export function identifiantExtension(chemin) {
  const condensat = createHash('sha256').update(Buffer.from(chemin, 'utf16le')).digest();
  return [...condensat.subarray(0, 16)]
    .flatMap((o) => [o >> 4, o & 0x0f])
    .map((q) => String.fromCharCode(97 + q))
    .join('');
}

/** Le binaire de l'hôte, release s'il existe, debug sinon. */
function cheminHote() {
  const cible = join(RACINE, 'apps', 'desktop', 'src-tauri', 'target');
  for (const profil of ['release', 'debug']) {
    const exe = join(cible, profil, 'noe-pont-dom.exe');
    if (existsSync(exe)) return exe;
  }
  return null;
}

function manifesteAttendu(hote, id) {
  return {
    name: NOM_HOTE,
    description: 'Pont entre le capteur navigateur de Noe et l application locale.',
    path: hote,
    type: 'stdio',
    // Une seule extension autorisee. Sans cette liste, n'importe quelle
    // extension du navigateur pourrait ouvrir le pont.
    allowed_origins: [`chrome-extension://${id}/`],
  };
}

function registre(action) {
  try {
    if (action === 'lire') {
      return execFileSync('reg', ['query', CLE_REGISTRE, '/ve'], {
        encoding: 'utf8',
        stdio: 'pipe',
      });
    }
    if (action === 'ecrire') {
      execFileSync('reg', ['add', CLE_REGISTRE, '/ve', '/t', 'REG_SZ', '/d', MANIFESTE, '/f'], {
        stdio: 'pipe',
      });
      return 'ecrit';
    }
    execFileSync('reg', ['delete', CLE_REGISTRE, '/f'], { stdio: 'pipe' });
    return 'supprime';
  } catch {
    return null;
  }
}

const mode = process.argv.includes('--desinstaller')
  ? 'desinstaller'
  : process.argv.includes('--verifier')
    ? 'verifier'
    : 'installer';

const id = identifiantExtension(EXTENSION);
const hote = cheminHote();

if (mode === 'desinstaller') {
  const r = registre('supprimer');
  console.log(`  cle de registre         ${r === null ? 'absente' : 'supprimee'}`);
  if (existsSync(MANIFESTE)) {
    writeFileSync(MANIFESTE, '');
    console.log(`  manifeste               vide (${MANIFESTE})`);
  }
  console.log('\nL extension reste chargee dans Chrome : retirez-la depuis chrome://extensions.');
  process.exit(0);
}

if (mode === 'verifier') {
  let ecarts = 0;
  const dire = (quoi, ok, detail) => {
    console.log(`  ${quoi.padEnd(24)}${ok ? 'ok' : 'MANQUE'}${detail ? `  ${detail}` : ''}`);
    if (!ok) ecarts += 1;
  };
  dire('binaire de l hote', hote !== null, hote ?? 'cargo build --bins');
  dire('manifeste', existsSync(MANIFESTE), MANIFESTE);
  if (existsSync(MANIFESTE) && hote) {
    const lu = JSON.parse(readFileSync(MANIFESTE, 'utf8'));
    const attendu = manifesteAttendu(hote, id);
    dire('manifeste a jour', JSON.stringify(lu) === JSON.stringify(attendu));
  }
  dire('cle de registre', registre('lire') !== null, CLE_REGISTRE);
  console.log(`\n  identifiant attendu     ${id}`);
  process.exit(ecarts === 0 ? 0 : 1);
}

if (hote === null) {
  console.error('binaire absent : lancez `cargo build --bins` dans apps/desktop/src-tauri');
  process.exit(1);
}

mkdirSync(DOSSIER_DONNEES, { recursive: true });
writeFileSync(MANIFESTE, `${JSON.stringify(manifesteAttendu(hote, id), null, 2)}\n`);
const ecrit = registre('ecrire');

console.log(`  hote                    ${hote}`);
console.log(`  manifeste               ${MANIFESTE}`);
console.log(`  cle de registre         ${ecrit === null ? 'ECHEC' : CLE_REGISTRE}`);
console.log(`  identifiant attendu     ${id}`);
console.log(`\nCharger l extension : chrome://extensions > mode developpeur > charger`);
console.log(`  ${EXTENSION}`);
console.log('\nSi l identifiant affiche par Chrome differe, relancez ce script :');
console.log('il le recalcule depuis le chemin et corrige `allowed_origins`.');
process.exit(ecrit === null ? 1 : 0);
