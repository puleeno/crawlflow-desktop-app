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
    "crawled_at": "2026-07-02T12:00:00"
  }
"""

import json
import time
import os
import re
from datetime import datetime
from html.parser import HTMLParser


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


def _extract_total_pages(html):
    """Uoc luong tong so trang tu phan tu phan trang."""
    numbers = re.findall(r'<a[^>]*>(?:<span[^>]*>)?(\d+)(?:</span>)?</a>', html)
    if numbers:
        return max(int(n) for n in numbers if n.isdigit())
    # Kiem tra text "Page X of Y"
    m = re.search(r'(?:page|trang|cua)\s*(\d+)\s*(?:of|trên|cua)\s*(\d+)', html, re.IGNORECASE)
    if m:
        return int(m.group(2))
    # Mặc dinh 20 pages
    return 20


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

    # Khoi tao progress
    started_at = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")

    all_products = []
    product_urls = set()
    total_pages_estimated = max_pages

    crawlflow.log(f"[OrekaShop] Bat dau crawl shop: {shop_url}", "info")

    for page in range(1, max_pages + 1):
        page_url = f"{shop_url.rstrip('?')}?{selectors['page_param']}={page}" if page > 1 else shop_url

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

        # Uoc luong tong so trang tu page 1
        if page == 1:
            estimated = _extract_total_pages(html)
            if estimated > 0:
                total_pages_estimated = min(estimated, max_pages)
                crawlflow.log(f"[OrekaShop] Uoc tinh {total_pages_estimated} trang", "info")

        # Trich xuat link san pham
        links = _extract_listing_links(html, base)
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

        crawlflow.log(f"[OrekaShop] Trang {page}: tim thay {new_links} san pham moi (tong: {len(product_urls)})", "info")

        # Kiem tra neu khong co link moi => het
        if new_links == 0 and page > 1:
            crawlflow.log(f"[OrekaShop] Khong tim thay san pham moi o trang {page}, dung phan trang", "info")
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
    """Xuat du lieu ra Excel voi co che append + check trung.

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
        # Header row
        headers = [
            "STT", "URL", "Ten san pham", "Gia (VND)", "Gia cu (VND)",
            "Hinh anh", "SKU", "Mo ta", "Danh muc", "Tinh trang",
            "Thong so", "Thoi gian crawl"
        ]
        ws.append(headers)
        crawlflow.log(f"[OrekaShop] Tao file Excel moi: {filepath}", "info")

    count = 0
    spec_limit = 5  # Chi ghi 5 thong so dau tien de tranh qua rong

    for item in products:
        dedup_key = item.get("url", "") or item.get("sku", "")
        if dedup_key and dedup_key in seen_ids:
            continue

        specs = item.get("specs", {})
        specs_str = "; ".join([f"{k}: {v}" for k, v in list(specs.items())[:spec_limit]])
        if len(specs) > spec_limit:
            specs_str += f" (va {len(specs) - spec_limit} thong so khac)"

        row = [
            ws.max_row,
            item.get("url", ""),
            item.get("name", ""),
            item.get("price", 0),
            item.get("old_price", 0),
            item.get("image", ""),
            item.get("sku", ""),
            item.get("description", ""),
            item.get("category", ""),
            item.get("availability", ""),
            specs_str[:500] if specs_str else "",
            item.get("crawled_at", ""),
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

    with open(filepath, mode, newline="", encoding="utf-8-sig") as f:
        writer = csv.writer(f)
        if has_header:
            writer.writerow([
                "URL", "Ten san pham", "Gia (VND)", "Gia cu (VND)",
                "Hinh anh", "SKU", "Mo ta", "Danh muc", "Tinh trang",
                "Thong so", "Thoi gian crawl"
            ])

        count = 0
        for item in products:
            specs = item.get("specs", {})
            specs_str = "; ".join([f"{k}: {v}" for k, v in list(specs.items())[:5]])

            writer.writerow([
                item.get("url", ""),
                item.get("name", ""),
                item.get("price", 0),
                item.get("old_price", 0),
                item.get("image", ""),
                item.get("sku", ""),
                item.get("description", ""),
                item.get("category", ""),
                item.get("availability", ""),
                specs_str[:500],
                item.get("crawled_at", ""),
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


def register_presets():
    return json.dumps([
        {
            "id": "oreka-shop-crawler",
            "name": "Oreka Shop Crawler",
            "description": "Crawl toan bo san pham tu mot shop tren oreka.vn, parse du lieu va xuat Excel. Ho tro append + dedup + realtime progress.",
            "icon": "Store",
            "icon_color": "#06b6d4",
            "project_settings": {
                "name": "Oreka Shop - {shop}",
                "description": "Crawl san pham tu shop oreka.vn",
                "intervalSeconds": 3600,
                "crawlDelay": 1500,
                "userAgent": "CrawlFlow/1.0",
                "concurrency": 1,
            },
            "nodes": [
                {
                    "id": "ds-oreka",
                    "type": "start",
                    "label": "Oreka Shop",
                    "position": {"x": 50, "y": 200},
                    "data": {
                        "sourceType": "plugin",
                        "pluginId": "oreka-shop-crawler",
                        "sourceValue": "",
                        "apiSettings": {
                            "authType": "none",
                            "authDetails": {},
                            "paginationType": "auto",
                            "paginationDetails": {}
                        }
                    }
                },
                {
                    "id": "proc-oreka",
                    "type": "processor",
                    "label": "Chuan hoa du lieu",
                    "position": {"x": 350, "y": 200},
                    "data": {
                        "processorType": "py-oreka-shop-crawler",
                        "processorConfig": {}
                    }
                },
                {
                    "id": "exp-oreka",
                    "type": "csvExport",
                    "label": "Xuat Excel",
                    "position": {"x": 650, "y": 200},
                    "data": {
                        "format": "xlsx",
                        "outputField": "file",
                        "hasHeader": True,
                    }
                }
            ],
            "edges": [
                {"id": "e-ds-proc", "source": "ds-oreka", "target": "proc-oreka", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-proc-exp", "source": "proc-oreka", "target": "exp-oreka", "sourceHandle": "data-out", "targetHandle": "data-in"},
            ]
        }
    ])


def on_unload():
    crawlflow.log("[OrekaShop] Plugin unloaded", "info")
