/**
 * La bibliothèque de motifs PII, troisième implémentation.
 *
 * **Elle ne redéfinit rien.** Elle lit le miroir généré depuis `MOTIFS_PII` de
 * `episode-spec`, exactement comme le fait l'adaptateur Rust. C'est pour ça que
 * la bibliothèque est déclarée en **chaînes** et pas en littéraux d'expression
 * régulière : trois moteurs doivent pouvoir la consommer telle quelle.
 *
 * ## À quoi elle sert ici, et à quoi elle ne sert PAS
 *
 * Elle ne rédacte pas. La pseudonymisation exige la clé HMAC du poste, protégée
 * par DPAPI, et faire entrer cette clé dans une page web serait absurde : une
 * page compromise la lirait, et avec elle tout le graphe d'entités.
 *
 * Elle sert à **ne pas envoyer** ce qui n'a rien à faire dans un ancrage. Un nom
 * accessible qui contient un numéro de téléphone n'est pas un meilleur ancrage
 * parce qu'il contient un numéro : c'est le même contrôle, avec une donnée en
 * plus qui traversera le pont pour rien. On la remplace donc par le TYPE détecté
 * — `[TEL_FR]` — ce qui garde l'ancrage stable et lisible sans transporter la
 * valeur.
 *
 * Et elle sert au canari : la spec 002 compare les trois implémentations sur le
 * même jeu d'entrées. Une divergence rendrait les canaris menteurs — la
 * bibliothèque dirait « propre » là où une autre voit une fuite.
 */

/** Le miroir, chargé une fois. Injecté par le banc, lu depuis l'extension sinon. */
let MIROIR = null;

export function chargerMiroir(miroir) {
  MIROIR = miroir;
}

async function miroir() {
  if (MIROIR) return MIROIR;
  const url = chrome.runtime.getURL('motifs.json');
  MIROIR = await (await fetch(url)).json();
  return MIROIR;
}

/**
 * Ramène les blancs exotiques à l'espace ASCII, avant toute recherche.
 *
 * Miroir exact de `normaliserBlancs` et de `normaliser_blancs`. Les motifs sont
 * compilés en ASCII des trois côtés — c'est ce qui garantit que les trois
 * moteurs lisent la même chaîne de la même façon. Le prix, c'est qu'un `U+00A0`
 * entre deux groupes de chiffres n'est pas un séparateur reconnu, et l'insécable
 * est ce que produisent Word, les signatures de courriel et beaucoup de CRM.
 */
export function normaliserBlancs(texte) {
  let sortie = '';
  for (const c of texte ?? '') {
    const p = c.codePointAt(0) ?? 0;
    if (p < 0x80) {
      sortie += c;
      continue;
    }
    const blanc = /\s/u.test(c) || p === 0x200b || p === 0x200c || p === 0x200d || p === 0xfeff;
    sortie += blanc ? ' ' : c;
  }
  return sortie;
}

/** Toutes les occurrences, triées par position puis par type — comme les deux autres. */
export function chercher(texte, m = MIROIR) {
  if (!m) throw new Error('miroir de motifs non charge');
  const cible = normaliserBlancs(texte);
  const trouvees = [];
  for (const motif of m.motifs) {
    const re = new RegExp(motif.source, motif.drapeaux);
    let x = re.exec(cible);
    while (x !== null) {
      trouvees.push({ type: motif.type, index: x.index, fin: x.index + x[0].length });
      x = re.exec(cible);
    }
  }
  return trouvees.sort((a, b) => a.index - b.index || a.type.localeCompare(b.type));
}

/**
 * Arbitre les chevauchements — miroir exact de `resoudreChevauchements`.
 *
 * Glouton : priorité croissante, puis longueur décroissante, puis position. Un
 * IBAN contient une suite de chiffres qu'un motif téléphonique reconnaît ; sans
 * arbitrage, le même texte produirait deux résultats selon l'ordre d'évaluation.
 */
export function resoudreChevauchements(occurrences, m = MIROIR) {
  const priorite = new Map(m.motifs.map((x) => [x.type, x.priorite]));
  const candidats = [...occurrences].sort((a, b) => {
    const pa = priorite.get(a.type) ?? Number.MAX_SAFE_INTEGER;
    const pb = priorite.get(b.type) ?? Number.MAX_SAFE_INTEGER;
    if (pa !== pb) return pa - pb;
    const la = a.fin - a.index;
    const lb = b.fin - b.index;
    if (la !== lb) return lb - la;
    return a.index - b.index;
  });

  const retenues = [];
  for (const c of candidats) {
    if (!retenues.some((r) => c.index < r.fin && r.index < c.fin)) retenues.push(c);
  }
  return retenues.sort((a, b) => a.index - b.index);
}

/**
 * Remplace toute PII d'un texte par son TYPE, entre crochets.
 *
 * Pas un jeton : produire un jeton demanderait la clé du poste. Le type suffit à
 * ce qu'on veut ici — que l'ancrage reste le même contrôle sans transporter la
 * donnée. Le rédacteur de l'application fera le reste sur ce qui, lui, doit
 * vraiment rejoindre le graphe d'entités.
 *
 * De la fin vers le début : remplacer par l'avant décalerait les positions
 * suivantes, et les occurrences restantes viseraient à côté.
 */
export function elider(texte, m = MIROIR) {
  const cible = normaliserBlancs(texte);
  const retenues = resoudreChevauchements(chercher(cible, m), m);
  if (retenues.length === 0) return cible;
  let sortie = cible;
  for (let i = retenues.length - 1; i >= 0; i -= 1) {
    const o = retenues[i];
    sortie = sortie.slice(0, o.index) + `[${o.type}]` + sortie.slice(o.fin);
  }
  return sortie;
}

export { miroir };
