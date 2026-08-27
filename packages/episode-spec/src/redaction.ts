/**
 * Bibliothèque de motifs PII et validateur de redaction.
 *
 * R4.6 (spec 002) donne enfin une définition mécanique à « redaction validée »,
 * que la spec 001 avait laissée en placeholder structurel : on sérialise
 * l'épisode entier et on exige **zéro correspondance**. Sinon, déclassement avec
 * motif.
 *
 * La bibliothèque est **versionnée** : un corpus jugé sous `v1` reste
 * interprétable même si `v2` durcit les motifs.
 */

/**
 * Version 4.
 *
 * - **v2** : `TEL_FR` accepte un séparateur après `+33`. La v1 laissait passer
 *   « +33 6 12 34 56 78 » en clair.
 * - **v3** : plus aucune anticipation dans la bibliothèque, et arbitrage des
 *   chevauchements par `priorite`. Le moteur de Rust ne connaît pas
 *   l'anticipation ; une bibliothèque censée être lue par trois moteurs doit
 *   tenir dans leur sous-ensemble commun.
 * - **v4** : trois fuites de la même famille, trouvées par revue adverse.
 *   `+33 (0)6 …` — la graphie d'affichage standard française — ne matchait
 *   aucun motif ; `0033 …` non plus ; et un espace insécable entre les groupes
 *   suffisait à faire passer n'importe quel numéro, parce que les motifs sont
 *   compilés en ASCII des deux côtés. Les deux premiers se corrigent dans le
 *   motif, le troisième dans une normalisation appliquée AVANT la recherche.
 *   Et pour que la classe cesse d'être invisible, le juge R4.6 ne s'appuie plus
 *   uniquement sur cette bibliothèque : voir `chercherCompact`.
 */
export const VERSION_MOTIFS = 4;

/**
 * Ramène les blancs exotiques à l'espace ASCII, avant toute recherche.
 *
 * Les motifs sont compilés en ASCII des deux côtés — `unicode(false)` en Rust,
 * `\d` ASCII en JavaScript — pour que les deux moteurs lisent la même chaîne de
 * la même façon. Le prix de cette garantie, c'est qu'un `U+00A0` entre deux
 * groupes de chiffres n'est pas un séparateur reconnu : « 06<NBSP>12<NBSP>34…»
 * traversait la redaction en clair. Or l'insécable est ce que produisent Word,
 * les signatures de courriel et beaucoup de champs de CRM.
 *
 * On normalise donc au lieu d'élargir les motifs : un seul endroit à corriger,
 * et les index restent comparables entre TypeScript (UTF-16) et Rust (octets)
 * puisque la sortie est ASCII dès que l'entrée ne portait que des blancs
 * exotiques.
 *
 * Les caractères de largeur nulle deviennent une espace et non rien : `06\u200b12`
 * doit se lire comme deux groupes séparés, pas comme `0612`, sinon on invente un
 * numéro que personne n'a écrit.
 */
export function normaliserBlancs(texte: string): string {
  let sortie = '';
  for (const c of texte) {
    const p = c.codePointAt(0) ?? 0;
    if (p < 0x80) {
      sortie += c;
      continue;
    }
    const blanc =
      /\s/u.test(c) ||
      p === 0x200b || // espace de largeur nulle
      p === 0x200c ||
      p === 0x200d ||
      p === 0xfeff; // marque d'ordre des octets
    sortie += blanc ? ' ' : c;
  }
  return sortie;
}

/**
 * La même chaîne, réduite à ses caractères signifiants.
 *
 * Tout ce qui n'est ni alphanumérique ni `+` disparaît. Sert au filet du juge :
 * une graphie que la bibliothèque ne connaît pas encore — un séparateur exotique,
 * une parenthèse, un tiret cadratin — se réduit ici à la même suite de chiffres
 * que la graphie canonique.
 */
export function compacter(texte: string): string {
  let sortie = '';
  for (const c of texte) {
    if (/[0-9A-Za-z+]/.test(c)) sortie += c;
  }
  return sortie;
}

