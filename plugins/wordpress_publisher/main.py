"""
WordPress Publisher - CrawlFlow Plugin
======================================

Publish posts, pages, products, and custom post types to WordPress via REST API.

Flow:
  1. process_data()  - Map CrawlFlow fields, call WordPress API
  2. export_data()   - Output results (su dung process_data)

Config:
  wp_url: str (bat buoc)              - WordPress site URL
  wp_username: str (bat buoc)         - WordPress username
  wp_password: str (bat buoc)         - Application Password
  content_type: str (mac dinh: post)  - post/page/product/custom
  custom_post_type: str               - Custom post type slug
  post_status: str (mac dinh: publish) - publish/draft/pending/private
  update_existing: bool               - Cap nhat bai co san
  field_mappings: list                - Map field tu data sang WP
  category_source: str                - fixed/from_data/by_name
  category_ids: str                   - Category IDs
  category_data_field: str            - Field chua category
  tag_source: str                     - fixed/from_data/split
  tag_ids: str                        - Tag IDs
  tag_data_field: str                 - Field chua tag
  tag_delimiter: str                  - Ky tu phan cach tag
  featured_image_field: str           - Field chua URL anh
  batch_size: int                     - So item moi lan
  skip_on_error: bool                 - Bo qua item loi
  timeout: int                        - Timeout request
  verify_ssl: bool                    - Kiem tra SSL

Output (process_data tra ve array):
  {
    "success": true,
    "wp_id": 123,
    "wp_url": "https://example.com/?p=123",
    "edit_url": "https://example.com/wp-admin/post.php?post=123&action=edit",
    "title": "Bai viet title",
    "errors": []
  }
"""

import json
import time
import os
import base64
from datetime import datetime

try:
    from urllib.request import Request, urlopen
    from urllib.error import HTTPError, URLError
except ImportError:
    Request = None
    urlopen = None
    HTTPError = None
    URLError = None


# ── WordPress REST API Client ─────────────────────────────

class WordPressClient:
    """Ket noi toi WordPress REST API."""

    def __init__(self, config, log_fn):
        self.base_url = config.get("wp_url", "").rstrip("/")
        self.username = config.get("wp_username", "")
        self.password = config.get("wp_password", "")
        self.timeout = int(config.get("timeout", 30))
        self.verify_ssl = config.get("verify_ssl", True)
        self.log = log_fn

        # Basic Auth header (App Passwords use Basic Auth)
        auth_str = f"{self.username}:{self.password}"
        self.auth_header = f"Basic {base64.b64encode(auth_str.encode()).decode()}"

        # Xac dinh REST API endpoint
        if self.base_url:
            # Thu REST API chuan
            self.rest_base = f"{self.base_url}/wp-json/wp/v2"
            self.wc_rest_base = f"{self.base_url}/wp-json/wc/v3"
        else:
            self.rest_base = ""
            self.wc_rest_base = ""

    def _request(self, method, endpoint, data=None):
        """Gui HTTP request toi WordPress REST API."""
        if not self.rest_base:
            return {"error": "WordPress URL is not configured"}

        url = None
        if endpoint.startswith("wc/"):
            url = f"{self.wc_rest_base}/{endpoint[3:]}"
        else:
            url = f"{self.rest_base}/{endpoint.lstrip('/')}"

        body = json.dumps(data).encode("utf-8") if data is not None else None

        headers = {
            "Authorization": self.auth_header,
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "CrawlFlow-WordPress-Publisher/1.0",
        }

        try:
            req = Request(url, data=body, headers=headers, method=method)
            resp = urlopen(req, timeout=self.timeout)
            resp_body = resp.read().decode("utf-8")
            return json.loads(resp_body) if resp_body else {}
        except HTTPError as e:
            error_body = e.read().decode("utf-8", errors="replace") if e.fp else ""
            try:
                error_json = json.loads(error_body)
                messages = [err.get("message", "") for err in error_json.get("errors", [error_json])]
                return {"error": "; ".join(messages) or f"HTTP {e.code}"}
            except (json.JSONDecodeError, AttributeError):
                return {"error": f"HTTP {e.code}: {error_body[:200]}"}
        except URLError as e:
            return {"error": f"Connection failed: {e.reason}"}
        except Exception as e:
            return {"error": str(e)}

    def get(self, endpoint):
        return self._request("GET", endpoint)

    def post(self, endpoint, data):
        return self._request("POST", endpoint, data)

    def put(self, endpoint, data):
        return self._request("PUT", endpoint, data)

    def delete(self, endpoint):
        return self._request("DELETE", endpoint)

    def get_content_type_endpoint(self, content_type, custom_type=""):
        """Tra ve endpoint REST cho loai content."""
        if content_type == "post":
            return "posts"
        elif content_type == "page":
            return "pages"
        elif content_type == "product":
            return "wc/products"
        elif content_type == "custom" and custom_type:
            return custom_type
        return "posts"

    def find_by_title(self, content_type, custom_type, title):
        """Tim bai viet theo title."""
        endpoint = self.get_content_type_endpoint(content_type, custom_type)
        result = self.get(f"{endpoint}?search={self._urlencode(title)}&per_page=1")
        if isinstance(result, list) and len(result) > 0:
            return result[0].get("id")
        return None

    def _urlencode(self, s):
        """Don gian URL encode."""
        import re
        return re.sub(r'[^\w\s-]', '', s).strip().replace(' ', '%20')


