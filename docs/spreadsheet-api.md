# Spreadsheet API — Hướng dẫn cho Plugin Developer

## Tổng quan

CrawlFlow Spreadsheet API cho phép Python plugins đọc/ghi các định dạng bảng tính phổ biến qua Rust backend — không cần cài thêm thư viện Python như `openpyxl` hay `csv`.

### Hỗ trợ

| Định dạng | Đọc | Ghi |
|---|---|---|
| **XLSX** (Excel) | ✅ | ✅ |
| **CSV** | ✅ | ✅ |
| **ODS** (LibreOffice) | ✅ | ✅ |

---

## Python API

Plugin Python truy cập qua module `crawlflow` (đã được inject sẵn):

```python
import json

# ── Đọc file ──────────────────────────────────────────────
data_json = crawlflow.spreadsheet_read("/path/to/file.xlsx")
data = json.loads(data_json)
# → {"sheets": [{"name": "Sheet1", "rows": [["col1", "col2"], ...]}]}

# ── Ghi file (auto-detect format từ extension) ────────────
crawlflow.spreadsheet_write(json.dumps(data), "/path/to/output.ods")

# ── Pipeline kết hợp: đọc → xử lý → ghi ───────────────────
def process_data(data_json, config_json):
    input_data = json.loads(data_json)

    # Đọc file Excel
    wb = json.loads(crawlflow.spreadsheet_read("input.xlsx"))
    sheet = wb["sheets"][0]

    # Xử lý dữ liệu
    headers = sheet["rows"][0]
    rows = sheet["rows"][1:]

    results = []
    for row in rows:
        item = dict(zip(headers, row))
        results.append(item)

    # Chuyển đổi và ghi ra CSV
    output_wb = {
        "sheets": [{
            "name": "Results",
            "rows": [list(results[0].keys())] + [list(r.values()) for r in results]
        }]
    }
    crawlflow.spreadsheet_write(json.dumps(output_wb), "output.csv")

    return json.dumps(results)
```

---

## JSON Workbook Format

Dữ liệu trao đổi giữa Python ↔ Rust luôn là JSON string với cấu trúc:

```json
{
  "sheets": [
    {
      "name": "Sheet1",
      "rows": [
        ["Name", "Age", "Active"],
        ["Alice", 30, true],
        ["Bob", 25, false],
        ["Empty Cell", null, ""]
      ]
    },
    {
      "name": "Sheet2",
      "rows": [
        ["Date", "Value"],
        ["2024-01-01", 100.5]
      ]
    }
  ]
}
```

### Cell types (tự động detect từ JSON type)

| JSON type | CellValue | Ghi chú |
|---|---|---|
| `string` | String | `"hello"` |
| `number` | Number | `30`, `100.5` |
| `boolean` | Bool | `true`, `false` |
| `null` | Empty | ô trống |

Khi đọc file, các kiểu dữ liệu khác (DateTime, Error) được convert thành String.

---

## Lưu ý

### CSV chỉ hỗ trợ 1 sheet

CSV không hỗ trợ nhiều sheet. Nếu Workbook có nhiều sheet, hàm ghi CSV sẽ ghi tuần tự với comment `# SheetName` làm separator:

```csv
# Sheet1
Name,Age
Alice,30

# Sheet2
Product,Price
Widget,100
```

### ODS — ghi không cần thư viện ngoài

ODS được sinh trực tiếp từ Rust (ZIP + XML), không phụ thuộc thư viện Python hay system library nào.

### XLSX — kế thừa từ `rust_xlsxwriter`

XLSX ghi qua `rust_xlsxwriter` (pure Rust) — hỗ trợ đầy đủ string, number, boolean, blank cells.

---

## Ví dụ Plugin hoàn chỉnh

**`plugins/spreadsheet-demo/main.py`**:

