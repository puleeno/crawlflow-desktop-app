"""
Oreka Shop Crawler - CrawlFlow Python Plugin

Crawl toan bo san pham tu mot shop tren oreka.vn,
parse du lieu va xuat ra file Excel voi co che append + check trung.

Flow:
  1. fetch_data()  - Crawl tat ca san pham tu shop (phan trang)
  2. process_data() - Chuan hoa du lieu
  3. export_data()  - Excel + dedup + cap nhat progress

Config:
  shop_url: str (bat buoc)  - URL cua shop, VD: https://oreka.vn/shop/ten-shop
  max_pages: int (mặc định: 50)
  delay_ms: int (mặc định: 1500)
  selectors: dict (tuỳ chon) - Ghi de selector mac dinh
  output_dir: str (mặc định: thu muc hien tai)
  project_id: str (tu dong truyen tu service)

KET QUA TRA VE (fetch_data tra ve JSON array, moi phan tu la 1 san pham):
  {
    "url": "https://oreka.vn/product/...",
    "name": "Ten san pham",
    "price": 299000,
    "old_price": 399000,
    "image": "https://oreka.vn/.../img.jpg",
    "sku": "SP001",
    "description": "Mo ta san pham...",
    "specs": { "Thuong hieu": "...", "Chat lieu": "..." },
    "category": "Danh muc",
    "availability": "Con hang",
    "stock": "1",
    "crawled_at": "2026-07-02T12:00:00"
  }

XUAT EXCEL/CSV:
  Cot: STT, Gia bia, Gia ban, Ton kho, Don vi,
       Khoi luong (g), Kich thuoc, Tinh trang, Nam XB,
       Thuong hieu, Ten sach, Danh muc
"""

import json
import hashlib
import time
import os
import re
import urllib.parse
from datetime import datetime
from html import unescape
from html.parser import HTMLParser


def register_presets():
    """Register preset for Oreka Shop Crawler plugin."""
    preset = {
        "id": "oreka-shop-crawler",
        "name": "Oreka Shop Crawler",
        "description": "Crawl sản phẩm từ shop oreka.vn với custom extraction rules và export ra Excel",
        "icon": "ShoppingCartIcon",
        "icon_color": "#10b981",
        "project_settings": {
            "name": "Oreka Shop - {shop_name}",
            "description": "Crawl sản phẩm từ shop oreka.vn",
            "crawlDelay": 1500,
            "userAgent": "CrawlFlow/1.0",
            "concurrency": 1,
            "executionMode": "queue",
        },
        "nodes": [
            {
                "id": "ds-oreka",
                "type": "start",
                "label": "Oreka Shop Source",
                "position": {"x": 50, "y": 50},
                "data": {
                    "pluginSourceType": "oreka-shop-crawler",
                    "sourceType": "url",
                    "sourceValue": "",
                    "pluginConfig": {
                        "shop_url": ""
                    },
                    "urlSettings": {
                        "httpClient": {
                            "clientType": "reqwest",
                            "headless": False,
                        },
                    },
                },
            },
            {
                "id": "pre-1",
                "type": "preprocessor",
                "label": "Preprocess HTML",
                "position": {"x": -568, "y": 38},
                "data": {
                    "inputType": "html",
                    "itemSelector": ".mt-12.grid.grid-cols-5.gap-10",
                    "csvDelimiter": ",",
                    "csvHasHeader": True,
                    "jsonItemPath": "",
                    "urlPatterns": [
                        {
                            "enabled": True,
                            "type": "regex",
                            "value": ".*-detail\\/[0-9]{1,}\\/?",
                        },
                    ],
                    "extractRules": [],
                },
            },
            {
                "id": "repository-node",
                "type": "repository",
                "label": "Raw Data Repository",
                "position": {"x": 50, "y": 329},
                "data": {},
            },
            {
                "id": "worker-1",
                "type": "worker",
                "label": "Product Detail Filter",
                "position": {"x": 40, "y": 641},
                "data": {
                    "detectionLogic": "and",
                    "detectionRules": [
                        {
                            "id": "1783651265684",
                            "type": "url-format",
                            "selector": "",
                            "condition": "exists",
                            "value": "",
                            "pattern": ".*-detail\\/[0-9]{1,}\\/?",
                        },
                    ],
                },
            },
            {
                "id": "ext-1",
                "type": "html-data-extractor",
                "label": "Extract Product Data",
                "position": {"x": -423, "y": 426},
                "data": {
                    "presets": ["ecommerce-product"],
                    "customRules": [
                        {
                            "id": "preset-ecom-html-1",
                            "name": "product_name",
                            "extractFrom": "html-element",
                            "selector": "h1.styles_nameProduct__QSdsj.mt-2",
                            "extract": "text",
                        },
                        {
                            "id": "preset-ecom-html-2",
                            "name": "price",
                            "extractFrom": "html-element",
                            "selector": "p.font-semibold.text-16.leading-8.text-black-600.line-clamp-1.break-all.styles_productPrice__zkPlt",
                            "extract": "text",
                        },
                        {
                            "id": "preset-ecom-html-3",
                            "name": "sku",
                            "extractFrom": "html-element",
                            "selector": ".sku, .product-sku",
                            "extract": "text",
                        },
                        {
                            "id": "preset-ecom-html-4",
                            "name": "description",
                            "extractFrom": "html-element",
                            "selector": "div.mt-6.whitespace-pre-wrap > p.text",
                            "extract": "html",
                        },
                        {
                            "id": "preset-ecom-html-5",
                            "name": "image_url",
                            "extractFrom": "html-element",
                            "selector": "img.styles_imageSlide__AUZey.object-cover.rounded-md",
                            "extract": "attribute",
                            "attribute": "src",
                        },
                        {
                            "id": "preset-ecom-html-6",
                            "name": "images",
                            "extractFrom": "html-element",
                            "selector": ".image-gallery-thumbnail img",
                            "extract": "attribute",
                            "attribute": "src",
                            "extractMultiple": True,
                        },
                    ],
                },
            },
            {
                "id": "proc-1",
                "type": "processor",
                "label": "Excel Export",
                "position": {"x": 52, "y": 914},
                "data": {
                    "processorType": "generate-excel-file",
                    "settings": {
                        "fileName": "crawl_results_{{date}}.xlsx",
                        "sheetName": "Sheet1",
                        "includeHeader": True,
                        "autoMapHeaders": True,
                        "columnMapping": {},
                    },
                },
            },
        ],
        "edges": [
            {"id": "e-ds-pre", "source": "ds-oreka", "target": "pre-1"},
            {"id": "e-pre-repo", "source": "pre-1", "target": "repository-node"},
            {"id": "e-repo-worker", "source": "repository-node", "target": "worker-1"},
            {"id": "e-ext-worker", "source": "ext-1", "target": "worker-1"},
            {"id": "e-worker-proc", "source": "worker-1", "target": "proc-1"},
        ],
    }
    return json.dumps([preset])