# ── Default Field Mappings ─────────────────────────────────

DEFAULT_MAPPINGS = [
    {"crawlflow_field": "name", "wordpress_field": "title"},
    {"crawlflow_field": "description", "wordpress_field": "content"},
    {"crawlflow_field": "content", "wordpress_field": "content"},
    {"crawlflow_field": "excerpt", "wordpress_field": "excerpt"},
    {"crawlflow_field": "slug", "wordpress_field": "slug"},
    {"crawlflow_field": "price", "wordpress_field": "meta", "custom_meta_key": "_price"},
    {"crawlflow_field": "old_price", "wordpress_field": "meta", "custom_meta_key": "_regular_price"},
    {"crawlflow_field": "sku", "wordpress_field": "meta", "custom_meta_key": "_sku"},
    {"crawlflow_field": "image", "wordpress_field": "meta", "custom_meta_key": "_thumbnail_url"},
    {"crawlflow_field": "category_name", "wordpress_field": "meta", "custom_meta_key": "_category_name"},
]


def _normalize_field_mappings(config_mappings):
    """Ket hop config mappings voi default mappings.
    Config mappings ghi de default neu cung crawlflow_field.
    """
    if not config_mappings:
        return DEFAULT_MAPPINGS
    merged = {}
    for m in DEFAULT_MAPPINGS:
        merged[m["crawlflow_field"]] = dict(m)
    for m in config_mappings:
        merged[m["crawlflow_field"]] = {
            "crawlflow_field": m["crawlflow_field"],
            "wordpress_field": m.get("wordpress_field", "meta"),
            "custom_meta_key": m.get("custom_meta_key", ""),
        }
    return list(merged.values())


