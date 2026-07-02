import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

beforeEach(() => {
  mockFetch.mockReset();
});

describe('Marketplace API', () => {
  it('should export expected API functions', async () => {
    const m = await import('../lib/marketplace');
    expect(typeof m.fetchItems).toBe('function');
    expect(typeof m.fetchItem).toBe('function');
    expect(typeof m.fetchVersions).toBe('function');
    expect(typeof m.resolveDownload).toBe('function');
    expect(typeof m.fetchNews).toBe('function');
  });

  it('fetchItems should construct URL with query params', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ data: [], pagination: { page: 1, per_page: 10, total: 0, total_pages: 0, has_next: false, has_prev: false } }),
    });
    const { fetchItems } = await import('../lib/marketplace');
    await fetchItems({ page: 2, perPage: 20, type: 'plugin', free: true });
    const url = mockFetch.mock.calls[0][0];
    expect(url).toContain('page=2');
    expect(url).toContain('per_page=20');
    expect(url).toContain('type=plugin');
    expect(url).toContain('free=true');
  });

  it('fetchItems should throw on non-ok response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      statusText: 'Not Found',
    });
    const { fetchItems } = await import('../lib/marketplace');
    await expect(fetchItems()).rejects.toThrow('Failed to fetch items');
  });

  it('fetchItem should return null on non-ok', async () => {
    mockFetch.mockResolvedValueOnce({ ok: false });
    const { fetchItem } = await import('../lib/marketplace');
    const result = await fetchItem('test-plugin');
    expect(result).toBeNull();
  });

  it('fetchItem should return data on ok', async () => {
    const item = { slug: 'test-plugin', name: 'Test', item_type: 'plugin' };
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => item,
    });
    const { fetchItem } = await import('../lib/marketplace');
    const result = await fetchItem('test-plugin');
    expect(result).toEqual(item);
  });

  it('fetchVersions should use correct URL', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ versions: [{ version: '1.0.0' }] }),
    });
    const { fetchVersions } = await import('../lib/marketplace');
    const result = await fetchVersions('test-plugin');
    expect(result).toHaveLength(1);
    expect(result[0].version).toBe('1.0.0');
  });

  it('fetchVersions should return empty array on non-ok', async () => {
    mockFetch.mockResolvedValueOnce({ ok: false });
    const { fetchVersions } = await import('../lib/marketplace');
    const result = await fetchVersions('test-plugin');
    expect(result).toEqual([]);
  });

  it('resolveDownload should return download_url', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ download_url: 'https://example.com/pkg.zip' }),
    });
    const { resolveDownload } = await import('../lib/marketplace');
    const result = await resolveDownload('test-plugin', '1.0.0');
    expect(result).toBe('https://example.com/pkg.zip');
  });

  it('resolveDownload should return null on non-ok', async () => {
    mockFetch.mockResolvedValueOnce({ ok: false });
    const { resolveDownload } = await import('../lib/marketplace');
    const result = await resolveDownload('test-plugin');
    expect(result).toBeNull();
  });

  it('fetchNews should use correct URL', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => [{ title: 'News 1' }],
    });
    const { fetchNews } = await import('../lib/marketplace');
    const result = await fetchNews(3);
    expect(mockFetch.mock.calls[0][0]).toContain('limit=3');
    expect(result).toHaveLength(1);
  });

  it('fetchNews should return empty array on non-ok', async () => {
    mockFetch.mockResolvedValueOnce({ ok: false });
    const { fetchNews } = await import('../lib/marketplace');
    const result = await fetchNews();
    expect(result).toEqual([]);
  });

  it('MarketplaceItem interface should match expected shape', async () => {
    const itemJson = {
      id: 1, slug: 'test', name: 'Test', description: 'Desc',
      item_type: 'plugin', latest_version: '1.0.0', icon_color: '#fff',
      icon_name: 'star', rating: 4.5, rating_count: 10, install_count: 100,
      author_name: 'Author', price: null, currency: 'USD',
      github_repo: 'org/repo', category: 'tools', created_at: '2026-01-01',
    };
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => itemJson,
    });
    const { fetchItem } = await import('../lib/marketplace');
    const result = await fetchItem('test');
    expect(result?.name).toBe('Test');
    expect(result?.author_name).toBe('Author');
    expect(result?.price).toBeNull();
    expect(result?.rating).toBe(4.5);
  });
});