def _add_page_to_url(url, page_param, page_num):
    parsed = urllib.parse.urlparse(url)
    qd = urllib.parse.parse_qs(parsed.query)
    qd[page_param] = [str(page_num)]
    new_query = urllib.parse.urlencode(qd, doseq=True)
    return urllib.parse.urlunparse((
        parsed.scheme,
        parsed.netloc,
        parsed.path,
        parsed.params,
        new_query,
        parsed.fragment
    ))


def find_store_id(data):
    if isinstance(data, dict):
        if "store" in data and isinstance(data["store"], dict) and "id" in data["store"]:
            return data["store"]["id"]
        if "storeId" in data:
            return data["storeId"]
        for val in data.values():
            res = find_store_id(val)
            if res:
                return res
    elif isinstance(data, list):
        for val in data:
            res = find_store_id(val)
            if res:
                return res
    return None


def _find_apollo_store_id(data):
    """Lay store ID tu __APOLLO_STATE__ cua Oreka Next.js page."""
    if isinstance(data, dict):
        apollo_state = data.get("__APOLLO_STATE__")
        if isinstance(apollo_state, dict):
            for cache_key, record in apollo_state.items():
                if not cache_key.startswith("Store:"):
                    continue
                store_id = record.get("id") if isinstance(record, dict) else None
                return str(store_id or cache_key.removeprefix("Store:")).strip()
        for value in data.values():
            store_id = _find_apollo_store_id(value)
            if store_id:
                return store_id
    elif isinstance(data, list):
        for value in data:
            store_id = _find_apollo_store_id(value)
            if store_id:
                return store_id
    return None


def _extract_store_slug_from_url(url):
    """Extract store slug from Oreka store URL."""
    # Pattern: https://www.oreka.vn/store/SLUG or /store/SLUG
    match = re.search(r'/store/([A-Za-z0-9\-\.]+)', url)
    if match:
        return match.group(1)
    return None


def _extract_store_id_from_next_data(html):
    """Parse JSON trong script __NEXT_DATA__ va lay Store:<id> tu Apollo cache."""
    match = re.search(
        r'<script\b(?=[^>]*\bid=["\']__NEXT_DATA__["\'])[^>]*>(.*?)</script>',
        html,
        re.DOTALL | re.IGNORECASE,
    )
    if not match:
        return None

    try:
        next_data = json.loads(unescape(match.group(1)).strip())
    except (TypeError, ValueError, json.JSONDecodeError):
        return None

    apollo_state = (
        next_data.get("props", {})
        .get("pageProps", {})
        .get("__APOLLO_STATE__", {})
    )
    if not isinstance(apollo_state, dict):
        return _find_apollo_store_id(next_data)

    for cache_key, record in apollo_state.items():
        if cache_key.startswith("Store:"):
            store_id = record.get("id") if isinstance(record, dict) else None
            return str(store_id or cache_key.removeprefix("Store:")).strip()
    return None


def _extract_store_id_from_html(html):
    """Lay store ID tu HTML trang Oreka, uu tien du lieu Next.js."""
    html = unescape(html)
    store_id = _extract_store_id_from_next_data(html)
    if store_id:
        return store_id

    # Direct fallback for Apollo Store UUID pattern (e.g. Store:950fc091-0766-4b5e-af6a-ad7b5325a1fb)
    uuid_match = re.search(r'["\']Store:([a-fA-F0-9\-]{36})["\']', html)
    if uuid_match:
        return uuid_match.group(1).strip()

    # Fallback to look inside social meta og:image URL pattern (e.g. store-950fc091-0766-4b5e-af6a-ad7b5325a1fb.webp)
    meta_match = re.search(r'store-([a-fA-F0-9\-]{36})\.', html)
    if meta_match:
        return meta_match.group(1).strip()

    for script_content in re.findall(r'<script\b[^>]*>(.*?)</script>', html, re.DOTALL | re.IGNORECASE):
        if "storeId" not in script_content and '"store"' not in script_content:
            continue
        try:
            script_data = json.loads(script_content.strip())
            store_id = _find_apollo_store_id(script_data) or find_store_id(script_data)
            if store_id:
                return str(store_id).strip()
        except (TypeError, ValueError, json.JSONDecodeError):
            continue

    for pattern in (
        r'["\']storeId["\']\s*:\s*["\']([^"\']+)["\']',
        r'["\']store["\']\s*:\s*\{\s*["\']id["\']\s*:\s*["\']([^"\']+)["\']',
        r'[?&]storeId=([^&"\'\s]+)',
    ):
        match = re.search(pattern, html, re.IGNORECASE)
        if match:
            return urllib.parse.unquote(match.group(1)).strip()
    return None


