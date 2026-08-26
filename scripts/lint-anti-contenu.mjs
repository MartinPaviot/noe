/**
 * Lint anti-contenu — INVARIANT I rendu mécanique.
 *
 * Refuse toute migration créant une colonne serveur capable d'accueillir du
 * contenu utilisateur : un épisode, un courriel, un enregistrement CRM, un
 * fragment de travail réel.
 *
 * Deux règles :
 *   1. Les types « fourre-tout » (json, jsonb, xml, bytea) sont interdits.
 *      Ce sont les conteneurs naturels d'un contenu arbitraire.
 *   2. Les colonnes textuelles dont le nom évoque du contenu sont interdites.
 *
 * Échappatoire volontaire : une colonne peut être autorisée explicitement en
 * plaçant `-- noe:contenu-autorise <raison>` sur la ligne précédente. C'est un
 * choix conscient, tracé dans le diff, pas un contournement silencieux.
 */

/** Types capables d'accueillir un contenu arbitraire. */
const TYPES_INTERDITS = ['jsonb', 'json', 'xml', 'bytea', 'hstore'];

/** Fragments de noms qui trahissent du contenu utilisateur. */
const NOMS_INTERDITS = [
  'contenu',
  'content',
  'body',
  'corps',
  'texte',
  'text_brut',
  'message',
  'sujet',
  'subject',
  'objet_mail',
  'episode',
  'transcript',
  'transcription',
  'note',
  'notes',
  'commentaire',
  'comment',
  'brouillon',
  'draft',
  'payload',
  'donnees_brutes',
  'raw',
  'extrait',
  'snippet',
  'resume',
  'summary',
];

const MARQUEUR_AUTORISE = 'noe:contenu-autorise';

/**
 * Analyse un contenu SQL et retourne la liste des violations.
 * @param {string} sql
 * @param {string} fichier
 * @returns {{fichier: string, ligne: number, colonne: string, motif: string, source: string}[]}
 */
export function analyser(sql, fichier = '<sql>') {
  const lignes = sql.split(/\r?\n/);
  const violations = [];
  let dansCreateTable = false;

  for (let i = 0; i < lignes.length; i++) {
    const brute = lignes[i] ?? '';
    const ligne = brute.trim();

    if (/^create\s+table/i.test(ligne)) dansCreateTable = true;
    if (dansCreateTable && /^\)\s*;/.test(ligne)) dansCreateTable = false;

    // `add column` compte aussi, hors de tout create table.
    const estAjout = /alter\s+table[\s\S]*add\s+column/i.test(ligne);
    if (!dansCreateTable && !estAjout) continue;
    if (ligne.startsWith('--') || ligne === '') continue;

    // Une autorisation explicite sur la ligne precedente leve le blocage.
    const precedente = (lignes[i - 1] ?? '').trim();
    if (precedente.includes(MARQUEUR_AUTORISE)) continue;

    // Sur un `alter table … add column X TYPE`, on repart apres « add column » :
    // sinon le nom de table serait pris pour le nom de colonne.
    const candidate = estAjout ? ligne.replace(/^[\s\S]*?add\s+column\s+/i, '') : ligne;

    const m = candidate.match(/^"?([a-z_][a-z0-9_]*)"?\s+([a-z][a-z0-9_ ]*)/i);
    if (!m) continue;

    const colonne = (m[1] ?? '').toLowerCase();
    const type = (m[2] ?? '').toLowerCase().trim();

    // Mots-cles SQL pris a tort pour des colonnes.
    if (
      ['constraint', 'primary', 'unique', 'foreign', 'check', 'create', 'alter'].includes(colonne)
    ) {
      continue;
    }

    const typeInterdit = TYPES_INTERDITS.find((t) => new RegExp(`\\b${t}\\b`).test(type));
    if (typeInterdit) {
      violations.push({
        fichier,
        ligne: i + 1,
        colonne,
        motif: `type « ${typeInterdit} » : conteneur de contenu arbitraire`,
        source: ligne,
      });
      continue;
    }

    const nomInterdit = NOMS_INTERDITS.find(
      (n) => colonne === n || colonne.startsWith(`${n}_`) || colonne.endsWith(`_${n}`),
    );
    if (nomInterdit && /\b(text|varchar|char)\b/.test(type)) {
      violations.push({
        fichier,
        ligne: i + 1,
        colonne,
        motif: `nom « ${nomInterdit} » : evoque du contenu utilisateur`,
        source: ligne,
      });
    }
  }

  return violations;
}
