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
 * Version 3.
 *
 * - **v2** : `TEL_FR` accepte un séparateur après `+33`. La v1 laissait passer
 *   « +33 6 12 34 56 78 » en clair.
 * - **v3** : plus aucune anticipation dans la bibliothèque, et arbitrage des
 *   chevauchements par `priorite`. Le moteur de Rust ne connaît pas
 *   l'anticipation ; une bibliothèque censée être lue par trois moteurs doit
 *   tenir dans leur sous-ensemble commun.
 */
export const VERSION_MOTIFS = 3;

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
    source: '(?:\\+33[ .-]?|0)[1-9](?:[ .-]?\\d{2}){4}',
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
 */
export function chercherPii(texte: string): OccurrencePii[] {
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
  const occurrences = chercherPii(JSON.stringify(episode));
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
