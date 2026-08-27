/**
 * La résolution des entités candidates (spec 003, R2).
 *
 * **Une entité résolue de travers est pire qu'une entité non résolue.** La
 * seconde le dit ; la première attribue le travail d'un opérateur au dossier de
 * quelqu'un d'autre, et rien en aval ne peut le rattraper — le graphe est faux et
 * il a l'air juste. C'est pourquoi R2.2 interdit la devinette et pourquoi ce
 * module n'a ni score, ni distance d'édition, ni « meilleur candidat ».
 *
 * ## Comparer sans exposer
 *
 * R6.2 : les valeurs d'identification lues des APIs sont **tokenisées à la
 * volée** et comparées en tokens. La valeur claire ne vit qu'en mémoire, jamais
 * persistée. Deux graphies d'une même adresse doivent donc converger vers le même
 * jeton **avant** le hachage, sinon « Jean.Dupont@Exemple.FR » et
 * « jean.dupont@exemple.fr » seraient deux personnes.
 */
import type { ApiRef, EntityCandidate, Resolution, StrongKey } from './connector.js';

/**
 * Normalise une valeur d'identification avant tokenisation.
 *
 * **Les mêmes règles des deux côtés**, sinon la comparaison en tokens ne compare
 * rien. C'est la même leçon que la spec 002 : deux moteurs qui normalisent
 * différemment produisent deux jetons pour une entité, et la jointure est perdue
 * sans que personne ne le voie.
 */
export function normaliserIdentifiant(kind: StrongKey['kind'], valeur: string): string {
  switch (kind) {
    case 'email_token':
      // La casse d'une adresse n'est pas significative en pratique. Les espaces
      // de bordure viennent des copier-coller et n'appartiennent à personne.
      return valeur.trim().toLowerCase();
    case 'domain_name':
      return valeur.trim().toLowerCase();
    case 'system_id':
      // Un identifiant système est opaque : on ne touche pas à sa casse, elle
      // peut être significative. Seuls les blancs de bordure partent.
      return valeur.trim();
  }
}

/** Un enregistrement candidat, tel qu'un adaptateur le remonte. */
export type CandidatDistant = {
  readonly ref: ApiRef;
  /** Ses clés fortes, **déjà tokenisées** quand elles portent une identité. */
  readonly keys: readonly StrongKey[];
};

/** Deux clés fortes désignent-elles la même chose ? */
export function memeCle(a: StrongKey, b: StrongKey): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === 'domain_name' && b.kind === 'domain_name') {
    return (
      normaliserIdentifiant('domain_name', a.domain) ===
        normaliserIdentifiant('domain_name', b.domain) &&
      normaliserIdentifiant('domain_name', a.name) === normaliserIdentifiant('domain_name', b.name)
    );
  }
  if (a.kind !== 'domain_name' && b.kind !== 'domain_name') {
    return normaliserIdentifiant(a.kind, a.value) === normaliserIdentifiant(b.kind, b.value);
  }
  return false;
}

/**
 * L'ordre dans lequel les clés tranchent.
 *
 * L'identifiant système d'abord : c'est le système lui-même qui l'a émis, il ne
 * peut pas désigner deux enregistrements. Le courriel ensuite. Le couple domaine
 * + nom en dernier, parce que deux personnes peuvent porter le même nom dans la
 * même entreprise — c'est la clé la plus faible des trois fortes.
 */
const PRIORITE: readonly StrongKey['kind'][] = ['system_id', 'email_token', 'domain_name'];

/**
 * Résout une candidate contre une liste d'enregistrements distants.
 *
 * Les clés sont essayées **dans l'ordre**, et la première qui donne exactement un
 * candidat tranche. Une clé qui en donne plusieurs n'est pas départagée par la
 * suivante : R2.2 dit que l'ambiguïté reste une ambiguïté, et affiner avec une
 * clé plus faible reviendrait exactement à deviner.
 */
export function resoudre(
  candidate: EntityCandidate,
  distants: readonly CandidatDistant[],
  maintenant: string,
): Resolution {
  for (const kind of PRIORITE) {
    const nôtres = candidate.keys.filter((k) => k.kind === kind);
    if (nôtres.length === 0) continue;

    const trouves = distants.filter((d) =>
      d.keys.some((dk) => nôtres.some((nk) => memeCle(nk, dk))),
    );
    if (trouves.length === 1) {
      const ref = trouves[0]?.ref;
      if (ref === undefined) continue;
      return { status: 'resolved', ref, by: kind, at: maintenant };
    }
    if (trouves.length >= 2) {
      // On s'ARRÊTE. Essayer la clé suivante pour départager, c'est laisser une
      // clé plus faible trancher là où une clé plus forte a échoué.
      return { status: 'ambiguous', count: trouves.length };
    }
  }
  return { status: 'not_found' };
}

/**
 * Le résumé d'une résolution, pour l'épisode.
 *
 * `not_found` et `ambiguous:2` n'appellent pas le même geste : le premier veut
 * dire « cet enregistrement n'existe pas ou n'est pas visible », le second « il y
 * en a trop et il faut une clé plus forte ». Un épisode qui dirait seulement
 * « non résolu » laisserait chercher au mauvais endroit.
 */
export function raison(r: Resolution): string {
  switch (r.status) {
    case 'resolved':
      return `resolue par ${r.by} le ${r.at}`;
    case 'not_found':
      return 'not_found';
    case 'ambiguous':
      return `ambiguous:${r.count}`;
  }
}