def _oreka_listing_url(source_url, store_id):
    parsed = urllib.parse.urlparse(source_url)
    base_url = f"{parsed.scheme or 'https'}://{parsed.netloc or 'www.oreka.vn'}"
    query = urllib.parse.urlencode({
        'storeId': store_id,
        'sort': 'createdAt',
        'order': 'desc',
    })
    return f"{base_url}/mua-ban?{query}"


def register_preprocessors():
    """Dang ky preprocessor chuyen trang store Oreka thanh danh sach san pham."""
    return json.dumps([{
        "id": "oreka-store-products",
        "name": "Oreka Store Products",
        "plugin_id": "",
        "input_type": "html",
        "platform": "oreka.vn",
        "config": {
            "input_type": "html",
            "item_selector": None,
            "url_patterns": [],
            "extract_rules": [],
            "csv_delimiter": None,
            "csv_has_header": None,
            "json_item_path": None,
            "client_type": None,
            "client_timeout_secs": None,
            "client_headless": None,
            "wait_for_selector": None,
            "wait_for_content": None,
            "wait_timeout_ms": None,
        },
    }])


def preprocess_data(data_json):
    """Lay store ID tu HTML nguon, rewrite thanh listing URL.

    Chi tra ve 1 item kieu 'listing_url' (URL da duoc rewrite thanh
    /mua-ban?storeId=...). Rust se fetch HTML cua URL nay o Stage B
    va dung [URL Patterns] cua fetch-data node de trich product URLs.
    """
    payload = json.loads(data_json) if isinstance(data_json, str) else data_json
    html = payload.get("raw_data", "")
    source_url = payload.get("source_url", "https://www.oreka.vn")
    store_id = _extract_store_id_from_html(html) or _extract_store_id_from_html(source_url)

    if not store_id:
        crawlflow.log("[OrekaShop][preprocess] Khong tim thay storeId trong HTML", "warn")
        return json.dumps([])

    base_listing_url = _oreka_listing_url(source_url, store_id)

    # Pagination: Oreka store listing supports ?page=N (default max 50).
    max_pages = int((config.get("max_pages") or 50) or 50)
    if max_pages < 1:
        max_pages = 1

    items = []
    for page_num in range(1, max_pages + 1):
        page_url = _add_page_to_url(base_listing_url, "page", page_num) if page_num > 1 else base_listing_url
        crawlflow.log(
            f"[OrekaShop][preprocess] Listing page {page_num}: {page_url} (storeId={store_id})",
            "info",
        )
        items.append({
            "source_url": page_url,
            "item_type": "listing_url",
            "item_hash": hashlib.sha256(page_url.encode("utf-8")).hexdigest(),
            "raw_content": None,
            "extracted_url": page_url,
        })

    crawlflow.log(
        f"[OrekaShop][preprocess] Da tao {len(items)} listing URL (page 1..{max_pages})",
        "info",
    )
    return json.dumps(items)


# ── Mac dinh cho oreka.vn ──────────────────────────────────────────────
DEFAULT_SELECTORS = {
    "product_list": ".product-list, .products-grid, [class*='product-grid'], [class*='list-product'], .shop-products",
    "product_item": "a[href*='/product/'], a[href*='/san-pham/'], .product-item a, [class*='product-card'] a, .item-product a",
    "product_link_attr": "href",
    "pagination": ".pagination, .page-list, [class*='pagination'], .pages",
    "next_page": "a.next, a[rel='next'], .pagination a:last-child, a:contains('Sau')",
    "page_param": "page",
}

DEFAULT_DETAIL = {
    "name": "h1.product-name, h1.product-title, [class*='product-name'], [class*='product-title'], .product_detail_name, h1",
    "price": "[class*='price'] [class*='current'], [class*='product-price'] [class*='special'], .product_price .price, [class*='product-price'] > span, .current-price, span.price",
    "old_price": "[class*='old-price'], [class*='price-old'], .product_price .old-price, del.price, .old-price",
    "image": "meta[property='og:image'], .product-gallery img, [class*='product-image'] img, .product_detail_img img, img.main-image",
    "sku": "[class*='sku'], .product-code, [class*='product-id'], .product_sku, span.sku",
    "description": "[class*='description'], .product-description, .product_detail_description, #product-description, .tab-content",
    "availability": "[class*='availability'], .product-status, .stock, span:contains('Con hang'), span:contains('Het hang')",
    "category": "[class*='breadcrumb'] a:last-child, .breadcrumbs a:last-child, [class*='breadcrumb'] li:last-child a, .breadcrumb li:last-child",
    "specs_table": "table. specifications, .product-attributes table, .product-specs table, [class*='specs'] table, .parameter table",
}


class HTMLContentParser(HTMLParser):
    """Parser HTML don gian de lay text va attribute."""

    def __init__(self):
        super().__init__()
        self.text_parts = []
        self._capture = False
        self._depth = 0
        self._target_tag = None
        self._target_attrs = None
        self.result_attrs = {}

    def extract_text(self, html):
        self.text_parts = []
        self.feed(html)
        return " ".join(self.text_parts).strip()

    def handle_data(self, data):
        if self._capture:
            stripped = data.strip()
            if stripped:
                self.text_parts.append(stripped)

    def handle_starttag(self, tag, attrs):
        if self._target_tag and tag == self._target_tag:
            if self._target_attrs:
                attr_dict = dict(attrs)
                for k, v in self._target_attrs.items():
                    if attr_dict.get(k) == v:
                        self._capture = True
                        self._depth += 1
                        break
            else:
                self._capture = True
                self._depth += 1
        elif self._capture:
            self._depth += 1

    def handle_endtag(self, tag):
        if self._capture:
            self._depth -= 1
            if self._depth <= 0:
                self._capture = False
                self._target_tag = None
                self._target_attrs = None


def _get_text(html, selector):
    """Don gian: lay text content tu HTML."""
    p = HTMLContentParser()
    return p.extract_text(html)


