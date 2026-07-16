import sqlite3
import json
import sys
import re

# Add plugin to path
sys.path.insert(0, "plugins/oreka_shop_crawler")
import main

# Mock the crawlflow module
class MockCrawlflow:
    def log(self, message, level="info"):
        print(f"[LOG {level}] {message}")
        
    def fetch_url(self, url, headers=None):
        print(f"[FETCH_URL] fetching: {url} with headers {headers}")
        # Fetch it using urllib.request like our test_store_id.py does
        import urllib.request
        req = urllib.request.Request(
            url,
            headers=headers or {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"}
        )
        try:
            with urllib.request.urlopen(req) as resp:
                status = resp.status
                body = resp.read().decode("utf-8")
                return json.dumps({
                    "status": status,
                    "body": body,
                    "url": url
                })
        except Exception as e:
            return json.dumps({
                "status": 500,
                "body": str(e),
                "url": url
            })

main.crawlflow = MockCrawlflow()

db_path = "/Users/puleeno/Library/Application Support/com.CrawlFlow.desktop/project_897ef925-caac-418f-8064-c1c2cff752f2.db"
conn = sqlite3.connect(db_path)
cursor = conn.cursor()

# Get the raw item
cursor.execute("SELECT raw_content, source_url FROM raw_items WHERE item_type = 'raw'")
row = cursor.fetchone()
if not row:
    print("No raw item found in project database!")
    sys.exit(1)

html, source_url = row

# Construct data_json
data = {
    "raw_data": html,
    "source_url": source_url,
    "config": {
        "input_type": "html"
    }
}
data_json = json.dumps(data)

# Call preprocess_data
print("Calling main.preprocess_data...")
result_str = main.preprocess_data(data_json)
result = json.loads(result_str)

print()
print("Result count:", len(result))
if result:
    print("First item:", json.dumps(result[0], indent=2))