export type MotifPii = {
  readonly type: string;
  readonly source: string;
  readonly drapeaux: string;
  /**
   * Qui l'emporte quand deux motifs mordent sur le même texte. Le plus petit
   * gagne.
   *
   * Ce champ remplace les anticipations négatives. Un IBAN contient une suite de
   * chiffres qu'un motif téléphonique reconnaît ; un numéro français est un
   * numéro international. Sans arbitrage, le même texte produirait deux jetons
   * différents selon l'ordre d'évaluation — et deux jetons pour une même entité,
   * c'est une jointure perdue, donc un graphe faux.
   */
  readonly priorite: number;
  readonly note: string;
};

/**
 * Les motifs, sous forme de chaînes plutôt que de littéraux `RegExp` : c'est ce
 * qui permettra à l'adaptateur Rust de la spec 002 de consommer exactement la
 * même bibliothèque, au lieu d'en maintenir une copie qui divergerait.
 */
export const MOTIFS_PII: readonly MotifPii[] = [
  {
    type: 'EMAIL',
    source: '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}',
    drapeaux: 'g',
    priorite: 30,
    note: 'adresse de courriel',
  },
  {
    type: 'TEL_FR',
    // Le séparateur après `+33` est OPTIONNEL mais doit être permis.
    //
    // La v1 écrivait `(?:\+33|0)[1-9]`, qui exige le chiffre collé à l'indicatif.
    // Or « +33 6 12 34 56 78 » — la façon la plus courante d'écrire un mobile
    // français à l'international — tombait alors dans un trou : TEL_FR le
    // refusait faute de séparateur permis, et TEL_INTL l'excluait explicitement
    // par son `(?!33)`. Le numéro traversait la redaction en clair.
    //
    // Trouvé par les vecteurs partagés avant qu'aucune capture réelle ne tourne.
    //
    // v4 : `(0)` et `0033`. « +33 (0)6 12 34 56 78 » est la graphie d'affichage
    // standard en France — en-tetes de courriel, signatures, cartes de visite,
    // et donc titres de fenetre et noms accessibles. Elle ne matchait RIEN :
    // apres `+33`, la parenthese cassait la branche, et le `0` de `(0)` etait
    // suivi de `)` la ou le motif attend `[1-9]`. Trouvee par revue adverse,
    // apres D24 — la meme classe, une graphie de plus.
    source: '(?:\\+?0033[ .-]?|\\+33[ .-]?(?:\\(0\\)[ .-]?)?|0)[1-9](?:[ .-]?\\d{2}){4}',
    drapeaux: 'g',
    // Avant TEL_INTL : un numéro français EST un numéro international, et il
    // doit toujours rendre le même jeton.
    priorite: 40,
    note: 'numero francais, indicatif +33 ou 0, avec ou sans separateurs',
  },
  {
    type: 'TEL_INTL',
    // PAS d'anticipation négative.
    //
    // La v2 écrivait `\+(?!33)` pour exclure la France. Le moteur d'expressions
    // régulières de Rust ne connaît NI anticipation NI rétrospection : la
    // bibliothèque était donc inconsommable par l'adaptateur natif, alors que sa
    // raison d'être est précisément d'être lue telle quelle par les trois
    // moteurs. Elle doit tenir dans leur sous-ensemble commun.
    //
    // L'exclusion française passe désormais par la priorité : TEL_FR gagne
    // l'arbitrage de chevauchement. Même résultat, syntaxe portable.
    source: '\\+\\d{1,3}[ .-]?\\d{2,4}(?:[ .-]?\\d{2,4}){2,}',
    drapeaux: 'g',
    priorite: 50,
    note: 'numero international ; la France est captee par TEL_FR, plus prioritaire',
  },
  {
    type: 'IBAN',
    source: '\\b[A-Z]{2}\\d{2}[A-Z0-9]{10,30}\\b',
    drapeaux: 'g',
    // Le plus prioritaire : un IBAN contient des suites de chiffres que les
    // motifs téléphonique et de carte reconnaissent au passage.
    priorite: 10,
    note: 'IBAN : deux lettres pays, deux chiffres de controle, puis le compte',
  },
  {
    type: 'CARTE',
    source: '\\b(?:\\d{4}[ -]?){3}\\d{4}\\b',
    drapeaux: 'g',
    priorite: 20,
    note: 'numero de carte a 16 chiffres',
  },
];

