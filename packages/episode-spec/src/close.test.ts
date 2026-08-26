import { describe, expect, it } from 'vitest';
import { cloturer, estClos, remplacer } from './close.js';
import { episodeAvecTrou, episodeValide } from './fixtures.js';
import { Episode } from './schema.js';

describe('cloture — immutabilite (R1.4)', () => {
  it('gele l episode', () => {
    expect(estClos(cloturer(episodeValide()))).toBe(true);
  });

  it('gele en profondeur : les evenements aussi', () => {
    const ep = cloturer(episodeValide());
    expect(Object.isFrozen(ep.events)).toBe(true);
    expect(Object.isFrozen(ep.events[0])).toBe(true);
    expect(Object.isFrozen(ep.entities[0]?.state_after)).toBe(true);
  });

  it('refuse toute mutation post-cloture, en mode strict', () => {
    const ep = cloturer(episodeValide());
    expect(() => {
      (ep as unknown as { task_slug: string }).task_slug = 'autre';
    }).toThrow();
  });

  it('refuse la mutation d un champ imbrique', () => {
    const ep = cloturer(episodeValide());
    expect(() => {
      (ep.events[0] as unknown as { seq: number }).seq = 99;
    }).toThrow();
  });

  it('recalcule le grade a la cloture plutot que de faire confiance au declare', () => {
    // On declare A sur un episode qui porte un trou : la cloture retablit B.
    const menteur = { ...episodeAvecTrou(), grade: 'A' as const, grade_reason: 'mensonge' };
    const clos = cloturer(menteur);
    expect(clos.grade).toBe('B');
    expect(clos.grade_reason).toContain('1 trou de capture');
  });
});

describe('supersedes — la seule correction legitime (R1.4)', () => {
  it('produit un nouvel episode plutot que de modifier l ancien', () => {
    const ancien = cloturer(episodeValide());
    const nouveau = remplacer(ancien, { task_slug: 'maj-crm-corrige' });

    expect(nouveau.id).not.toBe(ancien.id);
    expect(nouveau.supersedes).toBe(ancien.id);
    expect(nouveau.task_slug).toBe('maj-crm-corrige');
    // L'ancien n'a pas bouge d'un iota.
    expect(ancien.task_slug).toBe('maj-crm-post-echange');
    expect(ancien.supersedes).toBeUndefined();
  });

  it('le remplacant est lui-meme clos', () => {
    const nouveau = remplacer(cloturer(episodeValide()), {});
    expect(estClos(nouveau)).toBe(true);
  });

  it('le remplacant reste valide au schema', () => {
    const nouveau = remplacer(cloturer(episodeValide()), { task_slug: 'autre-tache' });
    expect(Episode.safeParse(nouveau).success).toBe(true);
  });

  it('une chaine de remplacements garde la trace du precedent', () => {
    const v1 = cloturer(episodeValide());
    const v2 = remplacer(v1, { task_slug: 'v2' });
    const v3 = remplacer(v2, { task_slug: 'v3' });
    expect(v3.supersedes).toBe(v2.id);
    expect(v2.supersedes).toBe(v1.id);
  });
});