def _spec_value(specs, *keys):
    """Lay gia tri tu specs bang nhieu key co the co."""
    for key in keys:
        val = specs.get(key)
        if val:
            return str(val).strip()
    return ""


def _safe_float(val, default=0):
    try:
        cleaned = re.sub(r'[^\d.,]', '', str(val))
        cleaned = cleaned.replace('.', '').replace(',', '.')
        return float(cleaned) if cleaned else default
    except (ValueError, TypeError):
        return default


def _safe_int(val, default=0):
    try:
        return int(float(re.sub(r'[^\d]', '', str(val)))) if re.sub(r'[^\d]', '', str(val)) else default
    except (ValueError, TypeError):
        return default


def _extract_attr(html, attr="src"):
    """Lay attribute value tu the HTML dau tien."""
    m = re.search(rf'{attr}\s*=\s*["\']([^"\']+)["\']', html)
    return m.group(1) if m else ""


def _extract_meta_content(html):
    """Lay content tu the meta tag."""
    m = re.search(r'content\s*=\s*["\']([^"\']+)["\']', html)
    return m.group(1) if m else ""


def _extract_specs(html):
    """Lay bang thong so san pham."""
    specs = {}
    rows = re.findall(r'<tr[^>]*>(.*?)</tr>', html, re.DOTALL)
    for row in rows:
        cells = re.findall(r'<t[dh][^>]*>(.*?)</t[dh]>', row, re.DOTALL)
        if len(cells) >= 2:
            key = _get_text(cells[0], "").strip()
            val = _get_text(cells[1], "").strip()
            if key:
                specs[key] = val
    return specs


def _extract_listing_links(html, base_url):
    """Trich xuat tat ca link san pham tu trang danh sach."""
    links = set()
    for m in re.finditer(r'<a[^>]*href\s*=\s*["\'](/[^"\']*(?:product|san-pham)[^"\']*)["\']', html, re.IGNORECASE):
        href = m.group(1)
        full = href if href.startswith("http") else (base_url.rstrip("/") + "/" + href.lstrip("/"))
        links.add(full)
    for m in re.finditer(r'<a[^>]*href\s*=\s*["\'](https?://[^"\']*(?:product|san-pham)[^"\']*)["\']', html, re.IGNORECASE):
        links.add(m.group(1))
    return list(links)


def _extract_oreka_listing_links(html, base_url):
    """Lay URL chi tiet san pham Oreka.

    Uu tien 1: Lay tu JSON-LD @type=ItemList (chinh xac nhat, Oreka nhung day).
    Uu tien 2: Lay tu the <a href> co pattern /mua-ban-*/...-detail/<id>.
    """
    import json as _json
    links = set()
    origin = "/".join(base_url.rstrip("/").split("/")[:3])  # https://www.oreka.vn

    # --- Uu tien 1: JSON-LD ItemList ---
    for ld_text in re.findall(
        r'<script[^>]*type=["\']application/ld\+json["\'][^>]*>(.*?)</script>',
        html, re.DOTALL | re.IGNORECASE
    ):
        try:
            data = _json.loads(ld_text)
            # Ho tro ca object don va @graph array
            nodes = []
            if isinstance(data, list):
                nodes = data
            elif isinstance(data, dict):
                nodes = data.get("@graph", [data])
            for node in nodes:
                if not isinstance(node, dict):
                    continue
                if node.get("@type") == "ItemList":
                    for item in node.get("itemListElement", []):
                        url = item.get("url", "")
                        if url:
                            full = urllib.parse.urljoin(origin + "/", url.lstrip("/"))
                            links.add(full)
        except Exception:
            pass

    if links:
        return links

    # --- Uu tien 2: <a href> regex ---
    # Pattern: /mua-ban-<category>/<slug>-detail/<id>
    pattern = re.compile(
        r'href=["\']((https?://[^"\'/][^"\']*)?' +
        r'/mua-ban(?:-[^/"\' ]+)?/[^"\' ]*-detail/\d+(?:\?[^"\' ]*)?)["\']',
        re.IGNORECASE,
    )
    for match in pattern.finditer(unescape(html)):
        href = match.group(1)
        links.add(urllib.parse.urljoin(origin + "/", href.lstrip("/")))

    return links


def _has_next_page(html, current_page):
    """Kiem tra xem trang hien tai co link/nut sang trang tiep theo khong.

    Chien luoc (theo thu tu uu tien):
    1. rel="next"  — chuan HTML, chinh xac nhat.
    2. Ton tai href chua ?page=<N+1> hoac &page=<N+1>  — chinh xac voi Oreka.
    3. aria-label chua "next" / "sau" / "tiep".
    4. class chua "next" / "forward" / "after" tren the <a> hoac <button>.
    5. Text cua link la ky tu phan trang pho bien: ›, », Next, Sau, Tiep.
    """
    next_page_num = current_page + 1

    # 0. Tim tat ca so trang tu link ?page=N / &page=N trong HTML. Neu co
    #    trang lon hon current -> chac chan con trang sau (Oreka render day du
    #    pagination nen cach nay rat tin cay).
    page_nums = [
        int(m)
        for m in re.findall(r'[?&]page=(\d+)', html)
        if m.isdigit()
    ]
    if page_nums and max(page_nums) > current_page:
        return True

    # 1. rel="next"
    if re.search(r'rel=["\']next["\']', html, re.IGNORECASE):
        return True

    # 2. Href co page=<N+1> — Oreka render link nay trong SSR HTML
    if re.search(
        r'href=["\'][^"\']*[?&]page=' + str(next_page_num) + r'(?:[&"\'/]|$)',
        html, re.IGNORECASE
    ):
        return True

    # 2b. Oreka specific: nut next la <a> chua <img alt="arrow-right">
    #     href cua nut nay luon tro den trang tiep theo
    if re.search(
        r'<a\b[^>]*href=[^>]+>\s*(?:<[^>]+>\s*)*<img[^>]+alt=["\']arrow-right["\']',
        html, re.IGNORECASE | re.DOTALL
    ):
        return True

    # 3. aria-label next/sau/tiep
    if re.search(
        r'aria-label=["\'][^"\']*(next|sau|tiep theo|trang sau)[^"\']*(?:["\']|$)',
        html, re.IGNORECASE
    ):
        return True

    # 4. class next/forward/after tren <a> hoac <button>
    if re.search(
        r'<(?:a|button)[^>]*class=["\'][^"\']*(\bnext\b|\bforward\b|\bafter\b)[^"\']*["\']',
        html, re.IGNORECASE
    ):
        return True

    # 5. Text pho bien cua nut next (›, », Next, Sau, Tiep)
    next_text_pattern = re.compile(
        r'<(?:a|button|li)[^>]*>[^<]{0,40}(?:›|»|&rsaquo;|&raquo;|\bNext\b|\bSau\b|\bTiep\b)[^<]{0,10}</(?:a|button|li)>',
        re.IGNORECASE
    )
    if next_text_pattern.search(html):
        return True

    return False


