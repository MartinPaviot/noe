import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { Episode } from '@noe/episode-spec';

/**
 * Générateur de corpus synthétique, pour la mesure de performance (R3.2).
 *
 * Ces épisodes ne servent qu'à charger le harness : ils ne remplacent pas le
 * corpus doré, qui lui est écrit à la main et couvre des cas réels.
 */

const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

/**
 * ULID déterministe à partir d'un index. Le déterminisme est le but, pas
 * l'unicité cosmique. Longueur exacte : 26 caractères — un ULID plus court est
 * refusé au parse, et le corpus entier finirait en « illisible ».
 */
const PREFIXE = '01JQB';
const LONGUEUR_SUFFIXE = 8;

function ulidDeterministe(i: number): string {
  let reste = i;
  const suffixe: string[] = [];
  for (let k = 0; k < LONGUEUR_SUFFIXE; k++) {
    suffixe.unshift(ALPHABET[reste % 32] ?? '0');
    reste = Math.floor(reste / 32);
  }
  const remplissage = '0'.repeat(26 - PREFIXE.length - LONGUEUR_SUFFIXE);
  return `${PREFIXE}${remplissage}${suffixe.join('')}`;
}

export function episodeSynthetique(i: number): Episode {
  const base = Date.UTC(2026, 7, 1, 9, 0, 0);
  const t0 = new Date(base + i * 600_000);
  const t1 = new Date(base + i * 600_000 + 300_000);
  const ts = new Date(base + i * 600_000 + 60_000);

  return {
    schema_v: 1,
    id: ulidDeterministe(i),
    task_slug: 'maj-crm-post-echange',
    t0: t0.toISOString(),
    t1: t1.toISOString(),
    events: [
      {
        schema_v: 1,
        kind: 'ui_action',
        seq: 0,
        ts: ts.toISOString(),
        source: 'ui',
        action: 'submit',
        target: { role: 'button', name: 'Enregistrer' },
      },
      {
        schema_v: 1,
        kind: 'api_change',
        seq: 1,
        ts: ts.toISOString(),
        source: 'api',
        connector: 'crm',
        object: 'contact',
        object_id: `c_${i}`,
        fields_changed: ['statut', 'prochaine_action'],
      },
    ],
    entities: [
      {
        key: { type: 'contact', value_pseudo: `PSEUDO_SYNTH_${i}` },
        first_seen_seq: 0,
        api_refs: [{ connector: 'crm', object: 'contact', id: `c_${i}` }],
        state_before: { statut: 'nouveau', prochaine_action: '', date_relance: null, notes: '' },
        state_after: {
          statut: i % 3 === 0 ? 'objection' : 'qualifie',
          prochaine_action: `action ${i}`,
          date_relance: null,
          notes: '',
        },
      },
    ],
    grade: 'A',
    grade_reason: 'sequence sans trou, toutes entites resolues, redaction validee',
    scope_fields: ['statut', 'prochaine_action', 'date_relance', 'notes'],
    completeness: { explained: 2, out_of_scope: 0, gaps: 0 },
  };
}

/** Écrit `n` épisodes synthétiques dans un dossier. Noms triés, donc rejeu stable. */
export async function genererCorpus(dossier: string, n: number): Promise<void> {
  for (let i = 0; i < n; i++) {
    const nom = `synth_${String(i).padStart(4, '0')}.json`;
    await writeFile(join(dossier, nom), JSON.stringify(episodeSynthetique(i), null, 2), 'utf8');
  }
}