def _build_wp_payload(item, mappings, config):
    """Xay dung payload cho WordPress REST API tu CrawlFlow data item."""
    payload = {}

    # Post status
    status = config.get("post_status", "publish")
    if status == "publish":
        payload["status"] = "publish"
    elif status == "draft":
        payload["status"] = "draft"
    elif status == "pending":
        payload["status"] = "pending"
    elif status == "private":
        payload["status"] = "private"

    # Title
    if "title" in config.get("content_type", "post"):
        payload["title"] = item.get("name", item.get("title", ""))

    # Meta fields
    meta = {}
    taxonomies = {"categories": [], "tags": []}

    for mapping in mappings:
        cf_field = mapping.get("crawlflow_field", "")
        wp_field = mapping.get("wordpress_field", "")
        val = item.get(cf_field, "")

        if not val:
            continue

        if wp_field == "title":
            payload["title"] = str(val)
        elif wp_field == "content":
            payload["content"] = str(val)
        elif wp_field == "excerpt":
            payload["excerpt"] = str(val)
        elif wp_field == "slug":
            payload["slug"] = str(val)
        elif wp_field == "status":
            payload["status"] = str(val)
        elif wp_field == "meta":
            meta_key = mapping.get("custom_meta_key", cf_field)
            if meta_key:
                meta[meta_key] = val

    # Categories
    cat_source = config.get("category_source", "fixed")
    if cat_source == "fixed":
        cat_ids_str = config.get("category_ids", "")
        if cat_ids_str:
            payload["categories"] = [int(x.strip()) for x in cat_ids_str.split(",") if x.strip().isdigit()]
    elif cat_source == "from_data":
        cat_field = config.get("category_data_field", "category")
        cat_val = item.get(cat_field, "")
        if isinstance(cat_val, (int, str)):
            try:
                payload["categories"] = [int(cat_val)]
            except (ValueError, TypeError):
                pass
        elif isinstance(cat_val, list):
            payload["categories"] = [int(x) for x in cat_val if str(x).isdigit()]
    elif cat_source == "by_name":
        cat_field = config.get("category_data_field", "category")
        cat_val = item.get(cat_field, "")
        if isinstance(cat_val, str) and cat_val.strip():
            taxonomies["categories"] = [cat_val.strip()]

    # Tags
    tag_source = config.get("tag_source", "fixed")
    if tag_source == "fixed":
        tag_ids_str = config.get("tag_ids", "")
        if tag_ids_str:
            payload["tags"] = [int(x.strip()) for x in tag_ids_str.split(",") if x.strip().isdigit()]
    elif tag_source == "from_data":
        tag_field = config.get("tag_data_field", "tags")
        tag_val = item.get(tag_field, "")
        if isinstance(tag_val, list):
            taxonomies["tags"] = [str(t) for t in tag_val]
        elif isinstance(tag_val, str) and tag_val.strip():
            taxonomies["tags"] = [t.strip() for t in tag_val.split(",")]
    elif tag_source == "split":
        tag_field = config.get("tag_data_field", "tags")
        delimiter = config.get("tag_delimiter", ",")
        tag_val = item.get(tag_field, "")
        if isinstance(tag_val, str) and tag_val.strip():
            taxonomies["tags"] = [t.strip() for t in tag_val.split(delimiter)]

    # Attach meta
    if meta:
        payload["meta"] = meta

    return payload, taxonomies


def _ensure_categories(client, category_names):
    """Tu dong tao category neu chua ton tai. Tra ve list ID."""
    ids = []
    for name in category_names:
        result = client.get(f"categories?search={name}&per_page=1")
        if isinstance(result, list) and len(result) > 0:
            ids.append(result[0].get("id"))
        else:
            create_result = client.post("categories", {"name": name, "slug": name.lower().replace(" ", "-")})
            if "id" in create_result:
                ids.append(create_result["id"])
            elif "error" in create_result:
                client.log(f"[WP] Cannot create category '{name}': {create_result['error']}", "warn")
    return ids


def _ensure_tags(client, tag_names):
    """Tu dong tao tag neu chua ton tai. Tra ve list ID."""
    ids = []
    for name in tag_names:
        result = client.get(f"tags?search={name}&per_page=1")
        if isinstance(result, list) and len(result) > 0:
            ids.append(result[0].get("id"))
        else:
            create_result = client.post("tags", {"name": name})
            if "id" in create_result:
                ids.append(create_result["id"])
            elif "error" in create_result:
                client.log(f"[WP] Cannot create tag '{name}': {create_result['error']}", "warn")
    return ids


def _upload_image(client, image_url):
    """Tai anh tu URL va upload len WordPress. Tra ve attachment ID."""
    if not image_url:
        return None

    try:
        # Download image
        img_req = Request(image_url, headers={"User-Agent": "CrawlFlow/1.0"})
        img_resp = urlopen(img_req, timeout=30)
        img_data = img_resp.read()

        # Get filename from URL
        filename = image_url.split("/")[-1].split("?")[0] or "image.jpg"
        content_type = img_resp.headers.get("Content-Type", "image/jpeg")

        # Upload via wp/v2/media
        import uuid
        boundary = f"----CrawlFlowFormBoundary{uuid.uuid4().hex[:16]}"
        body = (
            f"--{boundary}\r\n"
            f"Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
            f"Content-Type: {content_type}\r\n\r\n"
        ).encode("utf-8") + img_data + f"\r\n--{boundary}--\r\n".encode("utf-8")

        headers = {
            "Authorization": client.auth_header,
            "Content-Type": f"multipart/form-data; boundary={boundary}",
            "Accept": "application/json",
            "User-Agent": "CrawlFlow/1.0",
        }

        req = Request(f"{client.rest_base}/media", data=body, headers=headers, method="POST")
        resp = urlopen(req, timeout=client.timeout)
        result = json.loads(resp.read().decode("utf-8"))
        return result.get("id")
    except Exception as e:
        client.log(f"[WP] Cannot upload image {image_url}: {e}", "warn")
        return None


