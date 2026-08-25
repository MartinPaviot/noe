import { describe, expect, it } from 'vitest';
import { EPISODE_FORMAT_VERSION } from './index.js';

describe('@noe/episode-spec', () => {
  it('expose une version de format', () => {
    expect(EPISODE_FORMAT_VERSION).toBe('0.0.0');
  });
});