```python
import json

def register_presets():
    return json.dumps([
        {
            "id": "csv-to-excel",
            "name": "CSV → Excel Converter",
            "description": "Đọc CSV, ghi ra XLSX/ODS",
            "nodes": [
                {"type": "dataSource", "plugin": "spreadsheet-demo", "label": "Read CSV"},
                {"type": "processor", "plugin": "spreadsheet-demo", "label": "Transform"},
                {"type": "export", "plugin": "spreadsheet-demo", "label": "Write Excel"}
            ]
        }
    ])

def fetch_data(config_json):
    config = json.loads(config_json)
    path = config.get("path", "data.csv")

    wb = json.loads(crawlflow.spreadsheet_read(path))
    sheet = wb["sheets"][0]

    headers = sheet["rows"][0]
    rows = sheet["rows"][1:]

    result = []
    for row in rows:
        result.append(dict(zip(headers, row)))

    return json.dumps(result)

def process_data(data_json, config_json):
    data = json.loads(data_json)
    config = json.loads(config_json)

    field = config.get("field", "price")
    factor = config.get("factor", 1.1)

    for item in data:
        if field in item and isinstance(item[field], (int, float)):
            item[field] = round(item[field] * factor, 2)

    return json.dumps(data)

def export_data(data_json, config_json):
    data = json.loads(data_json)
    config = json.loads(config_json)

    output_path = config.get("output_path", "output.xlsx")

    if not data:
        headers = []
        rows = []
    else:
        headers = list(data[0].keys())
        rows = [headers] + [list(item.values()) for item in data]

    wb = json.dumps({
        "sheets": [{
            "name": config.get("sheet_name", "Data"),
            "rows": rows
        }]
    })

    crawlflow.spreadsheet_write(wb, output_path)
    return json.dumps({"success": True, "file": output_path})
```

---

## Rust API (cho built-in plugins)

### Data types

```rust
use crate::spreadsheet::{Workbook, Sheet, Row, CellValue};

// Tạo workbook programmatically
let wb = Workbook {
    sheets: vec![Sheet {
        name: "Sheet1".into(),
        rows: vec![
            Row { cells: vec![
                CellValue::String("Name".into()),
                CellValue::Number(30.0),
                CellValue::Bool(true),
            ]},
        ],
    }],
};

// Đọc file
let wb = spreadsheet::read("data.xlsx")?;

// Ghi file
spreadsheet::write(&wb, "output.ods")?;

// Export ra bytes
let xlsx_bytes = spreadsheet::to_xlsx_bytes(&wb)?;
let csv_string = spreadsheet::to_csv_string(&wb)?;
let ods_bytes = spreadsheet::to_ods_bytes(&wb)?;
```

### Convert từ JSON data (dùng chung với export cũ)

```rust
// Dùng chung với `inner_export_excel` / `inner_export_csv`
let wb = spreadsheet::Workbook::from_json_rows(&data_rows, "Sheet1");
let bytes = spreadsheet::to_xlsx_bytes(&wb)?;
```

### Tauri commands

| Command | Input | Output |
|---|---|---|
| `spreadsheet_read_cmd(path)` | path: string | JSON Workbook string |
| `spreadsheet_write_cmd(data, path)` | data: JSON string, path: string | `()` |
| `spreadsheet_export_cmd(data, config)` | data: array, config: `{format, sheetName}` | `{file_name, mime_type, content}` (base64) |

---

## Ví dụ test plugin

```python
# test_spreadsheet_plugin.py
import crawlflow
import json
import tempfile
import os

def test_read_write_xlsx():
    # Tạo workbook mẫu
    wb = {
        "sheets": [{
            "name": "Test",
            "rows": [
                ["A", "B"],
                [1, 2],
                [3, 4]
            ]
        }]
    }

    # Ghi ra file tạm
    with tempfile.NamedTemporaryFile(suffix=".xlsx", delete=False) as f:
        path = f.name

    try:
        crawlflow.spreadsheet_write(json.dumps(wb), path)

        # Đọc lại và verify
        result = json.loads(crawlflow.spreadsheet_read(path))
        assert result["sheets"][0]["rows"] == wb["sheets"][0]["rows"]
        print("✅ XLSX read/write OK")
    finally:
        os.unlink(path)

def test_read_write_csv():
    wb = {
        "sheets": [{
            "name": "Data",
            "rows": [
                ["Name", "Age"],
                ["Alice", "30"],
                ["Bob", "25"]
            ]
        }]
    }

    with tempfile.NamedTemporaryFile(suffix=".csv", delete=False) as f:
        path = f.name

    try:
        crawlflow.spreadsheet_write(json.dumps(wb), path)
        result = json.loads(crawlflow.spreadsheet_read(path))
        assert result["sheets"][0]["rows"] == wb["sheets"][0]["rows"]
        print("✅ CSV read/write OK")
    finally:
        os.unlink(path)

if __name__ == "__main__":
    test_read_write_xlsx()
    test_read_write_csv()
```
