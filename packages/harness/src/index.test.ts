import { describe, expect, it } from 'vitest';
import { AIDE } from './index.js';

describe('@noe/harness', () => {
  it("l'aide nomme les trois commandes prevues", () => {
    expect(AIDE).toContain('replay');
    expect(AIDE).toContain('judge');
    expect(AIDE).toContain('fixtures');
  });

  it("l'aide rappelle que seul le juge promeut", () => {
    expect(AIDE).toContain('Seul le juge mecanique promeut');
  });
});