def _parse_product_from_html(html, url):
    """Phan tich HTML trang chi tiet de lay thong tin san pham."""
    parser = HTMLContentParser()
    text = parser.extract_text(html)

    name = ""
    price = 0
    old_price = 0
    image = ""
    sku = ""
    description = ""
    availability = ""
    category = ""
    specs = {}

    # Ten san pham - uu tien the h1
    h1_match = re.search(r'<h1[^>]*>(.*?)</h1>', html, re.DOTALL)
    if h1_match:
        name = _get_text(h1_match.group(1), "").strip()

    # Gia
    price_patterns = [
        r'<span[^>]*class\s*=\s*["\'][^"\']*price[^"\']*["\'][^>]*>([^<]+)</span>',
        r'<div[^>]*class\s*=\s*["\'][^"\']*price[^"\']*["\'][^>]*>([^<]+)</div>',
        r'<ins[^>]*>([^<]+)</ins>',
        r'<span[^>]*class\s*=\s*["\'][^"\']*current[^"\']*["\'][^>]*>([^<]+)</span>',
    ]
    for pat in price_patterns:
        m = re.search(pat, html)
        if m:
            price = _safe_float(m.group(1))
            if price > 0:
                break

    if price == 0:
        gia_matches = re.findall(r'(?:(\d[\d.,]*)\s*(?:₫|vnđ|vnd|đ))', html, re.IGNORECASE)
        if gia_matches:
            price = _safe_float(gia_matches[0])

    # Gia cu
    old_patterns = [
        r'<span[^>]*class\s*=\s*["\'][^"\']*old[^"\']*["\'][^>]*>([^<]+)</span>',
        r'<del[^>]*>([^<]+)</del>',
        r'<strike[^>]*>([^<]+)</strike>',
    ]
    for pat in old_patterns:
        m = re.search(pat, html)
        if m:
            old_price = _safe_float(m.group(1))
            if old_price > 0:
                break
    if old_price == 0 and price > 0:
        old_price = price

    # Hinh anh - uu tien og:image
    og_image = re.search(r'<meta[^>]*property\s*=\s*["\']og:image["\'][^>]*content\s*=\s*["\']([^"\']+)["\']', html)
    if og_image:
        image = og_image.group(1)
    else:
        img_match = re.search(r'<img[^>]*class\s*=\s*["\'][^"\']*(?:product|main|gallery)[^"\']*["\'][^>]*src\s*=\s*["\']([^"\']+)["\']', html)
        if img_match:
            image = img_match.group(1)
        else:
            img_match = re.search(r'<img[^>]*src\s*=\s*["\']([^"\']+(?:product|san-pham)[^"\']+)["\']', html)
            if img_match:
                image = img_match.group(1)

    # SKU
    sku_matches = re.findall(r'(?:SKU|Mã sản phẩm|Product Code|Mã SP)\s*[:;]\s*([^\s<]+)', html, re.IGNORECASE)
    if sku_matches:
        sku = sku_matches[0].strip()

    # Mo ta - lay tu tab content hoac div mo ta
    desc_match = re.search(r'<div[^>]*(?:description|product-desc|tab-content|product_description)[^>]*>(.*?)</div>\s*</div>', html, re.DOTALL)
    if desc_match:
        description = _get_text(desc_match.group(1), "").strip()
    if not description:
        desc_match = re.search(r'<div[^>]*(?:description|product-desc)[^>]*>(.*?)</div>', html, re.DOTALL)
        if desc_match:
            description = _get_text(desc_match.group(1), "").strip()
    if not description and len(description) > 50000:
        description = description[:50000]

    # Tinh trang
    avail_match = re.search(r'(?:Còn hàng|Tạm hết|Hết hàng|Liên hệ|In stock|Out of stock)', html, re.IGNORECASE)
    if avail_match:
        availability = avail_match.group(0)

    # Ton kho
    stock = ""
    stock_match = re.search(r'(?:Số lượng|Tồn kho|Kho hàng|Còn lại)\s*[:;]?\s*(\d+)', html, re.IGNORECASE)
    if stock_match:
        stock = stock_match.group(1)
    if not stock:
        stock_match = re.search(r'(?:Còn|còn)\s*(\d+)\s*(?:sản phẩm|sp|hàng)', html, re.IGNORECASE)
        if stock_match:
            stock = stock_match.group(1)

    # Danh muc
    breadcrumb = re.search(r'<ul[^>]*class\s*=\s*["\'][^"\']*breadcrumb[^"\']*["\']>(.*?)</ul>', html, re.DOTALL)
    if breadcrumb:
        cats = re.findall(r'<a[^>]*>(.*?)</a>', breadcrumb.group(1))
        if len(cats) >= 2:
            category = _get_text(cats[-1], "").strip()

    # Bang thong so
    spec_tables = re.findall(r'<table[^>]*>(.*?)</table>', html, re.DOTALL)
    for table_html in spec_tables:
        rows = re.findall(r'<tr[^>]*>(.*?)</tr>', table_html, re.DOTALL)
        for row in rows[:30]:
            cells = re.findall(r'<t[dh][^>]*>(.*?)</t[dh]>', row, re.DOTALL)
            if len(cells) >= 2:
                k = _get_text(cells[0], "").strip()
                v = _get_text(cells[1], "").strip()
                if k:
                    specs[k] = v

    return {
        "url": url,
        "product_name": name or os.path.basename(url),
        "price": price,
        "old_price": old_price,
        "image_url": image,
        "sku": sku,
        "description": description[:500] if description else "",
        "specs": specs,
        "category": category,
        "stock": stock,
        "availability": availability or "Còn hàng",
        "crawled_at": datetime.now().strftime("%Y-%m-%dT%H:%M:%S"),
        "raw_html": html,
    }


