#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
/**
 * Génère les trois icônes de la barre d'état (spec 002, R5.1).
 *
 * Les trois états — observe, pause, question en attente — se distinguent par la
 * **forme** autant que par la couleur : disque plein, disque barré, anneau. Une
 * icône de tray qui ne se lit qu'à la couleur est illisible pour un daltonien et
 * indistincte en niveaux de gris, et c'est précisément le contrôle par lequel
 * l'opérateur vérifie d'un coup d'œil que rien ne l'observe à son insu.
 *
 * Les fichiers sont générés plutôt que déposés : la forme et la couleur restent
 * relisibles et modifiables ici, au lieu de vivre dans des octets opaques.
 *
 * Encodeur PNG écrit à la main — trois disques de 32 pixels ne justifient pas
 * une dépendance de traitement d'images dans le produit.
 */
import { deflateSync, inflateSync } from 'node:zlib';

const ICI = dirname(fileURLToPath(import.meta.url));
const DEST = join(ICI, '..', 'apps', 'desktop', 'src-tauri', 'icons');
const TAILLE = 32;
/** Suréchantillonnage : 4×4 sous-pixels, donc 17 niveaux de couverture. */
const SS = 4;

const CRC = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return (buf) => {
    let c = -1;
    for (const b of buf) c = t[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ -1) >>> 0;
  };
})();

function chunk(type, data) {
  const longueur = Buffer.alloc(4);
  longueur.writeUInt32BE(data.length);
  const corps = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(CRC(corps));
  return Buffer.concat([longueur, corps, crc]);
}

/** `pixels` : RGBA, une entrée par canal, longueur = t*t*4. */
function png(t, pixels) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(t, 0);
  ihdr.writeUInt32BE(t, 4);
  ihdr[8] = 8; // 8 bits par canal
  ihdr[9] = 6; // RGBA
  // Chaque ligne est précédée de son octet de filtre, ici 0 (aucun) : les
  // images sont minuscules, un filtre ne gagnerait rien de mesurable.
  const brut = Buffer.alloc(t * (t * 4 + 1));
  for (let y = 0; y < t; y++) {
    brut[y * (t * 4 + 1)] = 0;
    pixels.copy(brut, y * (t * 4 + 1) + 1, y * t * 4, (y + 1) * t * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(brut, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/**
 * `dedans(x, y)` répond en coordonnées continues centrées sur l'image.
 * Le suréchantillonnage transforme ce booléen en couverture, donc en alpha.
 */
function dessiner(t, couleur, dedans) {
  const px = Buffer.alloc(t * t * 4);
  const [r, v, b] = couleur;
  for (let y = 0; y < t; y++) {
    for (let x = 0; x < t; x++) {
      let couvert = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const px2 = x + (sx + 0.5) / SS - t / 2;
          const py2 = y + (sy + 0.5) / SS - t / 2;
          if (dedans(px2, py2)) couvert++;
        }
      }
      const i = (y * t + x) * 4;
      px[i] = r;
      px[i + 1] = v;
      px[i + 2] = b;
      px[i + 3] = Math.round((couvert / (SS * SS)) * 255);
    }
  }
  return px;
}

const R = TAILLE / 2 - 2;
const disque = (x, y) => x * x + y * y <= R * R;

const ETATS = {
  // Observe : disque plein. L'état le plus visible, parce que c'est celui qui
  // doit sauter aux yeux quand il ne devrait pas être là.
  observe: { couleur: [0x2e, 0x9e, 0x5b], forme: disque },

  // Pause : le glyphe de pause évidé dans le disque.
  pause: {
    couleur: [0xd9, 0x8a, 0x1f],
    forme: (x, y) => {
      if (!disque(x, y)) return false;
      const barre = Math.abs(y) < R * 0.55 && (Math.abs(x - 3) < 1.6 || Math.abs(x + 3) < 1.6);
      return !barre;
    },
  },

  // Question en attente : un anneau. Le creux central se lit même en niveaux de
  // gris, là où un simple changement de teinte disparaîtrait.
  question: {
    couleur: [0x3b, 0x7f, 0xd4],
    forme: (x, y) => {
      const d2 = x * x + y * y;
      return d2 <= R * R && d2 >= R * 0.45 * (R * 0.45);
    },
  },
};

/**
 * Extrait les pixels bruts d un PNG produit par ce meme encodeur.
 *
 * La verification compare les PIXELS, jamais les octets du fichier : `deflate`
 * ne garantit pas la meme sortie d une version de zlib a l autre, et comparer
 * les octets rendrait le controle rouge sur un simple changement de runtime
 * Node — un faux positif qu on finirait par desactiver, donc un controle mort.
 */
function pixelsDe(fichier) {
  const b = readFileSync(fichier);
  const morceaux = [];
  let o = 8;
  while (o < b.length) {
    const longueur = b.readUInt32BE(o);
    const type = b.subarray(o + 4, o + 8).toString('ascii');
    if (type === 'IDAT') morceaux.push(b.subarray(o + 8, o + 8 + longueur));
    o += 12 + longueur;
  }
  return inflateSync(Buffer.concat(morceaux));
}

/** Les memes lignes filtrees que celles que `png()` compresse. */
function lignesBrutes(t, pixels) {
  const brut = Buffer.alloc(t * (t * 4 + 1));
  for (let y = 0; y < t; y++) {
    brut[y * (t * 4 + 1)] = 0;
    pixels.copy(brut, y * (t * 4 + 1) + 1, y * t * 4, (y + 1) * t * 4);
  }
  return brut;
}

const verifier = process.argv.includes('--verifier');
if (!verifier) mkdirSync(DEST, { recursive: true });

let ecarts = 0;
for (const [nom, { couleur, forme }] of Object.entries(ETATS)) {
  const fichier = join(DEST, `tray-${nom}.png`);
  const pixels = dessiner(TAILLE, couleur, forme);

  if (verifier) {
    let identique = false;
    try {
      identique = pixelsDe(fichier).equals(lignesBrutes(TAILLE, pixels));
    } catch {
      identique = false;
    }
    console.log(`  ${`tray-${nom}.png`.padEnd(22)} ${identique ? 'identique' : 'DIFFERENTE'}`);
    if (!identique) ecarts++;
  } else {
    writeFileSync(fichier, png(TAILLE, pixels));
    console.log(`  ${`tray-${nom}.png`.padEnd(22)} ${TAILLE}x${TAILLE}`);
  }
}

if (verifier) {
  if (ecarts > 0) {
    console.error(`
${ecarts} icone(s) ne correspondent plus au generateur.`);
    process.exit(1);
  }
  console.log(`
${Object.keys(ETATS).length} icones conformes au generateur.`);
} else {
  console.log(`${Object.keys(ETATS).length} icones ecrites dans ${DEST}`);
}
