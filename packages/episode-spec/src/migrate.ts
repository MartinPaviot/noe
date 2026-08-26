import { Episode, gradeOf, SCHEMA_V } from './schema.js';

/**
 * Échec de migration. Jamais de lecture partielle silencieuse (R1.5) : soit
 * l'épisode arrive intact en version courante, soit le chargement échoue en
 * disant précisément ce qui manque.
 */
export class MigrationError extends Error {
  readonly trouvee: number;
  readonly attendue: number;

  constructor(message: string, trouvee: number, attendue: number) {
    super(message);
    this.name = 'MigrationError';
    this.trouvee = trouvee;
    this.attendue = attendue;
  }
}

type Migrateur = (ancien: Record<string, unknown>) => Record<string, unknown>;

/**
 * Migrateur 0 → 1.
 *
 * La v0 ne portait ni `scope_fields` ni `grade_reason` : le périmètre était
 * implicite et le déclassement non motivé. On reconstruit le premier depuis les
 * champs réellement touchés, et le second en rejouant les règles de grade.
 */
const de0vers1: Migrateur = (ancien) => {
  const events = Array.isArray(ancien.events) ? (ancien.events as Record<string, unknown>[]) : [];
  const entities = Array.isArray(ancien.entities)
    ? (ancien.entities as Record<string, unknown>[])
    : [];

  const champs = new Set<string>();
  for (const ev of events) {
    if (ev.kind === 'api_change' && Array.isArray(ev.fields_changed)) {
      for (const f of ev.fields_changed as string[]) champs.add(f);
    }
  }

  // On passe l objet COMPLET : le validateur de redaction (R4.6) balaye
  // l episode serialise, pas seulement events + entities.
  const verdict = gradeOf({
    ...(ancien as object),
    events: events as { kind: string }[],
    entities: entities as {
      key: { value_pseudo: string };
      state_before?: unknown;
      state_after?: unknown;
    }[],
  });

  return {
    ...ancien,
    schema_v: 1,
    events: events.map((e) => ({ ...e, schema_v: 1 })),
    scope_fields: [...champs].sort(),
    grade: verdict.grade,
    grade_reason: verdict.reason,
  };
};

/** Registre des migrateurs. La clé est la version de DÉPART. */
const MIGRATEURS: Record<number, Migrateur> = {
  0: de0vers1,
};

/**
 * Charge un épisode brut, en le migrant si besoin jusqu'à `SCHEMA_V`, puis en le
 * validant. Toute anomalie lève — jamais de dégradation silencieuse.
 */
export function load(brut: unknown): Episode {
  if (typeof brut !== 'object' || brut === null) {
    throw new MigrationError('episode illisible : ce n est pas un objet', Number.NaN, SCHEMA_V);
  }

  let courant = brut as Record<string, unknown>;
  const versionInitiale = courant.schema_v;

  if (typeof versionInitiale !== 'number' || !Number.isInteger(versionInitiale)) {
    throw new MigrationError(
      `schema_v absent ou non entier (recu : ${JSON.stringify(versionInitiale)})`,
      Number.NaN,
      SCHEMA_V,
    );
  }

  if (versionInitiale > SCHEMA_V) {
    throw new MigrationError(
      `episode en schema_v ${versionInitiale}, plus recent que le format supporte (${SCHEMA_V}). Mettez Noe a jour.`,
      versionInitiale,
      SCHEMA_V,
    );
  }

  let v = versionInitiale;
  while (v < SCHEMA_V) {
    const migrateur = MIGRATEURS[v];
    if (migrateur === undefined) {
      throw new MigrationError(
        `aucun migrateur enregistre pour passer de schema_v ${v} a ${v + 1}`,
        versionInitiale,
        SCHEMA_V,
      );
    }
    courant = migrateur(courant);
    const suivante = courant.schema_v;
    if (suivante !== v + 1) {
      throw new MigrationError(
        `le migrateur ${v} -> ${v + 1} n a pas produit la version attendue (obtenu : ${String(suivante)})`,
        versionInitiale,
        SCHEMA_V,
      );
    }
    v = suivante;
  }

  return Episode.parse(courant);
}

/** Versions depuis lesquelles une migration est possible. Utile aux diagnostics. */
export function versionsMigrables(): number[] {
  return Object.keys(MIGRATEURS)
    .map(Number)
    .sort((a, b) => a - b);
}
