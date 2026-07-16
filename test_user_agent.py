import urllib.request

url = "https://www.oreka.vn/mua-ban?storeId=950fc091-0766-4b5e-af6a-ad7b5325a1fb&sort=createdAt&order=desc"
req = urllib.request.Request(
    url,
    headers={"User-Agent": "CrawlFlow/1.0"}
)
try:
    with urllib.request.urlopen(req) as resp:
        print("Status with CrawlFlow/1.0:", resp.status)
except Exception as e:
    print("Failed with CrawlFlow/1.0:", e)

# Test with a real browser UA
real_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
req2 = urllib.request.Request(
    url,
    headers={"User-Agent": real_ua}
)
try:
    with urllib.request.urlopen(req2) as resp:
        print("Status with Browser UA:", resp.status)
except Exception as e:
    print("Failed with Browser UA:", e)