def on_load(config):
    crawlflow.log("[OrekaShop] Plugin loaded", "info")
    try:
        import openpyxl
        crawlflow.log("[OrekaShop] openpyxl available - will use Excel output", "info")
    except ImportError:
        crawlflow.log("[OrekaShop] openpyxl not installed - will use CSV output", "warn")


def _crawl_all_products(shop_url, max_pages, delay_ms, client_type=None, headless=None):
    """Tu crawl toan bo san pham cua shop (phan trang + trich product URL + parse).

    Tra ve danh sach cac dict san pham (da duoc parse day du). Logic phan trang
    hoan toan nam trong Python plugin (linh dong, khong phu thuoc Rust Stage B).
    client_type: 'reqwest' (mac dinh) hoac 'chrome' — su dung 2 kenh fetch.
    """
    def _fetch(url):
        # Goi Python SDK fetch_url voi kem client_type de chon kenh reqwest/chrome.
        return crawlflow.fetch_url(url, None, client_type, headless)

    # 1. Lay storeId tu store page.
    try:
        raw = _fetch(shop_url)
        store_result = json.loads(raw) if isinstance(raw, str) else raw
    except Exception as e:
        crawlflow.log(f"[OrekaShop] Loi fetch store page: {e}", "error")
        return []

    store_html = store_result.get("body", "") if isinstance(store_result, dict) else ""
    if not store_html:
        crawlflow.log("[OrekaShop] Store page rong", "error")
        return []

    store_id = _extract_store_id_from_html(store_html) or _extract_store_id_from_html(shop_url)
    if not store_id:
        crawlflow.log("[OrekaShop] Khong tim thay storeId", "error")
        return []

    base_listing_url = _oreka_listing_url(shop_url, store_id)
    crawlflow.log(
        f"[OrekaShop] storeId={store_id} | listing base={base_listing_url}", "info"
    )

    products = []
    seen_urls = set()
    page_num = 1

    while True:
        page_url = _add_page_to_url(base_listing_url, "page", page_num) if page_num > 1 else base_listing_url
        crawlflow.log(f"[OrekaShop] Listing page {page_num}: {page_url}", "info")

        try:
            raw = _fetch(page_url)
            listing_result = json.loads(raw) if isinstance(raw, str) else raw
        except Exception as e:
            crawlflow.log(f"[OrekaShop] Loi fetch listing page {page_num}: {e}", "error")
            break

        listing_html = listing_result.get("body", "") if isinstance(listing_result, dict) else ""
        if not listing_html:
            crawlflow.log(f"[OrekaShop] Listing page {page_num} rong", "warn")
            break

        # Trich product URLs tu listing HTML.
        product_urls = _extract_oreka_listing_links(listing_html, page_url)
        crawlflow.log(
            f"[OrekaShop] Tim thay {len(product_urls)} product URL o trang {page_num}",
            "info",
        )

        for p_url in product_urls:
            if p_url in seen_urls:
                continue
            seen_urls.add(p_url)
            try:
                praw = _fetch(p_url)
                pres = json.loads(praw) if isinstance(praw, str) else praw
                phtml = pres.get("body", "") if isinstance(pres, dict) else ""
                if phtml:
                    prod = _parse_product_from_html(phtml, p_url)
                    products.append(prod)
            except Exception as e:
                crawlflow.log(f"[OrekaShop] Loi fetch product {p_url}: {e}", "warn")

        # Tiep tuc phan trang: reqwest da lay xong HTML listing, giao cho
        # Python tim nut "next page" trong chinh HTML do (khong fetch them).
        if page_num >= max_pages:
            crawlflow.log(f"[OrekaShop] Da du max_pages={max_pages}", "info")
            break

        has_next = _has_next_page(listing_html, page_num)
        if not has_next:
            crawlflow.log(f"[OrekaShop] Het phan trang tai trang {page_num}", "info")
            break
        if page_num >= max_pages:
            crawlflow.log(f"[OrekaShop] Da du max_pages={max_pages}", "info")
            break
        page_num += 1
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)

    return products


