import { describe, it, expect } from 'vitest';

describe('Presets', () => {
  it('should export PRESETS as Record and PROCESSORS as array', async () => {
    const { PRESETS, PROCESSORS } = await import('../presets');
    expect(typeof PRESETS).toBe('object');
    expect(Array.isArray(PROCESSORS)).toBe(true);
  });

  it('should have presets with name and description', async () => {
    const { PRESETS } = await import('../presets');
    const keys = Object.keys(PRESETS);
    expect(keys.length).toBeGreaterThanOrEqual(1);
    for (const key of keys) {
      const preset = PRESETS[key];
      expect(preset).toHaveProperty('name');
      expect(preset).toHaveProperty('html');
      expect(preset).toHaveProperty('json');
    }
  });

  it('should have processors array with entries', async () => {
    const { PROCESSORS } = await import('../presets');
    expect(PROCESSORS.length).toBeGreaterThan(0);
    for (const proc of PROCESSORS) {
      expect(proc).toHaveProperty('id');
      expect(proc).toHaveProperty('name');
    }
  });
});
