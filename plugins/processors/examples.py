"""Vi du minh hoa cach dinh nghia processor settings voi CrawlFlow Settings Framework.

Moi processor dinh nghia settings rieng bang:
  1. Ke thua SettingsSchema
  2. Khai bao field trong list `settings`
  3. Goi ProcessorEngine de lay schema JSON cho frontend

Cac processor co the co settings hoan toan khac nhau.
"""

from plugins.settings import Field, SettingsSchema


class OrekaShopSettings(SettingsSchema):
    """Settings cho processor crawl Oreka.vn."""
    settings = [
        Field("shop_url", "Shop URL", type="string", required=True,
              description="URL cua shop tren oreka.vn",
              placeholder="https://oreka.vn/shop/ten-shop",
              order=1),
        Field("max_pages", "So trang toi da", type="number",
              default=50, minimum=1, maximum=1000,
              description="Gioi han so trang can crawl",
              order=2),
        Field("delay_ms", "Delay (ms)", type="number",
              default=1500, minimum=0, maximum=60000,
              description="Delay giua cac request (ms)",
              unit="ms", order=3),
        Field("extract_images", "Lay hinh anh", type="boolean",
              default=False,
              description="Tai va luu anh san pham",
              order=4),
        Field("output_format", "Dinh dang xuat", type="select",
              options=[{"value": "xlsx", "label": "Excel (.xlsx)"},
                       {"value": "csv", "label": "CSV (.csv)"}],
              default="xlsx",
              description="Dinh dang file xuat",
              order=5),
    ]


class RSSMonitorSettings(SettingsSchema):
    """Settings cho processor monitor RSS feeds."""
    settings = [
        Field("feed_url", "Feed URL", type="string", required=True,
              description="URL cua RSS/Atom feed",
              placeholder="https://example.com/feed.xml",
              order=1),
        Field("check_interval", "Kiem tra moi (phut)", type="number",
              default=15, minimum=5, maximum=1440,
              description="Tan suat kiem tra feed moi",
              unit="phut", order=2),
        Field("max_items", "So luong toi da", type="number",
              default=50, minimum=1, maximum=500,
              description="So luong item toi da moi lan crawl",
              order=3),
        Field("include_content", "Bao gom noi dung", type="boolean",
              default=False,
              description="Lay ca noi dung bai viet",
              order=4),
        Field("filter_keywords", "Tu khoa loc", type="multi_select",
              options=[{"value": "technology", "label": "Cong nghe"},
                       {"value": "business", "label": "Kinh doanh"},
                       {"value": "science", "label": "Khoa hoc"}],
              description="Chi lay tin co tu khoa nay",
              order=5),
    ]


class DatabaseExportSettings(SettingsSchema):
    """Settings cho processor xuat du lieu vao database."""
    settings = [
        Field("db_type", "Loai database", type="select", required=True,
              options=[{"value": "mysql", "label": "MySQL"},
                       {"value": "postgres", "label": "PostgreSQL"},
                       {"value": "sqlite", "label": "SQLite"}],
              order=1),
        Field("host", "Host", type="string",
              default="localhost",
              placeholder="localhost",
              description="Database host",
              conditions=[{"field": "db_type", "operator": "neq", "value": "sqlite"}],
              order=2),
        Field("port", "Port", type="number",
              default=3306, minimum=1, maximum=65535,
              conditions=[{"field": "db_type", "operator": "neq", "value": "sqlite"}],
              order=3),
        Field("credentials", "Thong tin dang nhap", type="group",
              description="Tai khoan ket noi database",
              fields=[
                  Field("username", "Username", type="string", required=True),
                  Field("password", "Password", type="secret", required=True),
              ],
              order=4),
        Field("table_name", "Ten bang", type="string", required=True,
              description="Bang dich de ghi du lieu",
              order=5),
        Field("batch_size", "Batch size", type="number",
              default=100, minimum=1, maximum=10000,
              description="So dong ghi moi lan",
              order=6),
    ]


if __name__ == "__main__":
    import json

    print("=== OrekaShop Settings ===")
    print(json.dumps(OrekaShopSettings.get_schema(), indent=2, ensure_ascii=False))
    print("\nDefaults:", json.dumps(OrekaShopSettings.get_defaults(), ensure_ascii=False))
    print("\nValidate:", OrekaShopSettings.validate({"shop_url": ""}))

    print("\n\n=== RSSMonitor Settings ===")
    print(json.dumps(RSSMonitorSettings.get_schema(), indent=2, ensure_ascii=False))

    print("\n\n=== DatabaseExport Settings ===")
    print(json.dumps(DatabaseExportSettings.get_schema(), indent=2, ensure_ascii=False))