def _publish_single(item, mappings, config, client):
    """Publish 1 item len WordPress. Tra ve ket qua."""
    title = item.get("name", item.get("title", item.get("url", "Untitled")))
    content_type = config.get("content_type", "post")
    custom_type = config.get("custom_post_type", "")
    update_existing = config.get("update_existing", False)
    featured_image_field = config.get("featured_image_field", "")

    client.log(f"[WP] Processing: {title[:80]}", "info")

    # Build payload
    payload, taxonomies = _build_wp_payload(item, mappings, config)
    if not payload.get("title"):
        payload["title"] = title

    # Xac dinh endpoint
    endpoint = client.get_content_type_endpoint(content_type, custom_type)

    # Tim bai co san neu can update
    existing_id = None
    if update_existing and payload.get("title"):
        existing_id = client.find_by_title(content_type, custom_type, payload["title"])

    # Tao hoac cap nhat
    if existing_id:
        client.log(f"[WP] Updating existing post ID {existing_id}", "info")
        result = client.put(f"{endpoint}/{existing_id}", payload)
    else:
        result = client.post(endpoint, payload)

    if "error" in result:
        return {
            "success": False,
            "title": title,
            "errors": [result["error"]],
        }

    wp_id = result.get("id")
    if not wp_id:
        return {
            "success": False,
            "title": title,
            "errors": ["No ID returned"],
        }

    # Xu ly taxonomies (categories by name, tags)
    all_cat_ids = list(result.get("categories", []))
    all_tag_ids = list(result.get("tags", []))

    if taxonomies.get("categories"):
        cat_ids = _ensure_categories(client, taxonomies["categories"])
        all_cat_ids.extend(cat_ids)

    if taxonomies.get("tags"):
        tag_ids = _ensure_tags(client, taxonomies["tags"])
        all_tag_ids.extend(tag_ids)

    # Cap nhat categories + tags
    if all_cat_ids or all_tag_ids:
        update_payload = {}
        if all_cat_ids:
            update_payload["categories"] = list(set(all_cat_ids))
        if all_tag_ids:
            update_payload["tags"] = list(set(all_tag_ids))

        if update_payload:
            if existing_id:
                client.put(f"{endpoint}/{existing_id}", update_payload)
            else:
                client.put(f"{endpoint}/{wp_id}", update_payload)

    # Featured image
    if featured_image_field:
        image_url = item.get(featured_image_field, "")
        if image_url:
            attachment_id = _upload_image(client, image_url)
            if attachment_id:
                client.put(f"{endpoint}/{wp_id}", {"featured_media": attachment_id})

    site_url = config.get("wp_url", "").rstrip("/")
    return {
        "success": True,
        "wp_id": wp_id,
        "wp_url": f"{site_url}/?p={wp_id}" if content_type in ("post",) else result.get("link", f"{site_url}/?p={wp_id}"),
        "edit_url": f"{site_url}/wp-admin/post.php?post={wp_id}&action=edit",
        "title": title,
        "errors": [],
    }


# ── Hook Functions ─────────────────────────────────────────

def on_load(config):
    crawlflow.log("[WP Publisher] Plugin loaded", "info")
    try:
        from urllib.request import Request, urlopen
        crawlflow.log("[WP Publisher] urllib available", "info")
    except ImportError:
        crawlflow.log("[WP Publisher] urllib not available!", "error")


