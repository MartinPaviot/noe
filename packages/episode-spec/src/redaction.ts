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

export const VERSION_MOTIFS = 1;

export type MotifPii = {
  readonly type: string;
  readonly source: string;
  readonly drapeaux: string;
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
    note: 'adresse de courriel',
  },
  {
    type: 'TEL_FR',
    source: '(?:\\+33|0)[1-9](?:[ .-]?\\d{2}){4}',
    drapeaux: 'g',
    note: 'numero francais, avec ou sans separateurs',
  },
  {
    type: 'TEL_INTL',
    source: '\\+(?!33)\\d{1,3}[ .-]?\\d{2,4}(?:[ .-]?\\d{2,4}){2,}',
    drapeaux: 'g',
    note: 'numero international hors France',
  },
  {
    type: 'IBAN',
    source: '\\b[A-Z]{2}\\d{2}[A-Z0-9]{10,30}\\b',
    drapeaux: 'g',
    note: 'IBAN : deux lettres pays, deux chiffres de controle, puis le compte',
  },
  {
    type: 'CARTE',
    source: '\\b(?:\\d{4}[ -]?){3}\\d{4}\\b',
    drapeaux: 'g',
    note: 'numero de carte a 16 chiffres',
  },
];

export type OccurrencePii = {
  readonly type: string;
  readonly extrait: string;
  readonly index: number;
};

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