def fetch_data(config_json):
    """Crawl toan bo san pham cua shop va tra ve truc tiep N item 'product'.

    Plugin tu quyet dinh toan bo logic: lay storeId, phan trang listing,
    trich product URL, parse chi tiet. Rust chi can gom cac item nay vao
    raw_items (item_type='product') de worker + exporter xu ly tiep.

    Config:
        shop_url (str, bat buoc): URL cua store (vd: https://www.oreka.vn/store/motsach)
        max_pages (int, mac dinh: 50): gioi han so trang (de an toan)
        delay_ms (int, mac dinh: 1000): nghi giua cac request
        project_id (str): ID project (tu dong inject)
    """
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    shop_url = (config.get("shop_url") or "").strip()
    if not shop_url:
        # Fallback: dung chinh sourceValue neu shop_url chua duoc set.
        shop_url = (config.get("source_value") or "").strip()
    if not shop_url:
        crawlflow.log("[OrekaShop] Thieu shop_url trong config", "error")
        return json.dumps([])

    max_pages = int((config.get("max_pages") or 50) or 50)
    if max_pages < 1:
        max_pages = 1
    delay_ms = int((config.get("delay_ms") or 1000) or 1000)
    if delay_ms < 0:
        delay_ms = 0

    # Chon kenh fetch: reqwest (mac dinh) hoac chrome. Doc tu config
    # (clientType) hoac urlSettings.httpClient.clientType cua node.
    client_type = (config.get("clientType") or config.get("client_type")
                   or (config.get("urlSettings") or {}).get("httpClient", {}).get("clientType"))
    if client_type not in ("reqwest", "chrome", "cdp"):
        client_type = "reqwest"
    headless = bool((config.get("headless")
                     or (config.get("urlSettings") or {}).get("httpClient", {}).get("headless", True)))

    crawlflow.log(
        f"[OrekaShop][fetch_data] Bat dau crawl shop={shop_url} (max_pages={max_pages}, client={client_type})",
        "info",
    )

    products = _crawl_all_products(shop_url, max_pages, delay_ms, client_type, headless)

    # Dung dinh dang item ma Rust/worker hieu: item_type='url'.
    items = []
    for p in products:
        item_url = p.get("url", "")
        raw_html = p.pop("raw_html", "")
        items.append({
            "source_url": item_url,
            "item_type": "url",
            "item_hash": hashlib.sha256(item_url.encode("utf-8")).hexdigest() if item_url else hashlib.sha256(json.dumps(p, ensure_ascii=False).encode("utf-8")).hexdigest(),
            "raw_content": raw_html,
            "extracted_url": item_url,
        })

    crawlflow.log(
        f"[OrekaShop][fetch_data] Hoan tat: {len(items)} san pham",
        "info",
    )
    return json.dumps(items)


def process_data(data_json, config_json):
    """Chuan hoa du lieu san pham."""
    data = json.loads(data_json) if isinstance(data_json, str) else data_json
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    normalized = []
    for item in data:
        norm = {
            "url": item.get("url", ""),
            "name": item.get("name", "").strip(),
            "price": _safe_float(item.get("price", 0)),
            "old_price": _safe_float(item.get("old_price", 0)),
            "image": item.get("image", ""),
            "sku": item.get("sku", "").strip(),
            "description": (item.get("description", "") or "").strip(),
            "category": item.get("category", "").strip(),
            "availability": item.get("availability", "Còn hàng"),
            "crawled_at": item.get("crawled_at", datetime.now().strftime("%Y-%m-%dT%H:%M:%S")),
            "specs": item.get("specs", {}),
        }
        normalized.append(norm)

    return json.dumps(normalized)


def export_data(data_json, config_json):
    """Xuat du lieu ra Excel/CSV voi co che append + check trung.

    Dinh dang cot: STT, Gia bia, Gia ban, Ton kho, Don vi,
                   Khoi luong (g), Kich thuoc, Tinh trang, Nam XB,
                   Thuong hieu, Ten sach, Danh muc

    Ghi log vao file dedup de tranh trung lap.
    Luon append vao file Excel (doc file cu, them rows moi, ghi de).
    """
    data = json.loads(data_json) if isinstance(data_json, str) else data_json
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    project_id = config.get("project_id", "default")
    output_dir = config.get("output_dir", os.getcwd())
    os.makedirs(output_dir, exist_ok=True)

    # Lay ten shop tu URL
    shop_url = config.get("shop_url", "")
    shop_name = "oreka_shop"
    if shop_url:
        parts = shop_url.rstrip("/").split("/")
        if parts:
            shop_name = parts[-1].replace("?", "_").replace("&", "_")

    started_at = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
    total_items = len(data)

    crawlflow.log(f"[OrekaShop] Bat dau export {total_items} san pham", "info")

    # ── Dedup: doc file dedup ────────────────────────────────────────
    dedup_path = os.path.join(output_dir, f".{shop_name}_dedup.json")
    seen_ids = set()
    if os.path.exists(dedup_path):
        try:
            content = crawlflow.read_file(dedup_path)
            seen_ids = set(json.loads(content))
            crawlflow.log(f"[OrekaShop] Da doc {len(seen_ids)} ID da xu ly tu file dedup", "info")
        except Exception as e:
            crawlflow.log(f"[OrekaShop] Loi doc dedup file: {e}", "warn")

    # Loc san pham moi
    new_products = []
    for item in data:
        dedup_key = item.get("url", "") or item.get("sku", "")
        if dedup_key and dedup_key in seen_ids:
            continue
        new_products.append(item)

    crawlflow.log(f"[OrekaShop] Sau dedup: {len(new_products)} san pham moi (da bo qua {len(data) - len(new_products)} san pham trung)", "info")

    if not new_products:
        crawlflow.log("[OrekaShop] Khong co san pham moi de export", "info")
        _update_progress(project_id, {
            "items_total": total_items,
            "items_processed": total_items,
            "items_success": 0,
            "items_failed": 0,
            "progress_pct": 100.0,
            "avg_time_ms": 0,
            "total_time_ms": 0,
            "started_at": started_at,
            "message": "Khong co san pham moi",
        })
        return json.dumps({"file": "", "count": 0, "new": 0, "skipped": len(data)})

    # ── Xuat Excel ──────────────────────────────────────────────────
    excel_path = os.path.join(output_dir, f"{shop_name}_products.xlsx")
    csv_path = os.path.join(output_dir, f"{shop_name}_products.csv")

    try:
        from openpyxl import Workbook, load_workbook
        has_openpyxl = True
    except ImportError:
        has_openpyxl = False

    if has_openpyxl:
        count = _export_xlsx(new_products, excel_path, seen_ids)
        crawlflow.log(f"[OrekaShop] Da ghi {count} san pham vao {excel_path}", "info")
    else:
        count = _export_csv(new_products, csv_path)
        crawlflow.log(f"[OrekaShop] Da ghi {count} san pham vao {csv_path} (CSV)", "info")

    # Luu dedup
    for item in new_products:
        dedup_key = item.get("url", "") or item.get("sku", "")
        if dedup_key:
            seen_ids.add(dedup_key)

    try:
        crawlflow.save_file(dedup_path, json.dumps(list(seen_ids), ensure_ascii=False))
    except Exception as e:
        crawlflow.log(f"[OrekaShop] Loi ghi dedup file: {e}", "warn")

    elapsed = (datetime.now() - datetime.strptime(started_at, "%Y-%m-%dT%H:%M:%S")).total_seconds() * 1000
    _update_progress(project_id, {
        "items_total": total_items,
        "items_processed": total_items,
        "items_success": count,
        "items_failed": total_items - count,
        "progress_pct": 100.0,
        "avg_time_ms": elapsed / max(total_items, 1),
        "total_time_ms": elapsed,
        "started_at": started_at,
        "message": f"Export hoan thanh: {count} san pham moi",
    })

    output_file = excel_path if has_openpyxl else csv_path
    result = {
        "file": output_file,
        "count": count,
        "new": len(new_products),
        "skipped": len(data) - len(new_products),
    }
    return json.dumps(result)


