import sqlite3

db_path = "/Users/puleeno/Library/Application Support/com.CrawlFlow.desktop/crawlflow.db"
conn = sqlite3.connect(db_path)
cursor = conn.cursor()

# Query log messages
cursor.execute("SELECT level, source, message, timestamp FROM logs ORDER BY id DESC LIMIT 150")
rows = cursor.fetchall()
print(f"Total logs: {len(rows)}")
for r in reversed(rows):
    lvl, sender, msg, date = r
    # Only print relevant logs to keep output clean
    if any(k in msg for k in ["Oreka", "PythonPlugin", "failed", "error", "warn"]) or sender in ["preprocessing", "fetching"]:
        print(f"[{date}] [{lvl}] [{sender}] {msg}")
