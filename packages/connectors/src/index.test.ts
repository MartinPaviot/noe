import { describe, expect, it } from 'vitest';
import { SYSTEMES_ENVISAGES } from './index.js';

describe('@noe/connectors', () => {
  it("n'implemente aucun connecteur en session 0", () => {
    expect(SYSTEMES_ENVISAGES).toHaveLength(2);
  });
});
