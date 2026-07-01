"""
JSON Transformer — a Python plugin for CrawlFlow.

This plugin demonstrates how Python acts as the processing layer:
- Rust provides low-level APIs (HTTP, HTML parsing, file I/O) via the `crawlflow` module.
- Python implements business logic (transform, filter, CSV export).
- The frontend (JS/TS) only renders UI and calls Tauri commands.

Available `crawlflow` API functions:
    crawlflow.fetch_url(url, headers=None) -> str (JSON)
    crawlflow.extract_html(html, rules_json) -> str (JSON)
    crawlflow.fetch_rss(url, max_items=50) -> str (JSON)
    crawlflow.export_csv(data_json, delimiter=",") -> str (CSV)
    crawlflow.parse_html_table(html, table_index=0, has_header=True) -> str (JSON)
    crawlflow.save_file(path, content) -> bool
    crawlflow.read_file(path) -> str
    crawlflow.log(message, level="info")
"""

import json


def on_load(config):
    crawlflow.log("JSON Transformer loaded", "info")


def process_data(data_json, config_json):
    """Processor: transform/remap JSON data based on config rules."""
    data = json.loads(data_json)
    config = json.loads(config_json)

    operation = config.get("operation", "passthrough")
    result = []

    for item in data:
        if operation == "passthrough":
            result.append(item)
        elif operation == "select_fields":
            fields = config.get("fields", [])
            if fields:
                result.append({k: item.get(k) for k in fields if k in item})
            else:
                result.append(item)
        elif operation == "rename_fields":
            mapping = config.get("mapping", {})
            new_item = {}
            for old_key, new_key in mapping.items():
                if old_key in item:
                    new_item[new_key] = item[old_key]
            for k, v in item.items():
                if k not in mapping:
                    new_item[k] = v
            result.append(new_item)
        elif operation == "add_field":
            field_name = config.get("field_name", "new_field")
            field_value = config.get("field_value", "")
            item[field_name] = field_value
            result.append(item)
        elif operation == "filter":
            field = config.get("field", "")
            op = config.get("operator", "equals")
            value = config.get("value", "")
            match = False
            val = item.get(field, "")
            if op == "equals":
                match = str(val) == str(value)
            elif op == "contains":
                match = str(value) in str(val)
            elif op == "greater_than":
                match = float(val) > float(value)
            elif op == "less_than":
                match = float(val) < float(value)
            if match:
                result.append(item)
        else:
            result.append(item)

    return json.dumps(result)


def fetch_data(config_json):
    """DataSource: fetch JSON data from an API or URL."""
    config = json.loads(config_json)
    source_type = config.get("source_type", "url")

    if source_type == "url":
        url = config.get("url", "")
        headers = config.get("headers", None)
        if headers:
            h_list = [(k, v) for k, v in headers.items()]
        else:
            h_list = None
        result_json = crawlflow.fetch_url(url, h_list)
        result = json.loads(result_json)
        if result.get("status") == 200:
            body = result.get("body", "[]")
            try:
                parsed = json.loads(body)
                return json.dumps(parsed if isinstance(parsed, list) else [parsed])
            except json.JSONDecodeError:
                return json.dumps([{"raw": body}])
        return json.dumps([{"error": result.get("error", "Unknown")}])

    elif source_type == "rss":
        url = config.get("url", "")
        max_items = config.get("max_items", 50)
        result_json = crawlflow.fetch_rss(url, max_items)
        return result_json

    return json.dumps([])


def export_data(data_json, config_json):
    """Export: convert data to CSV or other format."""
    data = json.loads(data_json)
    config = json.loads(config_json)
    fmt = config.get("format", "csv")

    if fmt == "csv":
        delimiter = config.get("delimiter", ",")
        return crawlflow.export_csv(json.dumps(data), delimiter)

    elif fmt == "json":
        indent = config.get("pretty", True)
        return json.dumps(data, indent=2 if indent else None)

    return json.dumps(data)


def on_unload():
    crawlflow.log("JSON Transformer unloaded", "info")
