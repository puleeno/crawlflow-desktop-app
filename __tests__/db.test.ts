import { describe, it, expect, vi } from 'vitest';

describe('Database utilities', () => {
  it('should detect non-Tauri environment', async () => {
    const { isTauri } = await import('../lib/db');
    expect(isTauri()).toBe(false);
  });

  it('should initialize master schema on first run', async () => {
    const { ensureMasterDbSchema } = await import('../lib/db');
    const execute = vi.fn().mockResolvedValue(undefined);
    const db = { execute };

    await ensureMasterDbSchema(db as any);

    expect(execute).toHaveBeenCalled();
    expect(execute.mock.calls.some(([sql]: [string]) => sql.includes('CREATE TABLE IF NOT EXISTS projects'))).toBe(true);
  });

  it('should have expected exports', async () => {
    const db = await import('../lib/db');
    const exports = Object.keys(db);
    expect(exports.length).toBeGreaterThan(0);
  });
});