export type OccurrencePii = {
  readonly type: string;
  readonly extrait: string;
  readonly index: number;
  /** Index de fin, exclusif. Nécessaire pour arbitrer les chevauchements. */
  readonly fin: number;
};

/**
 * Arbitre les chevauchements : quelles occurrences seront réellement remplacées.
 *
 * `chercherPii` rend TOUT ce qui matche, y compris des motifs qui mordent l'un
 * sur l'autre — un IBAN contient une suite de chiffres qu'un motif téléphonique
 * reconnaît. Pour **valider** (R4.6) cela suffit : n'importe quelle occurrence
 * rend l'épisode invalide. Pour **redacter**, non : remplacer deux occurrences
 * qui se chevauchent produirait un jeton tronqué au milieu d'un autre.
 *
 * La règle est gloutonne et entièrement déterministe : priorité croissante,
 * puis longueur décroissante, puis position. Elle est partagée par les trois
 * implémentations, et les vecteurs de test en figent le résultat — c'est ce qui
 * garantit qu'un même texte donne le même jeton partout, donc que les jointures
 * du graphe d'entités tiennent.
 */
export function resoudreChevauchements(
  occurrences: readonly OccurrencePii[],
): readonly OccurrencePii[] {
  const priorite = new Map(MOTIFS_PII.map((m) => [m.type, m.priorite]));
  const candidats = [...occurrences].sort((a, b) => {
    const pa = priorite.get(a.type) ?? Number.MAX_SAFE_INTEGER;
    const pb = priorite.get(b.type) ?? Number.MAX_SAFE_INTEGER;
    if (pa !== pb) return pa - pb;
    const la = a.fin - a.index;
    const lb = b.fin - b.index;
    if (la !== lb) return lb - la;
    return a.index - b.index;
  });

  const retenues: OccurrencePii[] = [];
  for (const c of candidats) {
    const chevauche = retenues.some((r) => c.index < r.fin && r.index < c.fin);
    if (!chevauche) retenues.push(c);
  }
  return retenues.sort((a, b) => a.index - b.index);
}

/**
 * Cherche des PII dans un texte.
 *
 * Retourne des occurrences **tronquées** : on signale qu'il y a une fuite et de
 * quel type, sans la recopier entièrement dans un message d'erreur qui finirait
 * lui-même dans un log.
 *
 * **Les index portent sur `normaliserBlancs(texte)`, pas sur `texte`.** Qui
 * remplace doit donc normaliser d'abord — c'est ce que fait le redacteur natif.
 * Sans cette normalisation, un insécable suffisait à faire passer un numéro.
 */
export function chercherPii(brutTexte: string): OccurrencePii[] {
  const texte = normaliserBlancs(brutTexte);
  const trouvees: OccurrencePii[] = [];
  for (const motif of MOTIFS_PII) {
    const re = new RegExp(motif.source, motif.drapeaux);
    let m: RegExpExecArray | null = re.exec(texte);
    while (m !== null) {
      const brut = m[0];
      trouvees.push({
        type: motif.type,
        // Assez pour identifier la fuite en revue, trop peu pour la reutiliser.
        extrait: `${brut.slice(0, 3)}…${brut.slice(-2)}`,
        index: m.index,
        fin: m.index + brut.length,
      });
      m = re.exec(texte);
    }
  }
  return trouvees.sort((a, b) => a.index - b.index || a.type.localeCompare(b.type));
}

