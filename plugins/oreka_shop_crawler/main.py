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
    """Lay store ID tu HTML nguon va tra ve tat ca URL san pham cua store."""
    payload = json.loads(data_json) if isinstance(data_json, str) else data_json
    html = payload.get("raw_data", "")
    source_url = payload.get("source_url", "https://www.oreka.vn")
    store_id = _extract_store_id_from_html(html) or _extract_store_id_from_html(source_url)

    if not store_id:
        crawlflow.log("[OrekaShop] Khong tim thay storeId trong HTML data source", "warn")
        return json.dumps([])

    listing_url = _oreka_listing_url(source_url, store_id)
    listing_origin = urllib.parse.urlunparse(
        urllib.parse.urlparse(listing_url)._replace(path="", params="", query="", fragment="")
    )
    product_urls = set()

    for page_number in range(1, 101):
        page_url = listing_url if page_number == 1 else _add_page_to_url(listing_url, "page", page_number)
        crawlflow.log(f"[OrekaShop] Fetch danh sach trang {page_number}: {page_url}", "info")
        try:
            response = crawlflow.fetch_url(page_url, None)
            result = json.loads(response) if isinstance(response, str) else response
        except Exception as error:
            crawlflow.log(f"[OrekaShop] Loi fetch trang {page_number}: {error}", "error")
            break

        if result.get("status") != 200:
            crawlflow.log(
                f"[OrekaShop] Trang {page_number} tra ve status {result.get('status')}",
                "warn",
            )
            break

        page_product_urls = _extract_oreka_listing_links(result.get("body", ""), listing_origin)
        new_urls = page_product_urls - product_urls
        if not new_urls:
            break
        product_urls.update(new_urls)

    product_urls = sorted(product_urls)
    items = [{
        "source_url": product_url,
        "item_type": "url",
        "item_hash": hashlib.sha256(product_url.encode("utf-8")).hexdigest(),
        "raw_content": None,
        "extracted_url": product_url,
    } for product_url in product_urls]
    crawlflow.log(
        f"[OrekaShop] Tim thay {len(items)} san pham cho store {store_id}",
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
        r'aria-label=["\'][^"\']*(next|sau|tiep theo|trang sau)[^"\'']*["\']',
        html, re.IGNORECASE
    ):
        return True

    # 4. class next/forward/after tren <a> hoac <button>
    if re.search(
        r'<(?:a|button)[^>]*class=["\'][^"\']*(\bnext\b|\bforward\b|\bafter\b)[^"\'']*["\']',
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
        "name": name or os.path.basename(url),
        "price": price,
        "old_price": old_price,
        "image": image,
        "sku": sku,
        "description": description[:500] if description else "",
        "specs": specs,
        "category": category,
        "stock": stock,
        "availability": availability or "Còn hàng",
        "crawled_at": datetime.now().strftime("%Y-%m-%dT%H:%M:%S"),
    }


def on_load(config):
    crawlflow.log("[OrekaShop] Plugin loaded", "info")
    try:
        import openpyxl
        crawlflow.log("[OrekaShop] openpyxl available - will use Excel output", "info")
    except ImportError:
        crawlflow.log("[OrekaShop] openpyxl not installed - will use CSV output", "warn")


