use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;

// ─── Data types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellValue {
    String(String),
    Number(f64),
    Bool(bool),
    Empty,
}

impl CellValue {
    pub fn as_string(&self) -> String {
        match self {
            CellValue::String(s) => s.clone(),
            CellValue::Number(n) => n.to_string(),
            CellValue::Bool(b) => b.to_string(),
            CellValue::Empty => String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub cells: Vec<CellValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

impl Workbook {
    pub fn from_json_rows(data: &[serde_json::Value], sheet_name: &str) -> Self {
        let mut rows = Vec::new();
        for item in data {
            let cells = match item {
                serde_json::Value::Array(arr) => {
                    arr.iter().map(|v| cell_from_json(v)).collect()
                }
                serde_json::Value::Object(obj) => {
                    obj.values().map(|v| cell_from_json(v)).collect()
                }
                _ => vec![CellValue::String(item.to_string())],
            };
            rows.push(Row { cells });
        }
        Workbook {
            sheets: vec![Sheet {
                name: sheet_name.to_string(),
                rows,
            }],
        }
    }
}

fn cell_from_json(v: &serde_json::Value) -> CellValue {
    match v {
        serde_json::Value::String(s) => CellValue::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                CellValue::Number(f)
            } else {
                CellValue::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => CellValue::Bool(*b),
        _ => CellValue::Empty,
    }
}

// ─── Format detection ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpreadsheetFormat {
    Xlsx,
    Csv,
    Ods,
}

impl SpreadsheetFormat {
    pub fn from_extension(path: &str) -> Result<Self, String> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "xlsx" => Ok(SpreadsheetFormat::Xlsx),
            "csv" => Ok(SpreadsheetFormat::Csv),
            "ods" => Ok(SpreadsheetFormat::Ods),
            _ => Err(format!("Unsupported spreadsheet format: .{}. Supported: .xlsx, .csv, .ods", ext)),
        }
    }
}

// ─── Read ─────────────────────────────────────────────────────────────────────

pub fn read(path: &str) -> Result<Workbook, String> {
    use calamine::{open_workbook_auto, Reader};

    let mut workbook =
        open_workbook_auto(path).map_err(|e| format!("Cannot open spreadsheet: {}", e))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut sheets = Vec::new();

    for name in &sheet_names {
        let range = workbook
            .worksheet_range(name)
            .map_err(|e| format!("Cannot read sheet '{}': {}", name, e))?;

        let mut rows = Vec::new();
        for row in range.rows() {
            let cells: Vec<CellValue> = row
                .iter()
                .map(|cell| {
                    use calamine::Data;
                    match cell {
                        Data::Empty => CellValue::Empty,
                        Data::String(s) => CellValue::String(s.clone()),
                        Data::Float(f) => CellValue::Number(*f),
                        Data::Int(i) => CellValue::Number(*i as f64),
                        Data::Bool(b) => CellValue::Bool(*b),
                        Data::DateTime(dt) => CellValue::String(dt.to_string()),
                        Data::DateTimeIso(s) => CellValue::String(s.clone()),
                        Data::DurationIso(s) => CellValue::String(s.clone()),
                        Data::Error(e) => CellValue::String(format!("ERR:{}", e)),
                    }
                })
                .collect();
            rows.push(Row { cells });
        }

        sheets.push(Sheet {
            name: name.clone(),
            rows,
        });
    }

    Ok(Workbook { sheets })
}

// ─── Write dispatcher ─────────────────────────────────────────────────────────

pub fn write(workbook: &Workbook, path: &str) -> Result<(), String> {
    let format = SpreadsheetFormat::from_extension(path)?;
    match format {
        SpreadsheetFormat::Xlsx => write_xlsx(workbook, path),
        SpreadsheetFormat::Csv => write_csv(workbook, path),
        SpreadsheetFormat::Ods => write_ods(workbook, path),
    }
}

// ─── Write XLSX ───────────────────────────────────────────────────────────────

pub fn write_xlsx(workbook: &Workbook, path: &str) -> Result<(), String> {
    let bytes = to_xlsx_bytes(workbook)?;
    std::fs::write(path, &bytes).map_err(|e| format!("Cannot write XLSX: {}", e))
}

