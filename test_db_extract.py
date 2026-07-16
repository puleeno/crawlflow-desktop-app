import sqlite3
import json
import sys
import re
from html import unescape

# Add plug-in to path
sys.path.insert(0, "plugins/oreka_shop_crawler")
import main

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
print("HTML length:", len(html))
print("Source URL:", source_url)

# Test extract_store_id_from_html
store_id = main._extract_store_id_from_html(html)
print("Extracted store_id from html:", store_id)

# Let's inspect parts of the html manually to see if NEXT_DATA exists
match = re.search(r'<script\b(?=[^>]*\bid=["\']__NEXT_DATA__["\'])[^>]*>(.*?)</script>', html, re.DOTALL | re.IGNORECASE)
print("NEXT_DATA tag found in DB html:", bool(match))
if match:
    try:
        content = match.group(1).strip()
        print("NEXT_DATA content length:", len(content))
        # Try loading json
        # In main.py:
        # next_data = json.loads(unescape(match.group(1)).strip())
        next_data = json.loads(unescape(content))
        print("Parsed NEXT_DATA successfully!")
        
        # In main.py:
        # apollo_state = next_data.get("props", {}).get("pageProps", {}).get("__APOLLO_STATE__", {})
        props = next_data.get("props", {})
        print("props keys:", list(props.keys()))
        pageProps = props.get("pageProps", {})
        print("pageProps keys:", list(pageProps.keys()))
        apollo_state = pageProps.get("__APOLLO_STATE__", {})
        print("apollo_state keys starting with Store:", [k for k in apollo_state.keys() if k.startswith("Store:")])
    except Exception as e:
        print("JSON parse error:", e)
else:
    # Print a snippet of the head or script tags to see what it is
    print("HTML snippet (first 1000 chars):")
    print(html[:1000])
    
    script_tags = re.findall(r'<script[^>]*>.*?</script>', html, re.DOTALL | re.IGNORECASE)
    print(f"Total script tags found: {len(script_tags)}")
    for i, tag in enumerate(script_tags[:5]):
        print(f"Tag {i}: {tag[:150]}...")
