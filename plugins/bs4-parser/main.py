"""
BeautifulSoup Parser — parse HTML using Python BeautifulSoup.

This plugin demonstrates the architecture where:
- Python (BeautifulSoup) handles HTML parsing
- Parsed data is returned as JSON
- Rust deserializes the JSON into native Rust structs for further processing

Usage in Rust:
    let parsed: Vec<ParsedHtmlItem> = serde_json::from_str(&result)?;
    // parsed[0].tag == "h1", parsed[0].text == "Title", etc.
"""

import json
from bs4 import BeautifulSoup


def on_load(config):
    crawlflow.log("BeautifulSoup HTML Parser loaded", "info")


def process_data(data_json, config_json):
    """Parse HTML items using BeautifulSoup.

    Input data is an array of objects with 'html' or 'body' fields,
    or a single object with 'html' field (from fetch_data).

    Config options:
        - selectors: list of CSS selectors to extract (default: auto-detect)
        - extract_links: bool (default: true)
        - extract_text: bool (default: true)
        - extract_tables: bool (default: true)
        - extract_meta: bool (default: true)
        - extract_headings: bool (default: true)
        - flatten: bool (default: true) — return flat item list

    Returns structured JSON that Rust deserializes into ParsedHtmlItem structs.
    """
    config = json.loads(config_json) if isinstance(config_json, str) else config_json

    # Handle both array of items and single fetch result
    items = json.loads(data_json) if isinstance(data_json, str) else data_json
    if isinstance(items, dict):
        items = [items]

    parsed_all = _parse_items(items, config)
    return json.dumps(parsed_all)


def parse_data(input_json, config_json):
    """Parse raw HTML string into structured data.

    Input is a JSON string containing:
        {"input": "<html>...</html>"}

    Returns JSON array of ParsedHtmlItem objects.
    """
    config = json.loads(config_json) if isinstance(config_json, str) else config_json
    input_data = json.loads(input_json) if isinstance(input_json, str) else input_json

    html = input_data.get("input", "")
    if not html:
        # Try the raw input field
        html = input_data if isinstance(input_data, str) else ""

    items = _parse_html(html, config)
    return json.dumps(items)


def _parse_items(items, config):
    """Parse each item containing HTML."""
    all_results = []
    for item in items:
        html = item.get("html") or item.get("body") or item.get("content", "")
        if not html:
            continue
        parsed = _parse_html(html, config)
        all_results.extend(parsed)
    return all_results


def _parse_html(html, config):
    """Core HTML parsing using BeautifulSoup."""
    soup = BeautifulSoup(html, "html.parser")
    results = []

    selectors = config.get("selectors", [])
    extract_links = config.get("extract_links", True)
    extract_text = config.get("extract_text", True)
    extract_tables = config.get("extract_tables", True)
    extract_meta = config.get("extract_meta", True)
    extract_headings = config.get("extract_headings", True)
    flatten = config.get("flatten", True)

    # Custom selectors
    if selectors:
        for sel in selectors:
            elements = soup.select(sel)
            for el in elements:
                results.append(_element_to_item(el, sel))

    # Headings (h1-h6)
    if extract_headings:
        for level in range(1, 7):
            for h in soup.find_all(f"h{level}"):
                results.append({
                    "tag": f"h{level}",
                    "text": h.get_text(strip=True),
                    "html": str(h),
                    "type": "heading",
                    "attributes": _get_attrs(h),
                })

    # All text paragraphs
    if extract_text:
        for p in soup.find_all(["p", "span", "div", "li", "td", "th"]):
            text = p.get_text(strip=True)
            if text and len(text) > 5:  # Skip short fragments
                results.append({
                    "tag": p.name,
                    "text": text,
                    "html": str(p),
                    "type": "text",
                    "attributes": _get_attrs(p),
                })

    # Links
    if extract_links:
        for a in soup.find_all("a", href=True):
            results.append({
                "tag": "a",
                "text": a.get_text(strip=True),
                "href": a["href"],
                "html": str(a),
                "type": "link",
                "attributes": _get_attrs(a),
            })

    # Images
    for img in soup.find_all("img"):
        src = img.get("src", "")
        if src:
            results.append({
                "tag": "img",
                "text": img.get("alt", ""),
                "src": src,
                "html": str(img),
                "type": "image",
                "attributes": _get_attrs(img),
            })

    # Meta tags
    if extract_meta:
        for meta in soup.find_all("meta"):
            name = meta.get("name") or meta.get("property") or ""
            content = meta.get("content", "")
            if name and content:
                results.append({
                    "tag": "meta",
                    "text": content,
                    "name": name.strip(),
                    "html": str(meta),
                    "type": "meta",
                    "attributes": _get_attrs(meta),
                })

        title_tag = soup.find("title")
        if title_tag:
            results.append({
                "tag": "title",
                "text": title_tag.get_text(strip=True),
                "html": str(title_tag),
                "type": "meta",
                "attributes": {},
            })

    # Tables
    if extract_tables:
        for table_idx, table in enumerate(soup.find_all("table")):
            rows = table.find_all("tr")
            table_data = []
            for row in rows:
                cells = row.find_all(["td", "th"])
                table_data.append([cell.get_text(strip=True) for cell in cells])
            results.append({
                "tag": "table",
                "text": "",
                "table_index": table_idx,
                "table_data": table_data,
                "html": str(table),
                "type": "table",
                "attributes": _get_attrs(table),
            })

    return results


def _element_to_item(el, selector):
    """Convert a BeautifulSoup element to a dictionary item."""
    item = {
        "tag": el.name,
        "text": el.get_text(strip=True),
        "html": str(el),
        "type": "element",
        "selector": selector,
        "attributes": _get_attrs(el),
    }

    if el.name == "a" and el.get("href"):
        item["href"] = el["href"]
        item["type"] = "link"
    elif el.name == "img":
        item["src"] = el.get("src", "")
        item["type"] = "image"

    return item


def _get_attrs(el):
    """Extract element attributes as a plain dict."""
    return {k: str(v) for k, v in el.attrs.items()}


def on_unload():
    crawlflow.log("BeautifulSoup HTML Parser unloaded", "info")
