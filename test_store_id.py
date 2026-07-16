import urllib.request
import re
import json
from html import unescape

url = "https://www.oreka.vn/store/C21AVGZS44L3UU"
req = urllib.request.Request(
    url,
    headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"},
)
with urllib.request.urlopen(req) as response:
    html_raw = response.read().decode("utf-8")

print("HTML Length:", len(html_raw))

# ─── Test 1: regex trước khi unescape ───────────────────────────────
script_regex = r'<script\s+id=["\']__NEXT_DATA__["\'][^>]*>(.*?)</script>'
m = re.search(script_regex, html_raw, re.DOTALL | re.IGNORECASE)
print("\n[Test 1] __NEXT_DATA__ found (raw html):", bool(m))
if m:
    content = m.group(1).strip()
    print("  Content length:", len(content))
    try:
        data = json.loads(content)
        apollo = data.get("props", {}).get("pageProps", {}).get("__APOLLO_STATE__", {})
        print("  __APOLLO_STATE__ type:", type(apollo).__name__)
        if isinstance(apollo, dict):
            all_keys = list(apollo.keys())
            print("  Total Apollo keys:", len(all_keys))
            store_keys = [k for k in all_keys if k.startswith("Store:")]
            print("  Store: keys found:", store_keys)
        else:
            print("  __APOLLO_STATE__ not a dict!")
    except json.JSONDecodeError as e:
        print("  JSON parse ERROR:", e)
        # Show context around error
        pos = e.pos
        print("  Context:", repr(content[max(0, pos-30):pos+30]))

# ─── Test 2: regex sau khi unescape ─────────────────────────────────
html_unescaped = unescape(html_raw)
m2 = re.search(script_regex, html_unescaped, re.DOTALL | re.IGNORECASE)
print("\n[Test 2] __NEXT_DATA__ found (after unescape):", bool(m2))
if m2:
    content2 = m2.group(1).strip()
    try:
        data2 = json.loads(content2)
        apollo2 = data2.get("props", {}).get("pageProps", {}).get("__APOLLO_STATE__", {})
        print("  __APOLLO_STATE__ type:", type(apollo2).__name__)
        store_keys2 = [k for k in apollo2.keys() if k.startswith("Store:")] if isinstance(apollo2, dict) else []
        print("  Store: keys:", store_keys2)
    except json.JSONDecodeError as e:
        print("  JSON parse ERROR after unescape:", e)

# ─── Test 3: double unescape (what current code does) ───────────────
if m2:
    content3 = m2.group(1).strip()
    try:
        data3 = json.loads(unescape(content3))
        print("\n[Test 3] Double unescape parse: OK")
    except json.JSONDecodeError as e:
        print("\n[Test 3] Double unescape JSON ERROR:", e)
        pos = e.pos
        print("  Context:", repr(content3[max(0, pos-40):pos+40]))

# ─── Test 4: og:image fallback ──────────────────────────────────────
meta_match = re.search(r"store-([a-fA-F0-9\-]{36})\.", html_raw)
print("\n[Test 4] og:image fallback:", meta_match.group(1) if meta_match else "NOT FOUND")

# ─── Test 5: Apollo UUID regex ──────────────────────────────────────
uuid_match = re.search(r'"Store:([a-fA-F0-9\-]{36})"', html_raw)
print("[Test 5] Apollo UUID regex:", uuid_match.group(1) if uuid_match else "NOT FOUND")

# ─── Test 6: current lookahead regex (from plugin code) ─────────────
lookahead_regex = r'<script\b(?=[^>]*\bid=["\']__NEXT_DATA__["\'])[^>]*>(.*?)</script>'
m6 = re.search(lookahead_regex, html_raw, re.DOTALL | re.IGNORECASE)
print("[Test 6] Current lookahead regex:", "FOUND" if m6 else "NOT FOUND")
if m6:
    try:
        d6 = json.loads(unescape(m6.group(1)).strip())
        a6 = d6.get("props", {}).get("pageProps", {}).get("__APOLLO_STATE__", {})
        s6 = [k for k in a6.keys() if k.startswith("Store:")] if isinstance(a6, dict) else []
        print("  Store keys:", s6)
    except json.JSONDecodeError as e:
        print("  JSON parse ERROR:", e)