pub fn to_xlsx_bytes(workbook: &Workbook) -> Result<Vec<u8>, String> {
    use rust_xlsxwriter::*;

    let mut xl_workbook = Workbook::new();

    for sheet in &workbook.sheets {
        let xl_sheet = xl_workbook.add_worksheet();
        xl_sheet
            .set_name(&sheet.name)
            .map_err(|e| e.to_string())?;

        for (row_idx, row) in sheet.rows.iter().enumerate() {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                match cell {
                    CellValue::String(s) => {
                        xl_sheet.write_string(row_idx as u32, col_idx as u16, s)
                            .map_err(|e| e.to_string())?;
                    }
                    CellValue::Number(n) => {
                        xl_sheet.write_number(row_idx as u32, col_idx as u16, *n)
                            .map_err(|e| e.to_string())?;
                    }
                    CellValue::Bool(b) => {
                        xl_sheet.write_boolean(row_idx as u32, col_idx as u16, *b)
                            .map_err(|e| e.to_string())?;
                    }
                    CellValue::Empty => {
                        xl_sheet.write_blank(row_idx as u32, col_idx as u16, &rust_xlsxwriter::Format::default())
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    xl_workbook
        .save_to_buffer()
        .map_err(|e| format!("Cannot create XLSX: {}", e))
}

// ─── Write CSV ────────────────────────────────────────────────────────────────

pub fn write_csv(workbook: &Workbook, path: &str) -> Result<(), String> {
    let content = to_csv_string(workbook)?;
    std::fs::write(path, &content).map_err(|e| format!("Cannot write CSV: {}", e))
}

pub fn to_csv_string(workbook: &Workbook) -> Result<String, String> {
    let mut output = String::new();

    for (si, sheet) in workbook.sheets.iter().enumerate() {
        if workbook.sheets.len() > 1 {
            if si > 0 {
                output.push('\n');
            }
            output.push_str(&format!("# {}\n", sheet.name));
        }

        for row in &sheet.rows {
            let line: Vec<String> = row
                .cells
                .iter()
                .map(|c| {
                    let s = c.as_string();
                    if s.contains(',') || s.contains('"') || s.contains('\n') {
                        format!("\"{}\"", s.replace('"', "\"\""))
                    } else {
                        s
                    }
                })
                .collect();
            output.push_str(&line.join(","));
            output.push('\n');
        }
    }

    Ok(output)
}

// ─── Write ODS ────────────────────────────────────────────────────────────────

pub fn write_ods(workbook: &Workbook, path: &str) -> Result<(), String> {
    let bytes = to_ods_bytes(workbook)?;
    std::fs::write(path, &bytes).map_err(|e| format!("Cannot write ODS: {}", e))
}

pub fn to_ods_bytes(workbook: &Workbook) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    // mimetype – MUST be stored uncompressed and be the first entry
    zip.start_file::<&str, ()>(
        "mimetype",
        FileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .map_err(|e| format!("ODS zip error: {}", e))?;
    zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")
    .map_err(|e| format!("ODS zip error: {}", e))?;

    // META-INF/manifest.xml
    zip.start_file::<&str, ()>(
        "META-INF/manifest.xml",
        FileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .map_err(|e| format!("ODS zip error: {}", e))?;
    zip.write_all(ods_manifest_xml())
        .map_err(|e| format!("ODS zip error: {}", e))?;

    // content.xml
    zip.start_file::<&str, ()>(
        "content.xml",
        FileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .map_err(|e| format!("ODS zip error: {}", e))?;
    let content = build_ods_content(workbook);
    zip.write_all(content.as_bytes())
        .map_err(|e| format!("ODS zip error: {}", e))?;

    let result = zip.finish().map_err(|e| format!("ODS zip error: {}", e))?;
    Ok(result.into_inner())
}

fn ods_manifest_xml() -> &'static [u8] {
    br#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn build_ods_content(workbook: &Workbook) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
  xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
  office:version="1.2">
  <office:body>
    <office:spreadsheet>
"#);

    for sheet in &workbook.sheets {
        let name = escape_xml(&sheet.name);
        xml.push_str(&format!("      <table:table table:name=\"{}\">\n", name));

        let max_cols = sheet
            .rows
            .iter()
            .map(|r| r.cells.len())
            .max()
            .unwrap_or(0);
        if max_cols > 0 {
            xml.push_str(&format!(
                "        <table:table-column table:number-columns-repeated=\"{}\"/>\n",
                max_cols
            ));
        }

        for row in &sheet.rows {
            xml.push_str("        <table:table-row>\n");
            for cell in &row.cells {
                match cell {
                    CellValue::String(s) => {
                        let escaped = escape_xml(s);
                        xml.push_str(&format!(
                            "          <table:table-cell office:value-type=\"string\">\n            <text:p>{}</text:p>\n          </table:table-cell>\n",
                            escaped
                        ));
                    }
                    CellValue::Number(n) => {
                        xml.push_str(&format!(
                            "          <table:table-cell office:value-type=\"float\" office:value=\"{}\">\n            <text:p>{}</text:p>\n          </table:table-cell>\n",
                            n, n
                        ));
                    }
                    CellValue::Bool(b) => {
                        let v = if *b { "true" } else { "false" };
                        xml.push_str(&format!(
                            "          <table:table-cell office:value-type=\"boolean\" office:boolean-value=\"{}\">\n            <text:p>{}</text:p>\n          </table:table-cell>\n",
                            v, v
                        ));
                    }
                    CellValue::Empty => {
                        xml.push_str("          <table:table-cell/>\n");
                    }
                }
            }
            xml.push_str("        </table:table-row>\n");
        }

        xml.push_str("      </table:table>\n");
    }

    xml.push_str(
        "    </office:spreadsheet>\n  </office:body>\n</office:document-content>\n",
    );
    xml
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_workbook() -> Workbook {
        Workbook {
            sheets: vec![Sheet {
                name: "Sheet1".into(),
                rows: vec![
                    Row {
                        cells: vec![
                            CellValue::String("Name".into()),
                            CellValue::String("Age".into()),
                            CellValue::String("Active".into()),
                        ],
                    },
                    Row {
                        cells: vec![
                            CellValue::String("Alice".into()),
                            CellValue::Number(30.0),
                            CellValue::Bool(true),
                        ],
                    },
                    Row {
                        cells: vec![
                            CellValue::String("Bob".into()),
                            CellValue::Number(25.0),
                            CellValue::Bool(false),
                        ],
                    },
                ],
            }],
        }
    }

    #[test]
    fn test_csv_roundtrip() {
        let wb = sample_workbook();
        let csv = to_csv_string(&wb).unwrap();
        assert!(csv.contains("Alice"));
        assert!(csv.contains("30"));
        assert!(csv.contains("true"));
    }

    #[test]
    fn test_xlsx_bytes() {
        let wb = sample_workbook();
        let bytes = to_xlsx_bytes(&wb).unwrap();
        assert!(!bytes.is_empty());
        // XLSX magic bytes
        assert_eq!(&bytes[0..2], &[0x50, 0x4B]);
    }

    #[test]
    fn test_ods_bytes() {
        let wb = sample_workbook();
        let bytes = to_ods_bytes(&wb).unwrap();
        assert!(!bytes.is_empty());
        // ZIP magic bytes
        assert_eq!(&bytes[0..2], &[0x50, 0x4B]);
    }

    #[test]
    fn test_format_detection() {
        assert_eq!(
            SpreadsheetFormat::from_extension("data.xlsx").unwrap(),
            SpreadsheetFormat::Xlsx
        );
        assert_eq!(
            SpreadsheetFormat::from_extension("data.csv").unwrap(),
            SpreadsheetFormat::Csv
        );
        assert_eq!(
            SpreadsheetFormat::from_extension("data.ods").unwrap(),
            SpreadsheetFormat::Ods
        );
        assert!(SpreadsheetFormat::from_extension("data.txt").is_err());
    }

    #[test]
    fn test_from_json_rows() {
        let json = serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let data: Vec<serde_json::Value> = serde_json::from_value(json).unwrap();
        let wb = Workbook::from_json_rows(&data, "Test");
        assert_eq!(wb.sheets.len(), 1);
        assert_eq!(wb.sheets[0].rows.len(), 2);
    }

    #[test]
    fn test_cell_value_as_string() {
        assert_eq!(CellValue::String("hi".into()).as_string(), "hi");
        assert_eq!(CellValue::Number(42.0).as_string(), "42");
        assert_eq!(CellValue::Bool(true).as_string(), "true");
        assert_eq!(CellValue::Empty.as_string(), "");
    }

    #[test]
    fn test_csv_escaping() {
        let wb = Workbook {
            sheets: vec![Sheet {
                name: "Sheet1".into(),
                rows: vec![Row {
                    cells: vec![
                        CellValue::String("contains, comma".into()),
                        CellValue::String("has \"quotes\"".into()),
                        CellValue::String("multi\nline".into()),
                    ],
                }],
            }],
        };
        let csv = to_csv_string(&wb).unwrap();
        assert!(csv.contains("\"contains, comma\""));
        assert!(csv.contains("\"has \"\"quotes\"\"\""));
        assert!(csv.contains("\"multi\nline\""));
    }
}