def process_data(data_json, config_json):
    """Publish data len WordPress.

    Input: JSON array of items (tu CrawlFlow pipeline)
    Output: JSON array of results
    """
    data = json.loads(data_json) if isinstance(data_json, str) else data_json
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    if not isinstance(data, list):
        data = [data]

    started_at = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
    total = len(data)
    crawlflow.log(f"[WP Publisher] Bat dau publish {total} items", "info")

    # Validate config
    wp_url = config.get("wp_url", "")
    wp_username = config.get("wp_username", "")
    wp_password = config.get("wp_password", "")

    if not wp_url or not wp_username or not wp_password:
        return json.dumps({
            "success": False,
            "results": [],
            "total": total,
            "published": 0,
            "errors": ["Missing WordPress connection config"],
        })

    # Init client
    client = WordPressClient(config, lambda msg, level="info": crawlflow.log(f"[WP] {msg}", level))

    # Test connection
    test = client.get("")
    if "error" in test:
        return json.dumps({
            "success": False,
            "results": [],
            "total": total,
            "published": 0,
            "errors": [f"WordPress connection failed: {test['error']}"],
        })

    crawlflow.log("[WP Publisher] WordPress connection OK", "info")

    # Build field mappings
    config_mappings = config.get("field_mappings", [])
    if isinstance(config_mappings, str):
        try:
            config_mappings = json.loads(config_mappings)
        except (json.JSONDecodeError, TypeError):
            config_mappings = []
    mappings = _normalize_field_mappings(config_mappings)

    # Process items
    results = []
    published = 0
    failed = 0
    skip_on_error = config.get("skip_on_error", True)
    batch_size = int(config.get("batch_size", 5))

    for i, item in enumerate(data):
        result = _publish_single(item, mappings, config, client)
        results.append(result)

        if result.get("success"):
            published += 1
            crawlflow.log(f"[WP Publisher] Published: {result['title'][:60]} (ID: {result['wp_id']})", "info")
        else:
            failed += 1
            for err in result.get("errors", []):
                crawlflow.log(f"[WP Publisher] Failed: {err}", "error")

            if not skip_on_error:
                crawlflow.log("[WP Publisher] Stopping due to error (skip_on_error=False)", "warn")
                break

        # Cap nhat progress
        pct = ((i + 1) / total) * 100
        crawlflow.update_progress(config.get("project_id", "default"), json.dumps({
            "items_total": total,
            "items_processed": i + 1,
            "items_success": published,
            "items_failed": failed,
            "progress_pct": round(pct, 1),
            "message": f"Published {published}/{total}",
        }))

        # Delay giua cac batch de tranh rate limit
        if (i + 1) % batch_size == 0 and i + 1 < total:
            delay = 1
            crawlflow.log(f"[WP Publisher] Batch pause {delay}s...", "info")
            time.sleep(delay)

    elapsed = (datetime.now() - datetime.strptime(started_at, "%Y-%m-%dT%H:%M:%S")).total_seconds()
    crawlflow.log(f"[WP Publisher] Hoan thanh: {published} published, {failed} failed in {elapsed:.1f}s", "info")

    return json.dumps({
        "success": failed == 0,
        "results": results,
        "total": total,
        "published": published,
        "failed": failed,
        "elapsed_seconds": round(elapsed, 1),
    })


def export_data(data_json, config_json):
    """Alias cho process_data (export capability)."""
    return process_data(data_json, config_json)


def register_presets():
    """Dang ky presets cho WordPress Publisher."""
    return json.dumps([
        {
            "id": "wordpress-publish-posts",
            "name": "WordPress - Publish Posts",
            "description": "Publish posts to WordPress with field mapping",
            "category": "Export",
            "nodes": [
                {
                    "id": "source",
                    "type": "dataSource",
                    "data": {
                        "sourceType": "url",
                        "sourceValue": "",
                    },
                    "position": {"x": 50, "y": 200},
                },
                {
                    "id": "processor",
                    "type": "processor",
                    "data": {
                        "pluginType": "wordpress-publisher",
                        "processorType": "py-wordpress-publisher",
                        "processorConfig": {
                            "content_type": "post",
                            "post_status": "publish",
                        },
                    },
                    "position": {"x": 350, "y": 200},
                },
            ],
            "edges": [
                {"id": "e1", "source": "source", "target": "processor"},
            ],
        },
        {
            "id": "wordpress-publish-products",
            "name": "WordPress - Publish Products (WooCommerce)",
            "description": "Publish WooCommerce products to WordPress",
            "category": "Export",
            "nodes": [
                {
                    "id": "source",
                    "type": "dataSource",
                    "data": {
                        "sourceType": "url",
                        "sourceValue": "",
                    },
                    "position": {"x": 50, "y": 200},
                },
                {
                    "id": "processor",
                    "type": "processor",
                    "data": {
                        "pluginType": "wordpress-publisher",
                        "processorType": "py-wordpress-publisher",
                        "processorConfig": {
                            "content_type": "product",
                            "post_status": "publish",
                        },
                    },
                    "position": {"x": 350, "y": 200},
                },
            ],
            "edges": [
                {"id": "e1", "source": "source", "target": "processor"},
            ],
        },
    ])
