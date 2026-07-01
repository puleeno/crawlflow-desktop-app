import { describe, it, expect, vi } from 'vitest';

describe('Database utilities', () => {
  it('should detect non-Tauri environment', async () => {
    const { isTauri } = await import('../lib/db');
    expect(isTauri()).toBe(false);
  });

  it('should have expected exports', async () => {
    const db = await import('../lib/db');
    const exports = Object.keys(db);
    expect(exports.length).toBeGreaterThan(0);
  });
});
