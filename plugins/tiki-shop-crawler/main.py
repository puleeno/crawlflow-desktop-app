"""
Tiki Shop Crawler - CrawlFlow Python Plugin

Crawl toan bo san pham tu mot shop/category tren tiki.vn,
parse du lieu tu __NEXT_DATA__ (SSR) va xuat ra file Excel voi co che
append + check trung. Toan bo request dung Google Chrome (client_type="chrome")
de dam bao giong voi nguoi dung thuc te mo trinh duyet.

Flow:
  1. fetch_data()  - Crawl tat ca san pham tu shop (phan trang cursor)
  2. process_data() - Chuan hoa du lieu
  3. export_data()  - Excel + dedup + cap nhat progress

Config:
  shop_url: str (bat buoc) - URL cua shop/category tren tiki.vn, VD:
    https://tiki.vn/cua-hang/tiki-trading?t=product&cid=120473&cursor=0&category_id=316&parent_id=8322
  max_pages: int (mặc định: 0 = không giới hạn)
  delay_ms: int (mặc định: 1500)
  client_type: str (mặc định: "chrome") - kenh fetch (reqwest/chrome/cdp)
  output_dir: str (mặc định: thu muc Downloads cua user)
  project_id: str (tu dong truyen tu service)

KET QUA TRA VE (fetch_data tra ve JSON array, moi phan tu la 1 san pham):
  {
    "url": "https://tiki.vn/...-p123456.html",
    "name": "Ten san pham",
    "price": 299000,
    "old_price": 399000,
    "discount": 100000,
    "discount_rate": 25,
    "image": "https://salt.tikicdn.com/...",
    "sku": "123456",
    "description": "Mo ta san pham...",
    "specs": { "Thuong hieu": "...", "Chat lieu": "..." },
    "category": "Dien thoai > Smartphone",
    "availability": "Con hang",
    "stock": "10",
    "crawled_at": "2026-07-02T12:00:00"
  }

XUAT EXCEL/CSV:
  Cot: STT, Gia bia, Gia ban, Giam gia, Phan tram, Ton kho,
       Tinh trang, Thuong hieu, Ten san pham, Danh muc,
       Nguoi ban, So luong review, SKU, URL
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


# ── Hang so dac thu tiki.vn ─────────────────────────────────────────────
TIKI_ORIGIN = "https://tiki.vn"

# URL mac dinh: listing san pham cua mot shop/category (phan trang bang cursor).
DEFAULT_SHOP_URL = (
    "https://tiki.vn/cua-hang/tiki-trading"
    "?t=product&cid=120473&cursor=0&category_id=316&parent_id=8322"
)

# Tiki tra ve toi da 25 san pham / page (limit=25) khi phan trang bang cursor.
DEFAULT_PAGE_SIZE = 25
# Toc do phan trang: moi trang cach nhau 25 cursor.
PAGE_STEP = 25

# Regex nhan dien URL san pham tiki.vn: /<slug>-p<id>.html
TIKI_PRODUCT_URL_RE = re.compile(r'-p\d+\.html', re.IGNORECASE)


def register_presets():
    """Register preset for Tiki Shop Crawler plugin."""
    preset = {
        "id": "tiki-shop-crawler",
        "name": "Tiki Shop Crawler",
        "description": "Crawl sản phẩm từ shop/category tiki.vn (Google Chrome) với custom extraction rules và export ra Excel",
        "icon": "ShoppingCartIcon",
        "icon_color": "#189eff",
        "project_settings": {
            "name": "",
            "description": "Crawl sản phẩm từ shop/category tiki.vn",
            "enabled": True,
            "crawlDelay": 1500,
            "userAgent": "CrawlFlow/1.0",
            "concurrency": 1,
            "executionMode": "queue",
            "groupExport": True,
            "groupFormat": "name",
            "refreshStrategy": "update_only",
            "updateMethod": "check_first_page_until_duplicate",
            "refreshInterval": 3600
        },
        "nodes": [
            {
                "id": "ds-tiki",
                "type": "start",
                "position": {"x": 50, "y": 50},
                "data": {
                    "pluginConfig": {"shop_url": DEFAULT_SHOP_URL},
                    "pluginSourceType": "tiki-shop-crawler",
                    "sourceType": "url",
                    "sourceValue": DEFAULT_SHOP_URL,
                    "urlSettings": {
                        "httpClient": {
                            "clientType": "chrome",
                            "headless": False
                        }
                    }
                },
                "deletable": True,
                "draggable": True,
                "width": 320,
                "height": 127,
                "zIndex": 0,
                "parentNode": None
            },
            {
                "id": "pre-1",
                "type": "preprocessor",
                "position": {"x": -568, "y": 38},
                "data": {
                    "csvDelimiter": ",",
                    "csvHasHeader": True,
                    "extractRules": [],
                    "inputType": "html",
                    "itemSelector": "",
                    "jsonItemPath": "",
                    "urlPatterns": [
                        {
                            "enabled": True,
                            "type": "regex",
                            "value": ".*-p\\d+\\.html",
                        }
                    ],
                },
                "deletable": True,
                "draggable": True,
                "width": 320,
                "height": 316,
                "zIndex": 0,
                "selected": False,
                "dragging": False,
            },
            {
                "id": "repository-node",
                "type": "repository",
                "position": {"x": 50, "y": 329},
                "data": {},
                "deletable": True,
                "draggable": True,
                "width": 320,
                "height": 183,
                "zIndex": 0,
                "parentNode": None,
            },
            {
                "id": "worker-1",
                "type": "worker",
                "position": {"x": 40, "y": 641},
                "data": {
                    "client_type": "chrome",
                    "detectionLogic": "and",
                    "detectionRules": [
                        {
                            "condition": "exists",
                            "id": "1783651265684",
                            "pattern": ".*-p\\d+\\.html",
                            "selector": "",
                            "type": "url-format",
                            "value": "",
                        }
                    ],
                },
                "deletable": True,
                "draggable": True,
                "width": 320,
                "height": 211,
                "zIndex": 0,
                "parentNode": None,
            },
            {
                "id": "ext-1",
                "type": "html-data-extractor",
                "position": {
                    "x": -492.44587280108254,
                    "y": 435.9208389715832,
                },
                "data": {
                    "customRules": [
                        {
                            "extract": "text",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-1",
                            "name": "product_name",
                            "selector": "h1",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-2",
                            "name": "price",
                            "selector": ".product-price__current-price",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-3",
                            "name": "old_price",
                            "selector": ".product-price__original-price",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-4",
                            "name": "discount_rate",
                            "selector": ".product-price__discount-rate",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "json-ld",
                            "id": "preset-tiki-json-1",
                            "jsonPath": "sku",
                            "name": "sku",
                        },
                        {
                            "attribute": "content",
                            "extract": "attribute",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-5",
                            "name": "description",
                            "selector": "meta[property='og:description']",
                        },
                        {
                            "attribute": "src",
                            "extract": "attribute",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-6",
                            "name": "image_url",
                            "selector": "[data-view-id='pdp_main_view_gallery'] img",
                        },
                        {
                            "attribute": "src",
                            "extract": "attribute",
                            "extractFrom": "html-element",
                            "extractMultiple": True,
                            "id": "preset-tiki-html-7",
                            "name": "images",
                            "selector": "[data-view-id='pdp_main_view_gallery'] img",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "html-element",
                            "extractMultiple": True,
                            "id": "preset-tiki-html-8",
                            "name": "category",
                            "selector": "[data-view-id='breadcrumb_container'] a",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "html-element",
                            "id": "preset-tiki-html-9",
                            "name": "stock_quantity",
                            "selector": "[data-view-id='pdp_quantity_sold']",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "json-ld",
                            "id": "preset-tiki-json-2",
                            "jsonPath": "brand.name",
                            "name": "brand_name",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "json-ld",
                            "id": "preset-tiki-json-3",
                            "jsonPath": "offers.seller.name",
                            "name": "seller_name",
                        },
                        {
                            "extract": "text",
                            "extractFrom": "json-ld",
                            "id": "preset-tiki-json-4",
                            "jsonPath": "aggregateRating.reviewCount",
                            "name": "review_count",
                        },
                    ],
                    "presets": ["ecommerce-product"],
                    "inspectorUrl": DEFAULT_SHOP_URL,
                    "inspectorLoading": False,
                    "inspectorHtmlContent": "",
                },
                "deletable": True,
                "draggable": True,
                "width": 320,
                "height": 239,
                "zIndex": 0,
                "selected": False,
                "dragging": False,
                "positionAbsolute": {
                    "x": -492.44587280108254,
                    "y": 435.9208389715832,
                },
            },
            {
                "id": "proc-1",
                "type": "processor",
                "position": {"x": 52, "y": 914},
                "data": {
                    "processorType": "generate-excel-file",
                    "settings": {
                        "autoMapHeaders": True,
                        "columnMapping": {},
                        "fileName": "tiki_crawl_results_{{date}}.xlsx",
                        "includeHeader": True,
                        "sheetName": "Sheet1",
                    },
                },
                "deletable": True,
                "draggable": True,
                "width": 320,
                "height": 167,
                "zIndex": 0,
                "parentNode": None,
            },
            {
                "id": "fetch-data-ds-tiki",
                "type": "fetchData",
                "position": {"x": 50, "y": 370},
                "data": {
                    "sourceType": "url",
                    "label": "Fetch Data (url)",
                },
                "deletable": False,
                "width": 320,
                "height": 243,
            },
            {
                "id": "completion-node",
                "type": "completion",
                "position": {"x": 52, "y": 1214},
                "data": {},
                "deletable": False,
                "draggable": False,
                "width": 320,
                "height": 157,
            },
        ],
        "edges": [
            {
                "id": "e-ds-pre",
                "source": "ds-tiki",
                "target": "pre-1",
                "sourceHandle": None,
                "targetHandle": None,
                "type": "smoothstep",
                "animated": False,
                "data": {},
            },
            {
                "id": "e-repo-worker",
                "source": "repository-node",
                "target": "worker-1",
                "sourceHandle": None,
                "targetHandle": None,
                "type": "smoothstep",
                "animated": False,
                "data": {},
            },
            {
                "id": "e-ext-worker",
                "source": "ext-1",
                "target": "worker-1",
                "sourceHandle": None,
                "targetHandle": None,
                "type": "smoothstep",
                "animated": False,
                "data": {},
            },
            {
                "id": "e-worker-proc",
                "source": "worker-1",
                "target": "proc-1",
                "sourceHandle": None,
                "targetHandle": None,
                "type": "smoothstep",
                "animated": False,
                "data": {},
            },
            {
                "id": "e-pre-1-fetch-data-ds-tiki",
                "source": "pre-1",
                "target": "fetch-data-ds-tiki",
                "animated": True,
            },
            {
                "id": "e-fetch-data-ds-tiki-repository-node",
                "source": "fetch-data-ds-tiki",
                "target": "repository-node",
                "animated": True,
            },
            {
                "id": "e-proc-1-completion-node",
                "source": "proc-1",
                "target": "completion-node",
                "type": "smoothstep",
            },
        ],
    }
    return json.dumps([preset])


# ── Helper URL / config ─────────────────────────────────────────────────
def _cfg_get(config, *keys):
    """Lay gia tri tu config, thu ca snake_case lan camelCase."""
    for key in keys:
        val = config.get(key)
        if val is not None and str(val) != "":
            return val
    return None


def _tiki_shop_url(source_url):
    """Quy ve URL shop/category co the dung de phan trang cursor."""
    url = (source_url or "").strip()
    if not url:
        return DEFAULT_SHOP_URL
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" and parsed.scheme != "http":
        url = TIKI_ORIGIN + ("/" + url.lstrip("/") if url.startswith("/") else "/" + url)
    return url


def _tiki_listing_url(base_url, cursor):
    """Set tham so cursor vao URL listing cua tiki.vn.

    Tiki phan trang theo cursor (0, 25, 50, ...), moi trang 25 san pham.
    Giữ nguyen cac tham so khac (t, cid, category_id, parent_id, ...).
    """
    parsed = urllib.parse.urlparse(base_url)
    qd = urllib.parse.parse_qs(parsed.query, keep_blank_values=True)
    qd["cursor"] = [str(cursor)]
    new_query = urllib.parse.urlencode(qd, doseq=True)
    return urllib.parse.urlunparse((
        parsed.scheme or "https",
        parsed.netloc,
        parsed.path,
        parsed.params,
        new_query,
        parsed.fragment
    ))


# ── Parser __NEXT_DATA__ cua tiki (Next.js SSR) ─────────────────────────
def _extract_next_data(html):
    """Parse JSON trong script __NEXT_DATA__ cua trang tiki.vn."""
    if not html:
        return None
    match = re.search(
        r'<script\b(?=[^>]*\bid=["\']__NEXT_DATA__["\'])[^>]*>(.*?)</script>',
        html,
        re.DOTALL | re.IGNORECASE,
    )
    if not match:
        return None
    try:
        return json.loads(unescape(match.group(1)).strip())
    except (TypeError, ValueError, json.JSONDecodeError):
        return None


def _iter_dicts_with_key(node, key):
    """Duyet de quy moi dict (va nested) co chua key cho truoc."""
    if isinstance(node, dict):
        if key in node:
            yield node
        for value in node.values():
            yield from _iter_dicts_with_key(value, key)
    elif isinstance(node, list):
        for value in node:
            yield from _iter_dicts_with_key(value, key)


def _extract_next_data_products(next_data):
    """Lay danh sach san pham tu __NEXT_DATA__ cua trang listing.

    Tiki render san pham trong props.initialState.desktop.sellerStore.widgets
    (array cac dict co url_path dang '<slug>-p<id>.html').
    """
    products = []
    seen = set()
    if not isinstance(next_data, dict):
        return products
    for node in _iter_dicts_with_key(next_data, "url_path"):
        url_path = node.get("url_path") or ""
        if not url_path or not TIKI_PRODUCT_URL_RE.search(url_path):
            continue
        pid = node.get("id")
        key = (str(pid), url_path)
        if key in seen:
            continue
        seen.add(key)
        products.append(node)
    return products


def _extract_next_data_product(next_data):
    """Lay object san pham chi tiet tu __NEXT_DATA__ cua trang product.

    Duong dan: props.initialState.productv2.productData.response.data
    """
    if not isinstance(next_data, dict):
        return None
    try:
        return (
            next_data.get("props", {})
            .get("initialState", {})
            .get("productv2", {})
            .get("productData", {})
            .get("response", {})
            .get("data")
        )
    except AttributeError:
        return None


def _extract_tiki_listing_links(html, base_url):
    """Lay URL chi tiet san pham tiki.vn tu trang listing.

    Uu tien 1: __NEXT_DATA__ (SSR) - chinh xac nhat, tiki nhung day.
    Uu tien 2: the <a href> co pattern /...-p<id>.html.
    """
    links = []
    seen = set()
    origin = TIKI_ORIGIN

    next_data = _extract_next_data(html)
    if next_data:
        for prod in _extract_next_data_products(next_data):
            url_path = (prod.get("url_path") or "").strip("/")
            if not url_path:
                continue
            full = urllib.parse.urljoin(origin + "/", url_path.lstrip("/"))
            if full not in seen:
                seen.add(full)
                links.append(full)
        if links:
            return links

    # Fallback: <a href> regex
    pattern = re.compile(
        r'href=["\']((?:https?://[^"\'/][^"\']*)?/[^"\']*-p\d+\.html(?:[?#][^"\']*)?)["\']',
        re.IGNORECASE,
    )
    for match in pattern.finditer(unescape(html)):
        href = match.group(1)
        full = urllib.parse.urljoin(origin + "/", href.lstrip("/"))
        if full not in seen:
            seen.add(full)
            links.append(full)
    return links


# ── Khai bao preprocessor ───────────────────────────────────────────────
def register_preprocessors():
    """Dang ky preprocessor chuyen trang listing tiki.vn thanh danh sach san pham."""
    return json.dumps([{
        "id": "tiki-store-products",
        "name": "Tiki Store Products",
        "plugin_id": "",
        "input_type": "html",
        "platform": "tiki.vn",
        "config": {
            "input_type": "html",
            "item_selector": None,
            "url_patterns": [],
            "extract_rules": [],
            "csv_delimiter": None,
            "csv_has_header": None,
            "json_item_path": None,
            "client_type": "chrome",
            "client_timeout_secs": None,
            "client_headless": None,
            "wait_for_selector": None,
            "wait_for_content": None,
            "wait_timeout_ms": None,
        },
    }])


def preprocess_data(data_json):
    """Lay URL san pham tu trang listing tiki.vn (phan trang bang cursor)
    va luu vao DB NGAY khi tim thay (de progress bar cap nhat realtime).

    Khac voi truoc day (tra ve 1 loat 'listing_url' de Rust fetch sau), o day
    plugin tu fetch tung trang, trich product URL va goi crawlflow.save_raw_items
    luu tung luot vao DB. UI ('items_pending') se tang len tung buoc thay vi
    chi nhay 1 lan sau khi xong het.

    Tra ve [] de bao cho Rust rang item da duoc luu truc tiep vao DB
    (Rust se skip Stage B trich URL trung lap).
    """
    payload = json.loads(data_json) if isinstance(data_json, str) else data_json
    html = payload.get("raw_data", "")
    source_url = payload.get("source_url", DEFAULT_SHOP_URL)
    project_id = payload.get("project_id", "")
    db_path = payload.get("db_path", "")
    config = payload.get("config", {}) or {}
    refresh_strategy = payload.get("refresh_strategy", "refresh")

    # Skip listing crawl for update_only — existing items in DB are sufficient.
    # Only refresh / refresh_update need to re-crawl listing pages.
    if refresh_strategy == "update_only":
        crawlflow.log("[TikiShop][preprocess] update_only — skip listing crawl", "info")
        return json.dumps([])

    shop_url = _tiki_shop_url(
        _cfg_get(config, "shop_url", "shopUrl") or source_url
    )

    # Chon kenh fetch: mac dinh dung Chrome (toan bo plugin).
    client_type = (_cfg_get(config, "client_type", "clientType") or "chrome").strip().lower()
    if client_type not in ("reqwest", "chrome", "cdp"):
        client_type = "chrome"
    headless = bool(_cfg_get(config, "headless"))

    # Pagination: tiki phan trang bang cursor (0, 25, 50, ...).
    # max_pages = 0 hoac None => unlimited.
    max_pages = int((_cfg_get(config, "max_pages") or 0) or 0)
    if max_pages < 1:
        max_pages = 0  # 0 = unlimited

    # Cac page da hoan thanh (tu cycle truoc) se duoc skip khi resume.
    done_pages = set()
    if project_id:
        try:
            done_pages = set(crawlflow.get_done_pages(project_id))
        except Exception:
            done_pages = set()

    total_saved = 0
    page_num = 1
    cursor = 0
    delay_ms = int((_cfg_get(config, "delay_ms") or 1000) or 1000)
    if delay_ms < 0:
        delay_ms = 0

    while True:
        if max_pages and page_num > max_pages:
            break
        page_url = _tiki_listing_url(shop_url, cursor)

        if page_num in done_pages:
            crawlflow.log(
                f"[TikiShop][preprocess] Bo qua page {page_num} (da done, resume)",
                "info",
            )
            cursor += PAGE_STEP
            page_num += 1
            continue

        crawlflow.log(
            f"[TikiShop][preprocess] Fetch listing page {page_num}: {page_url}",
            "info",
        )

        listing_html = ""
        if page_num == 1 and html:
            # Dung HTML da fetch san tu Stage A (khong fetch lai trang dau).
            listing_html = html
        else:
            try:
                raw = crawlflow.fetch_url(page_url, None, client_type, headless)
                result = json.loads(raw) if isinstance(raw, str) else raw
                listing_html = result.get("body", "") if isinstance(result, dict) else ""
            except Exception as e:
                crawlflow.log(
                    f"[TikiShop][preprocess] Loi fetch page {page_num}: {e}", "error"
                )
                break

        if not listing_html:
            crawlflow.log(
                f"[TikiShop][preprocess] Listing page {page_num} rong", "warn"
            )
            break

        product_urls = _extract_tiki_listing_links(listing_html, page_url)
        crawlflow.log(
            f"[TikiShop][preprocess] Tim thay {len(product_urls)} product URL o trang {page_num}",
            "info",
        )

        # Luu tung luot product URL vao DB NGAY de UI cap nhat realtime.
        if product_urls and project_id and db_path:
            raw_items = []
            for p_url in product_urls:
                raw_items.append({
                    "source_url": p_url,
                    "item_type": "url",
                    "item_hash": hashlib.sha256(p_url.encode("utf-8")).hexdigest(),
                })
            try:
                res = json.loads(crawlflow.save_raw_items(project_id, db_path, json.dumps(raw_items)))
                saved = int(res.get("inserted", 0))
                total_saved += saved
                crawlflow.log(
                    f"[TikiShop][preprocess] Da luu {saved} URL moi (tong {total_saved}) vao DB",
                    "info",
                )
            except Exception as e:
                crawlflow.log(
                    f"[TikiShop][preprocess] Loi save_raw_items: {e}", "warn"
                )

        # Danh dau page done de resume.
        if project_id:
            try:
                crawlflow.mark_page_done(project_id, page_url, page_num, len(product_urls))
            except Exception:
                pass

        # Tiki moi trang toi da 25 san pham. Neu it hon => het phan trang.
        if len(product_urls) < DEFAULT_PAGE_SIZE:
            crawlflow.log(
                f"[TikiShop][preprocess] Het phan trang tai trang {page_num} "
                f"(chi {len(product_urls)}/{DEFAULT_PAGE_SIZE})",
                "info",
            )
            break

        cursor += PAGE_STEP
        page_num += 1
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)

    crawlflow.log(
        f"[TikiShop][preprocess] Hoan tat: {total_saved} product URL da luu vao DB "
        f"(max_pages={max_pages or 'unlimited'}, bo qua {len(done_pages)} done)",
        "info",
    )
    return json.dumps([])


# ── Helpers parser chung ────────────────────────────────────────────────
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


def _stock_number(val, default=""):
    """Trich so luong ton kho tu chuoi, VD '10 sản phẩm có sẵn' -> '10'."""
    if val is None:
        return default
    m = re.search(r'\d+', str(val))
    return m.group(0) if m else default


def _clean_category(value):
    """Chuan hoa chuoi category tiki.vn:
    - Bo phan tu 'Home'/'Trang chủ' dau tien cua breadcrumb
    - Noi cac phan con lai bang '>'
    """
    if value is None:
        return ""
    if isinstance(value, list):
        parts = [str(v) for v in value]
    else:
        parts = re.split(r'\s*(?:>|>>|›|»|\||;)\s*', str(value))
    cleaned = []
    for p in parts:
        q = re.sub(r'^[\s>›»]+', '', str(p))
        q = re.sub(r'[\s>›»]+$', '', q)
        q = q.replace('>', '').replace('›', '').replace('»', '').strip()
        if q:
            cleaned.append(q)
    # Bo phan tu Home/Trang chủ (ten cung khong mang y nghia danh muc)
    if cleaned and re.match(r'^(home|trang[\s_]?chủ)$', cleaned[0], re.IGNORECASE):
        cleaned = cleaned[1:]
    return ">".join(cleaned)


# Kich thuoc anh lon nhat duoc ho tro boi salt.tikicdn.com CDN
_TIKI_IMAGE_SIZE = "w1280"

# Regex bat tien to kich thuoc cache cua tikicdn (vd /cache/w780/, /cache/750x750/)
_TIKI_SIZE_PREFIX_RE = re.compile(r'^(https?://salt\.tikicdn\.com/cache/)[^/]+/(.*)$')


def _upgrade_tiki_image(url):
    """Chuyen URL anh tiki.vn tu kich thuoc nho (vd /cache/w90/) sang kich thuoc
    lon nhat (w1280). Chi ap dung voi CDN salt.tikicdn.com.
    """
    if not url:
        return url
    url = url.strip()
    if not url.startswith("https://salt.tikicdn.com/"):
        return url
    m = _TIKI_SIZE_PREFIX_RE.match(url)
    if not m:
        return url
    return f"{m.group(1)}{_TIKI_IMAGE_SIZE}/{m.group(2)}"


def _upgrade_tiki_images(value):
    """Upgrade mot hoac nhieu URL anh (chuoi cach nhau boi khoang trang hoac list)."""
    if value is None:
        return value
    if isinstance(value, list):
        return [_upgrade_tiki_image(v) for v in value]
    if isinstance(value, str):
        parts = value.split()
        upgraded = [_upgrade_tiki_image(u) for u in parts]
        if len(parts) > 1:
            return " ".join(upgraded)
        return upgraded[0] if upgraded else ""
    return value


# ── Reusable "library" filter, registered with the backend ───────────────
# Rust invokes this automatically on every item's parsed data (the `images`
# array) — no hard-coded field surgery inside process_data.
def tiki_filter_parsed_data(data_json):
    """Filter chay tu dong tren parsed data cua tung item.

    Input: JSON string cua mot list gom 1 object item.
    Output: JSON string cua list (cung kich thuoc) voi image/images da duoc
    chuan hoa (len size w1280).
    """
    try:
        items = json.loads(data_json) if isinstance(data_json, str) else data_json
    except Exception:
        return data_json

    for item in items:
        if not isinstance(item, dict):
            continue
        if "image" in item:
            item["image"] = _upgrade_tiki_image(item.get("image", ""))
        if "image_url" in item:
            item["image_url"] = _upgrade_tiki_image(item.get("image_url", ""))
        if "images" in item:
            item["images"] = _upgrade_tiki_images(item.get("images"))
        # stock_quantity: text goc co dang "10 sản phẩm có sẵn" -> chi giu lai so.
        if "stock_quantity" in item:
            item["stock_quantity"] = _stock_number(item.get("stock_quantity"))
        # stock: dong bo voi stock_quantity neu co (uu tien so luong chinh xac).
        if "stock_quantity" in item:
            item["stock"] = item["stock_quantity"]

    return json.dumps(items)


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


def _tiki_status(status):
    """Chuyen trang thai tieng Anh cua tiki sang tieng Viet."""
    mapping = {
        "available": "Còn hàng",
        "in_stock": "Còn hàng",
        "out_of_stock": "Hết hàng",
        "sold_out": "Hết hàng",
        "unavailable": "Hết hàng",
        "preorder": "Đặt trước",
        "discontinued": "Ngừng kinh doanh",
    }
    s = (status or "").strip().lower()
    return mapping.get(s, status or "Còn hàng")


def _parse_category_from_breadcrumbs(breadcrumbs, exclude_leaf=None):
    """Breadcrumb tiki: [{ 'name': 'Home' }, { 'name': 'Dien thoai' }, ...].

    Bo phan tu cuoi cung neu no trung voi ten san pham (tiki luon them san
    pham hien tai vao cuoi breadcrumb).
    """
    if not breadcrumbs:
        return ""
    names = []
    for item in breadcrumbs:
        if isinstance(item, dict):
            n = (item.get("name") or "").strip()
            if n:
                names.append(n)
    if exclude_leaf and names and names[-1] == exclude_leaf:
        names = names[:-1]
    return _clean_category(names)


def _parse_specs_from_next_data(product):
    """specifications tiki: [{ 'name': 'Thông số', 'specifications': [{name,value}] }]."""
    specs = {}
    groups = product.get("specifications") or []
    for group in groups:
        if not isinstance(group, dict):
            continue
        for spec in group.get("specifications", []) or []:
            if not isinstance(spec, dict):
                continue
            key = (spec.get("name") or "").strip()
            value = (spec.get("value") or "").strip()
            if key and value:
                specs[key] = value
    return specs


def _parse_product_from_html(html, url):
    """Phan tich HTML trang chi tiet san pham tiki.vn.

    Uu tien: __NEXT_DATA__ (SSR) -> props.initialState.productv2.productData.response.data
    Fallback: the <h1>, gia tri gia .product-price__*, JSON-LD schema.org.
    """
    parser = HTMLContentParser()
    text = parser.extract_text(html)

    name = ""
    price = 0
    old_price = 0
    discount = 0
    discount_rate = 0
    image = ""
    images = []
    sku = ""
    description = ""
    availability = ""
    category = ""
    specs = {}
    seller_name = ""
    seller_id = ""
    brand_name = ""
    brand_id = ""
    review_count = ""
    rating_average = ""
    stock = ""
    stock_quantity = ""
    quantity_sold = ""

    next_data = _extract_next_data(html)
    product = _extract_next_data_product(next_data)

    if isinstance(product, dict):
        name = (product.get("name") or "").strip()

        price = _safe_float(product.get("price"), 0)
        old_price = _safe_float(
            product.get("list_price") or product.get("original_price"), 0
        )
        discount = _safe_float(product.get("discount"), 0)
        discount_rate = _safe_int(product.get("discount_rate"), 0)

        sku = str(product.get("sku") or "").strip()

        brand = product.get("brand") or {}
        if isinstance(brand, dict):
            brand_id = brand.get("id")
            brand_name = (brand.get("name") or "").strip()

        # Tiki seller: field 'current_seller' (hoac 'seller') trong productv2
        seller = product.get("current_seller") or product.get("seller") or {}
        if isinstance(seller, dict):
            seller_id = seller.get("id")
            seller_name = (seller.get("name") or "").strip()

        # Mo ta la HTML -> strip the
        desc = product.get("description") or ""
        if isinstance(desc, str) and desc:
            description = _get_text(desc, "")

        # Hinh anh: images:[{ large_url, thumbnail_url }]
        img_items = product.get("images") or []
        if isinstance(img_items, list):
            for img in img_items:
                if isinstance(img, dict):
                    large = img.get("large_url") or img.get("thumbnail_url") or ""
                    if large:
                        images.append(large)
        thumb = product.get("thumbnail_url") or ""
        if images:
            image = images[0]
        elif thumb:
            image = thumb

        # Ton kho + so luong da ban
        stock_item = product.get("stock_item") or {}
        if isinstance(stock_item, dict):
            qty = stock_item.get("qty")
            if qty is not None:
                stock_quantity = str(qty)
                stock = stock_quantity
        qs = product.get("quantity_sold")
        if qs is not None:
            quantity_sold = str(qs)

        review_count = str(product.get("review_count") or "").strip() or ""
        ra = product.get("rating_average")
        rating_average = str(ra) if ra is not None else ""

        availability = _tiki_status(product.get("inventory_status") or "")

        category = _parse_category_from_breadcrumbs(product.get("breadcrumbs"), exclude_leaf=name)
        specs = _parse_specs_from_next_data(product)

    # ── Fallback khi __NEXT_DATA__ khong co du lieu ────────────────────
    if not name:
        h1_match = re.search(r'<h1[^>]*>(.*?)</h1>', html, re.DOTALL)
        if h1_match:
            name = _get_text(h1_match.group(1), "").strip()

    if not name:
        # Fallback: JSON-LD BreadcrumbList (last item = product name)
        for ld_text in re.findall(
            r'<script[^>]*type=["\']application/ld\+json["\'][^>]*>(.*?)</script>',
            html, re.DOTALL | re.IGNORECASE
        ):
            try:
                ld_data = json.loads(ld_text)
                if isinstance(ld_data, dict) and ld_data.get("@type") == "BreadcrumbList":
                    items = ld_data.get("itemListElement", [])
                    if items:
                        last_item = items[-1]
                        name = (last_item.get("name") or "").strip()
                        if name:
                            break
            except (json.JSONDecodeError, AttributeError):
                pass

    # Fallback: name tu URL slug: /<slug>-p<id>.html
    if not name:
        slug_match = re.search(r'/([^/-]+(?:-[^/]+)*)-p\d+\.html', url)
        if slug_match:
            name = slug_match.group(1).replace('-', ' ').strip()

    if price == 0:
        price_patterns = [
            r'<span[^>]*class\s*=\s*["\'][^"\']*product-price__current-price[^"\']*["\'][^>]*>([^<]+)</span>',
            r'<span[^>]*class\s*=\s*["\'][^"\']*price[^"\']*["\'][^>]*>([^<]+)</span>',
        ]
        for pat in price_patterns:
            m = re.search(pat, html)
            if m:
                price = _safe_float(m.group(1))
                if price > 0:
                    break

    if old_price == 0 and price > 0:
        old_match = re.search(
            r'<span[^>]*class\s*=\s*["\'][^"\']*product-price__original-price[^"\']*["\'][^>]*>([^<]+)</span>',
            html,
        )
        if old_match:
            old_price = _safe_float(old_match.group(1))

    if discount == 0 and old_price > price:
        discount = old_price - price
    if discount_rate == 0 and old_price > price:
        discount_rate = int(round((old_price - price) / old_price * 100))

    if not image:
        og_image = re.search(
            r'<meta[^>]*property\s*=\s*["\']og:image["\'][^>]*content\s*=\s*["\']([^"\']+)["\']',
            html,
        )
        if og_image:
            image = og_image.group(1)
        if image and image not in images:
            images.insert(0, image)

    if not sku:
        try:
            product_block = None
            for ld_text in re.findall(
                r'<script[^>]*type=["\']application/ld\+json["\'][^>]*>(.*?)</script>',
                html, re.DOTALL | re.IGNORECASE
            ):
                ld_data = json.loads(ld_text)
                nodes = []
                if isinstance(ld_data, dict):
                    nodes = ld_data.get("@graph", [ld_data])
                elif isinstance(ld_data, list):
                    nodes = ld_data
                for node in nodes:
                    if isinstance(node, dict) and node.get("@type") == "Product":
                        product_block = node
                        break
                if product_block:
                    break
            if product_block:
                sku = str(product_block.get("sku") or "").strip()
                if product_block.get("offers") and isinstance(product_block["offers"], dict):
                    offer = product_block["offers"]
                    if seller_name == "" and offer.get("seller") and isinstance(offer["seller"], dict):
                        seller_name = (offer["seller"].get("name") or "").strip()
                    if rating_average == "" and offer.get("aggregateRating") is None:
                        pass
                if not brand_name and product_block.get("brand") and isinstance(product_block["brand"], dict):
                    brand_name = (product_block["brand"].get("name") or "").strip()
                if not review_count and isinstance(product_block.get("aggregateRating"), dict):
                    review_count = str(product_block["aggregateRating"].get("reviewCount") or "")
                if not rating_average and isinstance(product_block.get("aggregateRating"), dict):
                    rating_average = str(product_block["aggregateRating"].get("ratingValue") or "")
                if not availability:
                    offer = product_block.get("offers") or {}
                    if isinstance(offer, dict):
                        availability = _tiki_status(offer.get("availability") or "")
        except (json.JSONDecodeError, AttributeError, TypeError):
            pass

    # Mo ta fallback: og:description
    if not description:
        og_desc = re.search(
            r'<meta[^>]*(?:name|property)\s*=\s*["\'](?:og:)?description["\'][^>]*content\s*=\s*["\']([^"\']+)["\']',
            html, re.IGNORECASE,
        )
        if og_desc:
            description = og_desc.group(1).strip()

    # Ton kho fallback
    if not stock:
        stock_match = re.search(r'(?:Số lượng|Tồn kho|Còn lại)\s*[:;]?\s*(\d+)', html, re.IGNORECASE)
        if stock_match:
            stock = stock_match.group(1)
    if not stock_quantity:
        stock_quantity = stock

    if not category:
        breadcrumb = re.search(r'<ul[^>]*class\s*=\s*["\'][^"\']*breadcrumb[^"\']*["\']>(.*?)</ul>', html, re.DOTALL)
        if breadcrumb:
            cats = re.findall(r'<a[^>]*>(.*?)</a>', breadcrumb.group(1))
            if len(cats) >= 2:
                category = _clean_category(_get_text(cats[-1], "").strip())

    if not specs:
        specs = _extract_specs(html)

    return {
        "url": url,
        "name": name or os.path.basename(url),
        "price": price,
        "old_price": old_price,
        "discount": discount,
        "discount_rate": discount_rate,
        "image_url": image,
        "image": image,
        "images": images,
        "sku": sku,
        "description": description[:500] if description else "",
        "specs": specs,
        "category": _clean_category(category),
        "stock": stock,
        "stock_quantity": stock_quantity,
        "quantity_sold": quantity_sold,
        "seller_name": seller_name,
        "seller_id": seller_id,
        "brand_name": brand_name,
        "brand_id": brand_id,
        "review_count": review_count,
        "rating_average": rating_average,
        "availability": availability or "Còn hàng",
        "crawled_at": datetime.now().strftime("%Y-%m-%dT%H:%M:%S"),
        "raw_html": html,
    }


def on_load(config=None):
    crawlflow.log("[TikiShop] Plugin loaded", "info")
    try:
        import openpyxl
        crawlflow.log("[TikiShop] openpyxl available - will use Excel output", "info")
    except ImportError:
        crawlflow.log("[TikiShop] openpyxl not installed - will use CSV output", "warn")

    # Dang ky filter "library" chay tu dong tren parsed data cua tung item.
    # Rust se goi tiki_filter_parsed_data() moi khi co parsed data (mang images).
    try:
        crawlflow.register_filter("parsed_data", tiki_filter_parsed_data)
        crawlflow.log("[TikiShop] Registered 'parsed_data' filter", "info")
    except Exception as e:
        crawlflow.log(f"[TikiShop] register_filter failed: {e}", "warn")


def _crawl_all_products(shop_url, max_pages, delay_ms, client_type=None, headless=None, project_id=None, db_path=None, refresh_strategy="refresh", update_method="check_first_page_until_duplicate"):
    """Tu crawl toan bo san pham cua shop/category tiki.vn (phan trang cursor + parse).

    refresh_strategy:
        'refresh'           — crawl lai hoan toan tu trang 1
        'refresh_update'    — crawl lai hoan toan (pipeline skip re-process)
        'update_only'       — chi quet data moi, dung theo update_method
    update_method (cho update_only):
        'check_first_page_until_duplicate' — quet tu trang 1, dung khi het san pham
        'check_last_page'                  — quet tu trang cuoi cung, dung khi het san pham
    """
    def _fetch(url):
        # Goi Python SDK fetch_url voi kem client_type de chon kenh reqwest/chrome.
        return crawlflow.fetch_url(url, None, client_type, headless)

    base_listing_url = _tiki_shop_url(shop_url)
    crawlflow.log(
        f"[TikiShop] Listing base={base_listing_url} | client={client_type}", "info"
    )

    products = []
    seen_urls = set()
    page_num = 1
    cursor = 0

    # Cac page da hoan thanh o chu ky crawl truoc (luu trong bang crawl_pages)
    # se duoc bo qua de ho tro resume khi service bi dung dot ngot.
    done_pages = set()
    if project_id:
        try:
            done_pages = set(crawlflow.get_done_pages(project_id))
        except Exception:
            done_pages = set()

    while True:
        page_url = _tiki_listing_url(base_listing_url, cursor)
        if page_num in done_pages:
            crawlflow.log(f"[TikiShop] Bo qua page {page_num} (da done, resume)", "info")
            cursor += PAGE_STEP
            page_num += 1
            if max_pages and page_num > max_pages:
                break
            continue
        crawlflow.log(f"[TikiShop] Listing page {page_num}: {page_url}", "info")

        try:
            raw = _fetch(page_url)
            listing_result = json.loads(raw) if isinstance(raw, str) else raw
        except Exception as e:
            crawlflow.log(f"[TikiShop] Loi fetch listing page {page_num}: {e}", "error")
            break

        listing_html = listing_result.get("body", "") if isinstance(listing_result, dict) else ""
        if not listing_html:
            crawlflow.log(f"[TikiShop] Listing page {page_num} rong", "warn")
            break

        # Trich product URLs tu listing HTML.
        product_urls = _extract_tiki_listing_links(listing_html, page_url)
        crawlflow.log(
            f"[TikiShop] Tim thay {len(product_urls)} product URL o trang {page_num}",
            "info",
        )

        # Neu khong tim thay URL nao => het san pham
        if not product_urls and refresh_strategy != "refresh":
            crawlflow.log(f"[TikiShop] Khong con san pham, dung tai trang {page_num}", "info")
            break

        # Luu tung luot product URL vao DB NGAY de progress bar (pending)
        # cap nhat realtime thay vi chi nhay 1 lan sau khi xong het.
        saved = 0
        if product_urls and project_id and db_path:
            raw_items = []
            for p_url in product_urls:
                raw_items.append({
                    "source_url": p_url,
                    "item_type": "url",
                    "item_hash": hashlib.sha256(p_url.encode("utf-8")).hexdigest(),
                })
            try:
                res = json.loads(crawlflow.save_raw_items(project_id, db_path, json.dumps(raw_items)))
                saved = int(res.get("inserted", 0))
                crawlflow.log(
                    f"[TikiShop] Da luu {saved} URL moi vao DB (trang {page_num})",
                    "info",
                )
            except Exception as e:
                crawlflow.log(f"[TikiShop] Loi save_raw_items: {e}", "warn")

        # update_only: chi dung khi trang trong khong co product URL nao.
        # Khong dung khi saved==0 vi san pham moi co o trang sau.
        if refresh_strategy == "update_only" and not product_urls:
            crawlflow.log(f"[TikiShop] Trang {page_num} trong, dung phan trang", "info")
            break

        # Danh dau page nay da hoan thanh de ho tro resume.
        if project_id:
            try:
                crawlflow.mark_page_done(project_id, page_url, page_num, len(product_urls))
            except Exception as e:
                crawlflow.log(f"[TikiShop] mark_page_done loi: {e}", "warn")

        for p_url in product_urls:
            if p_url in seen_urls:
                crawlflow.log(f"[TikiShop] Bo qua URL trung lap: {p_url}", "debug")
                continue
            seen_urls.add(p_url)
            crawlflow.log(f"[TikiShop] Fetch product ({len(seen_urls)}/{len(product_urls)} trang {page_num}): {p_url}", "info")
            t0 = time.time()
            try:
                praw = _fetch(p_url)
                pres = json.loads(praw) if isinstance(praw, str) else praw
                phtml = pres.get("body", "") if isinstance(pres, dict) else ""
                elapsed_ms = int((time.time() - t0) * 1000)
                if phtml:
                    prod = _parse_product_from_html(phtml, p_url)
                    products.append(prod)
                    name = prod.get("name") or prod.get("title") or "(khong ten)"
                    price = prod.get("price") or prod.get("current_price") or ""
                    crawlflow.log(
                        f"[TikiShop] OK {elapsed_ms}ms — \"{name}\"" + (f" | gia: {price}" if price else "") + f" | {p_url}",
                        "info",
                    )
                else:
                    crawlflow.log(f"[TikiShop] Canh bao: HTML rong sau {elapsed_ms}ms — {p_url}", "warn")
            except Exception as e:
                elapsed_ms = int((time.time() - t0) * 1000)
                crawlflow.log(f"[TikiShop] Loi fetch product sau {elapsed_ms}ms — {p_url} — {e}", "warn")

        # Tiep tuc phan trang: tiki tra ve 25 san pham / page, neu it hon
        # la da het san pham.
        if max_pages and page_num >= max_pages:
            crawlflow.log(f"[TikiShop] Da du max_pages={max_pages}", "info")
            break

        if len(product_urls) < DEFAULT_PAGE_SIZE:
            crawlflow.log(
                f"[TikiShop] Het phan trang tai trang {page_num} "
                f"(chi {len(product_urls)}/{DEFAULT_PAGE_SIZE})",
                "info",
            )
            break

        cursor += PAGE_STEP
        page_num += 1
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)

    return products


def fetch_data(config_json):
    """Crawl toan bo san pham cua shop/category tiki.vn va tra ve truc tiep
    N item 'product'.

    Plugin tu quyet dinh toan bo logic: phan trang listing (cursor), trich
    product URL, parse chi tiet. Rust chi can gom cac item nay vao raw_items
    (item_type='url') de worker + exporter xu ly tiep.

    Config:
        shop_url (str, bat buoc): URL cua shop/category (vd:
            https://tiki.vn/cua-hang/tiki-trading?t=product&cid=120473&cursor=0&category_id=316&parent_id=8322)
        max_pages (int, mac dinh: 0): gioi han so trang (0 = khong gioi han)
        delay_ms (int, mac dinh: 1000): nghi giua cac request
        project_id (str): ID project (tu dong inject)
    """
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    shop_url = _cfg_get(config, "shop_url", "shopUrl", "source_value", "sourceValue", "source_url")
    shop_url = _tiki_shop_url(shop_url)
    crawlflow.log(f"[TikiShop][fetch_data] shop_url={shop_url}", "info")

    max_pages = int((_cfg_get(config, "max_pages") or 0) or 0)
    if max_pages < 1:
        max_pages = 0  # 0 = unlimited
    delay_ms = int((_cfg_get(config, "delay_ms") or 1000) or 1000)
    if delay_ms < 0:
        delay_ms = 0

    # Chon kenh fetch: mac dinh dung Chrome (toan bo plugin dung chrome).
    # Doc tu config (clientType) hoac urlSettings.httpClient.clientType cua node.
    client_type = (_cfg_get(config, "clientType", "client_type")
                   or (config.get("urlSettings") or {}).get("httpClient", {}).get("clientType")
                   or "chrome")
    client_type = str(client_type).strip().lower()
    if client_type not in ("reqwest", "chrome", "cdp"):
        client_type = "chrome"
    headless = bool(_cfg_get(config, "headless")
                    or (config.get("urlSettings") or {}).get("httpClient", {}).get("headless", False))

    crawlflow.log(
        f"[TikiShop][fetch_data] Bat dau crawl shop={shop_url} (max_pages={max_pages}, client={client_type})",
        "info",
    )

    refresh_strategy = config.get("refresh_strategy") or "refresh"
    update_method = config.get("update_method") or "check_first_page_until_duplicate"
    crawlflow.log(
        f"[TikiShop][fetch_data] refresh_strategy={refresh_strategy}, update_method={update_method}",
        "info",
    )

    products = _crawl_all_products(shop_url, max_pages, delay_ms, client_type, headless, config.get("project_id"), config.get("db_path"), refresh_strategy, update_method)

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
        f"[TikiShop][fetch_data] Hoan tat: {len(items)} san pham",
        "info",
    )
    return json.dumps(items)


def process_data(data_json, config_json):
    """Chuan hoa du lieu san pham."""
    data = json.loads(data_json) if isinstance(data_json, str) else data_json
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    total = len(data)
    crawlflow.log(f"[TikiShop][process] Bat dau chuan hoa {total} san pham", "info")

    normalized = []
    for idx, item in enumerate(data, 1):
        norm = {
            "url": item.get("url", ""),
            "name": (item.get("name") or item.get("product_name") or "").strip(),
            "price": _safe_float(item.get("price", 0)),
            "old_price": _safe_float(item.get("old_price", 0)),
            "discount": _safe_float(item.get("discount", 0)),
            "discount_rate": _safe_int(item.get("discount_rate", 0)),
            "image": item.get("image", ""),
            "image_url": item.get("image_url", ""),
            "images": item.get("images"),
            "sku": item.get("sku", "").strip(),
            "description": (item.get("description", "") or "").strip(),
            "category": _clean_category(item.get("category", "")),
            "availability": _tiki_status(item.get("availability", "")),
            "stock": item.get("stock", ""),
            "stock_quantity": item.get("stock_quantity", ""),
            "quantity_sold": item.get("quantity_sold", ""),
            "seller_name": item.get("seller_name", ""),
            "seller_id": item.get("seller_id", ""),
            "brand_name": item.get("brand_name", ""),
            "brand_id": item.get("brand_id", ""),
            "review_count": item.get("review_count", ""),
            "rating_average": item.get("rating_average", ""),
            "crawled_at": item.get("crawled_at", datetime.now().strftime("%Y-%m-%dT%H:%M:%S")),
            "specs": item.get("specs", {}),
        }
        name = norm["name"] or "(khong ten)"
        price_str = f" | gia: {norm['price']}" if norm["price"] else ""
        crawlflow.log(
            f"[TikiShop][process] [{idx}/{total}] \"{name}\"{price_str} | {norm['url']}",
            "debug",
        )
        normalized.append(norm)

    crawlflow.log(f"[TikiShop][process] Hoan tat chuan hoa {len(normalized)}/{total} san pham", "info")
    return json.dumps(normalized)


def export_data(data_json, config_json):
    """Xuat du lieu ra Excel/CSV voi co che append + check trung.

    Dinh dang cot: STT, Gia bia, Gia ban, Giam gia, Phan tram, Ton kho,
                   Tinh trang, Thuong hieu, Ten san pham, Danh muc,
                   Nguoi ban, So luong review, SKU, URL

    Ghi log vao file dedup de tranh trung lap.
    Luon append vao file Excel (doc file cu, them rows moi, ghi de).
    """
    data = json.loads(data_json) if isinstance(data_json, str) else data_json
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    project_id = config.get("project_id", "default")
    output_dir = config.get("output_dir")
    if not output_dir:
        # Mac dinh dung thu muc Downloads cua user hien tai
        output_dir = os.path.join(os.path.expanduser("~"), "Downloads")
    os.makedirs(output_dir, exist_ok=True)

    # Lay ten shop: uu tien projectName tu config (neu co), sau do lay tu URL
    project_name = config.get("projectName", "")
    shop_url = config.get("shop_url", "")
    shop_name = "tiki_shop"

    if project_name:
        # Dung project name neu duoc cung cap
        shop_name = project_name.replace("?", "_").replace("&", "_").replace("/", "_").replace("\\", "_")
    elif shop_url:
        # Neu khong co project name, lay tu URL
        parts = shop_url.rstrip("/").split("/")
        if parts:
            shop_name = parts[-1].replace("?", "_").replace("&", "_")

    started_at = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
    total_items = len(data)

    crawlflow.log(f"[TikiShop] Bat dau export {total_items} san pham", "info")

    # ── Dedup: doc file dedup ────────────────────────────────────────
    dedup_path = os.path.join(output_dir, f".{shop_name}_dedup.json")
    seen_ids = set()
    if os.path.exists(dedup_path):
        try:
            content = crawlflow.read_file(dedup_path)
            seen_ids = set(json.loads(content))
            crawlflow.log(f"[TikiShop] Da doc {len(seen_ids)} ID da xu ly tu file dedup", "info")
        except Exception as e:
            crawlflow.log(f"[TikiShop] Loi doc dedup file: {e}", "warn")

    # Loc san pham moi
    new_products = []
    for item in data:
        dedup_key = item.get("url", "") or item.get("sku", "")
        if dedup_key and dedup_key in seen_ids:
            continue
        new_products.append(item)

    crawlflow.log(f"[TikiShop] Sau dedup: {len(new_products)} san pham moi (da bo qua {len(data) - len(new_products)} san pham trung)", "info")

    if not new_products:
        crawlflow.log("[TikiShop] Khong co san pham moi de export", "info")
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
        crawlflow.log(f"[TikiShop] Da ghi {count} san pham vao {excel_path}", "info")
    else:
        count = _export_csv(new_products, csv_path)
        crawlflow.log(f"[TikiShop] Da ghi {count} san pham vao {csv_path} (CSV)", "info")

    # Luu dedup
    for item in new_products:
        dedup_key = item.get("url", "") or item.get("sku", "")
        if dedup_key:
            seen_ids.add(dedup_key)

    try:
        crawlflow.save_file(dedup_path, json.dumps(list(seen_ids), ensure_ascii=False))
    except Exception as e:
        crawlflow.log(f"[TikiShop] Loi ghi dedup file: {e}", "warn")

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
    from openpyxl import Workbook, load_workbook

    try:
        wb = load_workbook(filepath)
        ws = wb.active
        crawlflow.log(f"[TikiShop] Mo file Excel co san: {filepath}", "info")
    except Exception:
        wb = Workbook()
        ws = wb.active
        ws.title = "San pham"
        headers = [
            "STT", "Giá bìa", "Giá bán", "Giảm giá", "Phần trăm", "Tồn kho",
            "Tình trạng", "Thương hiệu", "Tên sản phẩm", "Danh mục",
            "Người bán", "Số lượng review", "SKU", "URL"
        ]
        ws.append(headers)
        crawlflow.log(f"[TikiShop] Tao file Excel moi: {filepath}", "info")

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
            item.get("discount", 0),
            item.get("discount_rate", 0),
            item.get("stock_quantity") or item.get("stock", ""),
            item.get("availability", ""),
            _spec_value(specs, "Thương hiệu") or item.get("brand_name", ""),
            item.get("name") or item.get("product_name", ""),
            _clean_category(item.get("category", "")),
            item.get("seller_name", ""),
            item.get("review_count", ""),
            item.get("sku", ""),
            item.get("url", ""),
        ]
        ws.append(row)
        count += 1

    wb.save(filepath)
    crawlflow.log(f"[TikiShop] Da them {count} dong vao Excel", "info")
    return count


def _export_csv(products, filepath):
    """Fallback: ghi CSV."""
    import csv

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
                "STT", "Giá bìa", "Giá bán", "Giảm giá", "Phần trăm", "Tồn kho",
                "Tình trạng", "Thương hiệu", "Tên sản phẩm", "Danh mục",
                "Người bán", "Số lượng review", "SKU", "URL"
            ])

        count = existing_count
        for item in products:
            specs = item.get("specs", {})

            writer.writerow([
                count + 1,
                item.get("old_price", 0),
                item.get("price", 0),
                item.get("discount", 0),
                item.get("discount_rate", 0),
                item.get("stock_quantity") or item.get("stock", ""),
                item.get("availability", ""),
                _spec_value(specs, "Thương hiệu") or item.get("brand_name", ""),
                item.get("name") or item.get("product_name", ""),
                _clean_category(item.get("category", "")),
                item.get("seller_name", ""),
                item.get("review_count", ""),
                item.get("sku", ""),
                item.get("url", ""),
            ])
            count += 1

    crawlflow.log(f"[TikiShop] Da them {count} dong vao CSV", "info")
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
        crawlflow.log(f"[TikiShop] Loi update progress: {e}", "error")


def on_unload():
    crawlflow.log("[TikiShop] Plugin unloaded", "info")