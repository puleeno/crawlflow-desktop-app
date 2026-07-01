const MARKETPLACE_API = 'https://crawlflow.pages.dev/api';

export interface MarketplaceItem {
    id: number;
    slug: string;
    name: string;
    description: string;
    item_type: 'plugin' | 'template';
    latest_version: string;
    icon_color: string;
    icon_name: string;
    icon_svg?: string;
    rating: number;
    rating_count: number;
    install_count: number;
    author_name: string;
    price: number | null;
    currency: string;
    github_repo: string;
    category: string;
    tags?: string;
    is_featured?: number;
    created_at: string;
}

export interface MarketplaceVersion {
    version: string;
    changelog: string;
    download_url: string;
    created_at: string;
}

export interface MarketplacePagination {
    page: number;
    per_page: number;
    total: number;
    total_pages: number;
    has_next: boolean;
    has_prev: boolean;
}

export interface MarketplaceResponse {
    data: MarketplaceItem[];
    pagination: MarketplacePagination;
}

export async function fetchItems(params?: {
    page?: number;
    perPage?: number;
    type?: 'plugin' | 'template';
    category?: string;
    free?: boolean;
}): Promise<MarketplaceResponse> {
    const search = new URLSearchParams();
    if (params?.page) search.set('page', String(params.page));
    if (params?.perPage) search.set('per_page', String(params.perPage));
    if (params?.type) search.set('type', params.type);
    if (params?.category) search.set('category', params.category);
    if (params?.free) search.set('free', 'true');

    const res = await fetch(`${MARKETPLACE_API}/items?${search}`);
    if (!res.ok) throw new Error(`Failed to fetch items: ${res.statusText}`);
    return res.json();
}

export async function fetchItem(slug: string): Promise<MarketplaceItem | null> {
    const res = await fetch(`${MARKETPLACE_API}/items/${slug}`);
    if (!res.ok) return null;
    return res.json();
}

export async function fetchVersions(slug: string): Promise<MarketplaceVersion[]> {
    const res = await fetch(`${MARKETPLACE_API}/items/${slug}/versions`);
    if (!res.ok) return [];
    const data = await res.json();
    return data.versions || [];
}

export async function resolveDownload(slug: string, version?: string): Promise<string | null> {
    const search = new URLSearchParams();
    if (version) search.set('version', version);
    const res = await fetch(`${MARKETPLACE_API}/items/${slug}/resolve?${search}`);
    if (!res.ok) return null;
    const data = await res.json();
    return data.download_url || null;
}

export async function fetchNews(limit = 5): Promise<any[]> {
    const res = await fetch(`${MARKETPLACE_API}/news?limit=${limit}`);
    if (!res.ok) return [];
    return res.json();
}
