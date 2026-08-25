import { describe, expect, it } from 'vitest';
import { DOMAIN_VERSION } from './index.js';

describe('@noe/core', () => {
  it('expose une version de domaine', () => {
    expect(DOMAIN_VERSION).toBe('0.0.0');
  });
});
