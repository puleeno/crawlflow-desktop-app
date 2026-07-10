import urllib.request
import re
import json

url = "https://www.oreka.vn/store/C21AVGZS44L3UU"
req = urllib.request.Request(
    url, 
    headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'}
)
try:
    with urllib.request.urlopen(req) as response:
        html = response.read().decode('utf-8')
    print("HTML Length:", len(html))
    
    # Check if __NEXT_DATA__ exists
    match = re.search(r'<script\s+id=["\']__NEXT_DATA__["\']\s+type=["\']application/json["\'][^>]*>(.*?)</script>', html, re.DOTALL)
    if match:
        print("Found __NEXT_DATA__ tag!")
        js_data = json.loads(match.group(1))
        # Save a subset or search for store
        def search_dict(d, path=""):
            if isinstance(d, dict):
                for k, v in d.items():
                    current_path = f"{path}.{k}" if path else k
                    if k.lower() in ("storeid", "store"):
                        print(f"Match path {current_path}: {v}")
                    search_dict(v, current_path)
            elif isinstance(d, list):
                for i, v in enumerate(d):
                    search_dict(v, f"{path}[{i}]")
        search_dict(js_data)
    else:
        print("NOT found __NEXT_DATA__ tag!")
        # Print some script tags
        script_tags = re.findall(r'<script[^>]*>.*?</script>', html, re.DOTALL)
        print(f"Found {len(script_tags)} script tags")
        for st in script_tags[:5]:
            print(st[:100])
except Exception as e:
    print("Error:", e)