def fetch_data(config_json):
    """Crawl toan bo san pham tu shop oreka.vn.

    Config:
        shop_url (str, bat buoc): URL cua shop
        max_pages (int, mac dinh 50): So trang toi da
        delay_ms (int, mac dinh 1500): Delay giua cac request (ms)
        project_id (str): ID project (tu dong inject)
        selectors (dict, tuy chon): Ghi de selector
    """
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    shop_url = config.get("shop_url", "").strip()
    if not shop_url:
        crawlflow.log("[OrekaShop] Thieu shop_url trong config", "error")
        return json.dumps([])

    max_pages = int(config.get("max_pages", 50))
    delay_ms = int(config.get("delay_ms", 1500))
    project_id = config.get("project_id", "default")
    custom_selectors = config.get("selectors", {})

    selectors = dict(DEFAULT_SELECTORS)
    selectors.update(custom_selectors)

    # Lay domain goc
    base_url = re.match(r'(https?://[^/]+)', shop_url)
    base = base_url.group(1) if base_url else shop_url

    # Check and rewrite store URL to MUABAN search page if store/ URL is passed
    if "/store/" in shop_url:
        crawlflow.log(f"[OrekaShop] Kiem tra URL tin cua store de lay storeId: {shop_url}", "info")
        try:
            raw = crawlflow.fetch_url(shop_url, None)
            result = json.loads(raw) if isinstance(raw, str) else raw
            if result.get("status") == 200:
                html_body = result.get("body", "")
                crawlflow.log(f"[OrekaShop] HTML body length: {len(html_body)} characters", "info")

                # Try robust extractor first (Next.js __NEXT_DATA__ + Apollo cache)
                store_id = _extract_store_id_from_html(html_body)

                # Regex fallbacks if extractor failed
                if not store_id:
                    m = re.search(r'"storeId"\s*:\s*["\']([^"\']+)["\']', html_body)
                    if m:
                        store_id = m.group(1).strip()
                if not store_id:
                    m = re.search(r'"store"\s*:\s*\{\s*"id"\s*:\s*["\']([^"\']+)["\']', html_body)
                    if m:
                        store_id = m.group(1).strip()

                if store_id:
                    shop_url = f"{base.rstrip('/')}/mua-ban?storeId={store_id}&sort=createdAt&order=desc"
                    crawlflow.log(f"[OrekaShop] Chuyen doi URL cua store thanh: {shop_url}", "info")
                else:
                    store_slug = _extract_store_slug_from_url(shop_url)
                    if store_slug:
                        crawlflow.log(f"[OrekaShop] Khong tim thay storeId. Dung store slug: {store_slug}", "warn")
                    else:
                        crawlflow.log("[OrekaShop] Khong tim thay storeId. Tiep tuc voi URL goc.", "warn")
            else:
                crawlflow.log(f"[OrekaShop] Web request to store page returned status {result.get('status')}", "warn")
        except Exception as e:
            crawlflow.log(f"[OrekaShop] Loi khi lay storeId tu store page: {e}", "error")

    # Khoi tao progress
    started_at = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")

    all_products = []
    product_urls = set()
    total_pages_estimated = max_pages

    crawlflow.log(f"[OrekaShop] Bat dau crawl shop: {shop_url}", "info")

    for page in range(1, max_pages + 1):
        page_url = _add_page_to_url(shop_url, selectors['page_param'], page) if page > 1 else shop_url

        crawlflow.log(f"[OrekaShop] Dang crawl trang {page}/{total_pages_estimated}: {page_url}", "info")

        try:
            raw = crawlflow.fetch_url(page_url, None)
            result = json.loads(raw) if isinstance(raw, str) else raw
        except Exception as e:
            crawlflow.log(f"[OrekaShop] Loi fetch trang {page}: {e}", "error")
            continue

        if result.get("status") != 200:
            crawlflow.log(f"[OrekaShop] Trang {page} tra ve status {result.get('status')}", "warn")
            if page == 1:
                crawlflow.log("[OrekaShop] Khong the truy cap shop, dung lai", "error")
                break
            continue

        html = result.get("body", "")

        # Kiem tra con trang tiep theo khong
        has_next = _has_next_page(html, page)

        # Trich xuat link san pham Oreka (/mua-ban-*/.../--detail/<id>)
        links = _extract_oreka_listing_links(html, base)
        if not links:
            # Fallback sang generic extractor cho cac site khac
            links = set(_extract_listing_links(html, base))
        before = len(product_urls)
        product_urls.update(links)
        new_links = len(product_urls) - before

        # Cap nhat progress
        elapsed = (datetime.now() - datetime.strptime(started_at, "%Y-%m-%dT%H:%M:%S")).total_seconds() * 1000
        _update_progress(project_id, {
            "items_total": len(product_urls) + len(all_products),
            "items_processed": len(product_urls),
            "items_success": len(product_urls),
            "items_failed": 0,
            "progress_pct": min(95.0, (page / total_pages_estimated) * 50.0),
            "avg_time_ms": elapsed / max(page, 1),
            "total_time_ms": elapsed,
            "started_at": started_at,
            "message": f"Dang tim san pham... trang {page}/{total_pages_estimated}",
        })

        crawlflow.log(f"[OrekaShop] Trang {page}: tim thay {new_links} san pham moi (tong: {len(product_urls)}), has_next={has_next}", "info")

        # Dung neu khong con next page hoac khong co them link moi
        if not has_next:
            crawlflow.log(f"[OrekaShop] Khong tim thay nut trang tiep theo o trang {page}, dung phan trang", "info")
            break
        if new_links == 0 and page > 1:
            crawlflow.log(f"[OrekaShop] Khong tim thay san pham moi o trang {page} du co next, dung phan trang", "info")
            break

        time.sleep(delay_ms / 1000.0)

    # Crawl chi tiet tung san pham
    all_urls = list(product_urls)
    total = len(all_urls)
    crawlflow.log(f"[OrekaShop] Tim thay {total} san pham, bat dau lay chi tiet...", "info")

    for idx, url in enumerate(all_urls):
        try:
            raw = crawlflow.fetch_url(url, None)
            result = json.loads(raw) if isinstance(raw, str) else raw
        except Exception as e:
            crawlflow.log(f"[OrekaShop] Loi fetch chi tiet {url}: {e}", "error")
            # Van them san pham co ban
            all_products.append(_parse_product_from_html("", url))
            _update_progress(project_id, {
                "items_total": total,
                "items_processed": idx + 1,
                "items_success": len(all_products),
                "items_failed": (idx + 1) - len(all_products),
                "progress_pct": min(95.0, 50.0 + ((idx + 1) / total) * 45.0),
                "avg_time_ms": 0,
                "total_time_ms": (datetime.now() - datetime.strptime(started_at, "%Y-%m-%dT%H:%M:%S")).total_seconds() * 1000,
                "started_at": started_at,
                "message": f"Loi: {e}",
            })
            continue

        html = result.get("body", "")
        product = _parse_product_from_html(html, url)
        all_products.append(product)

        if (idx + 1) % 5 == 0 or idx == total - 1:
            elapsed = (datetime.now() - datetime.strptime(started_at, "%Y-%m-%dT%H:%M:%S")).total_seconds() * 1000
            _update_progress(project_id, {
                "items_total": total,
                "items_processed": idx + 1,
                "items_success": len(all_products),
                "items_failed": (idx + 1) - len(all_products),
                "progress_pct": min(95.0, 50.0 + ((idx + 1) / total) * 45.0),
                "avg_time_ms": elapsed / (idx + 1),
                "total_time_ms": elapsed,
                "started_at": started_at,
                "message": f"Dang lay chi tiet... {idx + 1}/{total}",
            })
            crawlflow.log(f"[OrekaShop] Lay chi tiet: {idx + 1}/{total} - {product.get('name', 'N/A')}", "info")

        time.sleep(delay_ms / 1000.0)

    elapsed = (datetime.now() - datetime.strptime(started_at, "%Y-%m-%dT%H:%M:%S")).total_seconds() * 1000
    _update_progress(project_id, {
        "items_total": len(all_products),
        "items_processed": len(all_products),
        "items_success": len(all_products),
        "items_failed": 0,
        "progress_pct": 95.0,
        "avg_time_ms": elapsed / max(len(all_products), 1),
        "total_time_ms": elapsed,
        "started_at": started_at,
        "message": f"Hoan thanh crawl: {len(all_products)} san pham",
    })

    crawlflow.log(f"[OrekaShop] Crawl hoan thanh: {len(all_products)} san pham", "info")
    return json.dumps(all_products)


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
        info = {
            "items_total": data.get("items_total", 0),
            "items_processed": data.get("items_processed", 0),
            "items_success": data.get("items_success", 0),
            "items_failed": data.get("items_failed", 0),
            "progress_pct": data.get("progress_pct", 0.0),
            "avg_time_ms": data.get("avg_time_ms", 0.0),
            "total_time_ms": data.get("total_time_ms", 0),
            "started_at": data.get("started_at", ""),
            "message": data.get("message", ""),
        }
        crawlflow.update_progress(project_id, json.dumps(info))
    except Exception as e:
        crawlflow.log(f"[OrekaShop] Loi update progress: {e}", "error")




def on_unload():
    crawlflow.log("[OrekaShop] Plugin unloaded", "info")