/**
 * Le filet du juge : les motifs appliqués au texte **compacté**.
 *
 * R4.6 valide la redaction en cherchant des PII avec `MOTIFS_PII` — la
 * bibliothèque même qui a servi à redacter. Un juge adossé à ce qu'il contrôle
 * est aveugle par construction : tout trou de motif passe deux fois, à
 * l'écriture puis à la validation, et l'épisode ressort gradé « redaction
 * validée ». C'est exactement ce qui s'est produit trois fois — D24, puis les
 * graphies `(0)` et insécable de la v4.
 *
 * Ce filet ne partage pas les motifs. Il regarde la suite de chiffres, séparateurs
 * ôtés : n'importe quelle graphie d'un numéro français s'y réduit à la même
 * chaîne, y compris celles que personne n'a encore imaginées.
 *
 * Il ne sert QUE à valider, jamais à redacter. Un filet qui remplacerait
 * pseudonymiserait des montants et des références, et abîmerait des données que
 * la spec 003 doit pouvoir comparer.
 */
export const MOTIFS_COMPACT: readonly MotifPii[] = [
  {
    type: 'TEL_FR_COMPACT',
    // Neuf chiffres derriere un indicatif ou un zero de tete : la forme d'un
    // numero francais, quelle que soit la ponctuation qui l'habillait.
    source: '(?:\\+?0033|\\+?33|0)[1-9]\\d{8}',
    drapeaux: 'g',
    priorite: 100,
    note: 'filet du juge : numero francais une fois les separateurs otes',
  },
];

/**
 * Toutes les chaînes d'une valeur JSON, clés comprises.
 *
 * Le filet s'applique champ par champ et jamais sur l'objet sérialisé entier :
 * en compactant un JSON complet, les chiffres de deux champs voisins se
 * colleraient et fabriqueraient des numéros que personne n'a écrits. Un faux
 * positif ici déclasse un épisode honnête sans qu'on puisse rien y faire.
 */
function chaines(valeur: unknown, vues: string[] = []): string[] {
  if (typeof valeur === 'string') vues.push(valeur);
  else if (Array.isArray(valeur)) for (const v of valeur) chaines(v, vues);
  else if (valeur !== null && typeof valeur === 'object')
    for (const [k, v] of Object.entries(valeur)) {
      vues.push(k);
      chaines(v, vues);
    }
  return vues;
}

/** Les occurrences vues par le filet, sur une chaîne isolée. */
export function chercherCompact(brutTexte: string): OccurrencePii[] {
  const texte = compacter(normaliserBlancs(brutTexte));
  const trouvees: OccurrencePii[] = [];
  for (const motif of MOTIFS_COMPACT) {
    const re = new RegExp(motif.source, motif.drapeaux);
    let m: RegExpExecArray | null = re.exec(texte);
    while (m !== null) {
      const brut = m[0];
      trouvees.push({
        type: motif.type,
        extrait: `${brut.slice(0, 3)}…${brut.slice(-2)}`,
        index: m.index,
        fin: m.index + brut.length,
      });
      m = re.exec(texte);
    }
  }
  return trouvees;
}

export type VerdictRedaction = {
  readonly valide: boolean;
  readonly occurrences: readonly OccurrencePii[];
};

/**
 * Valide la redaction d'un épisode entier (R4.6).
 *
 * On sérialise et on balaye : un motif qui traverse la frontière d'un champ
 * compte quand même, et c'est voulu — la question n'est pas « ce champ est-il
 * propre » mais « quelque chose peut-il fuir de cet objet ».
 */
export function validerRedaction(episode: unknown): VerdictRedaction {
  const occurrences = [...chercherPii(JSON.stringify(episode))];
  // Puis le filet, champ par champ. Il peut voir ce que la bibliothèque a raté ;
  // c'est sa seule raison d'exister, et le jour où il parle seul, c'est la
  // bibliothèque qu'il faut corriger — pas lui qu'il faut taire.
  for (const c of chaines(episode)) occurrences.push(...chercherCompact(c));
  return { valide: occurrences.length === 0, occurrences };
}

/** Résumé lisible d'un échec de redaction, pour `grade_reason`. */
export function resumerOccurrences(occurrences: readonly OccurrencePii[]): string {
  const parType = new Map<string, number>();
  for (const o of occurrences) parType.set(o.type, (parType.get(o.type) ?? 0) + 1);
  return [...parType.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([type, n]) => `${n}×${type}`)
    .join(', ');
}