def _export_xlsx(products, filepath, seen_ids):
    """Ghi san pham vao file Excel (append neu file da ton tai)."""
    try:
        wb = load_workbook(filepath)
        ws = wb.active
        crawlflow.log(f"[OrekaShop] Mo file Excel co san: {filepath}", "info")
    except Exception:
        wb = Workbook()
        ws = wb.active
        ws.title = "San pham"
        headers = [
            "STT", "Giá bìa", "Giá bán", "Tồn kho", "Đơn vị",
            "Khối lượng (g)", "Kích thước", "Tình trạng", "Năm XB",
            "Thương hiệu", "Tên sách", "Danh mục"
        ]
        ws.append(headers)
        crawlflow.log(f"[OrekaShop] Tao file Excel moi: {filepath}", "info")

    count = 0

    for item in products:
        dedup_key = item.get("url", "") or item.get("sku", "")
        if dedup_key and dedup_key in seen_ids:
            continue

        specs = item.get("specs", {})

        row = [
            ws.max_row,
            item.get("old_price", 0),
            item.get("price", 0),
            item.get("stock", ""),
            _spec_value(specs, "Đơn vị", "Đơn vị tính"),
            _spec_value(specs, "Trọng lượng", "Khối lượng", "Khối lượng (g)"),
            _spec_value(specs, "Kích thước", "Kích thước sản phẩm"),
            item.get("availability", ""),
            _spec_value(specs, "Năm xuất bản", "Năm XB"),
            _spec_value(specs, "Thương hiệu", "Nhà xuất bản", "NXB", "Tác giả"),
            item.get("name", ""),
            item.get("category", ""),
        ]
        ws.append(row)
        count += 1

    wb.save(filepath)
    crawlflow.log(f"[OrekaShop] Da them {count} dong vao Excel", "info")
    return count


def _export_csv(products, filepath):
    """Fallback: ghi CSV."""
    import csv
    import io

    mode = "a" if os.path.exists(filepath) else "w"
    has_header = mode == "w"

    # Count existing data rows to continue STT
    existing_count = 0
    if mode == "a" and os.path.exists(filepath):
        with open(filepath, "r", encoding="utf-8-sig") as f:
            existing_count = sum(1 for _ in f) - 1  # subtract header row

    with open(filepath, mode, newline="", encoding="utf-8-sig") as f:
        writer = csv.writer(f)
        if has_header:
            writer.writerow([
                "STT", "Giá bìa", "Giá bán", "Tồn kho", "Đơn vị",
                "Khối lượng (g)", "Kích thước", "Tình trạng", "Năm XB",
                "Thương hiệu", "Tên sách", "Danh mục"
            ])

        count = existing_count
        for item in products:
            specs = item.get("specs", {})

            writer.writerow([
                count + 1,
                item.get("old_price", 0),
                item.get("price", 0),
                item.get("stock", ""),
                _spec_value(specs, "Đơn vị", "Đơn vị tính"),
                _spec_value(specs, "Trọng lượng", "Khối lượng", "Khối lượng (g)"),
                _spec_value(specs, "Kích thước", "Kích thước sản phẩm"),
                item.get("availability", ""),
                _spec_value(specs, "Năm xuất bản", "Năm XB"),
                _spec_value(specs, "Thương hiệu", "Nhà xuất bản", "NXB", "Tác giả"),
                item.get("name", ""),
                item.get("category", ""),
            ])
            count += 1

    crawlflow.log(f"[OrekaShop] Da them {count} dong vao CSV", "info")
    return count


def _update_progress(project_id, data):
    """Cap nhat progress vao Rust backend."""
    try:
        def _to_u64(v):
            try:
                return int(round(float(v))) if v is not None else 0
            except (TypeError, ValueError):
                return 0

        info = {
            "items_total": _to_u64(data.get("items_total", 0)),
            "items_processed": _to_u64(data.get("items_processed", 0)),
            "items_success": _to_u64(data.get("items_success", 0)),
            "items_failed": _to_u64(data.get("items_failed", 0)),
            "progress_pct": float(data.get("progress_pct", 0.0) or 0.0),
            "avg_time_ms": float(data.get("avg_time_ms", 0.0) or 0.0),
            "total_time_ms": _to_u64(data.get("total_time_ms", 0)),
            "started_at": data.get("started_at", ""),
            "message": data.get("message", ""),
        }
        crawlflow.update_progress(project_id, json.dumps(info))
    except Exception as e:
        crawlflow.log(f"[OrekaShop] Loi update progress: {e}", "error")




def on_unload():
    crawlflow.log("[OrekaShop] Plugin unloaded", "info")
