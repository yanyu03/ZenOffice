use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits, ReadError};
use zen_opc::ZipWriter;

fn create_test_xlsx(workbook_xml: &str) -> Vec<u8> {
    let mut w = ZipWriter::new();
    let ct = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#;
    w.add("[Content_Types].xml", ct.as_bytes().to_vec());

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;
    w.add("_rels/.rels", root_rels.as_bytes().to_vec());

    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#;
    w.add("xl/_rels/workbook.xml.rels", wb_rels.as_bytes().to_vec());

    w.add("xl/workbook.xml", workbook_xml.as_bytes().to_vec());

    let sheet_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>123</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    w.add("xl/worksheets/sheet1.xml", sheet_xml.as_bytes().to_vec());

    w.finish().unwrap()
}

#[test]
fn test_mce_must_understand_unsupported_rejected() {
    // Red-team test case: mc:MustUnderstand="x15" on root element
    let wb = r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
          xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
          xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main"
          mc:MustUnderstand="x15">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#;

    let bytes = create_test_xlsx(wb);
    let res = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default());
    assert!(res.is_err(), "Expected MustUnderstand='x15' to be rejected");
    match res {
        Err(ReadError::Unsupported(msg)) => {
            assert!(msg.contains("MustUnderstand") && msg.contains("x15"),
                "Expected error message mentioning MustUnderstand and x15, got: {msg}");
        }
        other => panic!("Expected ReadError::Unsupported, got: {:?}", other),
    }
}

#[test]
fn test_mce_must_understand_understood_accepted() {
    let wb = r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
          xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
          xmlns:sml="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          mc:MustUnderstand="sml">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#;

    let bytes = create_test_xlsx(wb);
    let res = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default());
    assert!(res.is_ok(), "Understood MustUnderstand='sml' must be accepted: {:?}", res.err());
}

#[test]
fn test_mce_alternate_content_fallback_selection() {
    let wb = r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
          xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
          xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main">
  <sheets>
    <mc:AlternateContent>
      <mc:Choice Requires="x15">
        <sheet name="ShouldNotBeSelected" sheetId="1" r:id="rId1"/>
      </mc:Choice>
      <mc:Fallback>
        <sheet name="FallbackSheet" sheetId="1" r:id="rId1"/>
      </mc:Fallback>
    </mc:AlternateContent>
  </sheets>
</workbook>"#;

    let bytes = create_test_xlsx(wb);
    let wb_view = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default()).expect("read succeeded");
    let sheets = wb_view.workbook().sheets();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].name(), "FallbackSheet");
}

#[test]
fn test_mce_unselected_choice_must_understand_ignored() {
    // ECMA-376 Part 3: unselected Choice blocks are removed before processing,
    // so MustUnderstand inside unselected Choice MUST NOT cause rejection.
    let wb = r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
          xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
          xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main">
  <sheets>
    <mc:AlternateContent>
      <mc:Choice Requires="x15" mc:MustUnderstand="x15">
        <sheet name="Unselected" sheetId="1" r:id="rId1"/>
      </mc:Choice>
      <mc:Fallback>
        <sheet name="ActiveSheet" sheetId="1" r:id="rId1"/>
      </mc:Fallback>
    </mc:AlternateContent>
  </sheets>
</workbook>"#;

    let bytes = create_test_xlsx(wb);
    let wb_view = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default()).expect("read succeeded");
    let sheets = wb_view.workbook().sheets();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].name(), "ActiveSheet");
}

#[test]
fn test_mce_red_team_pinned_xlsx_must_understand_injection_rejected() {
    let src_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx");
    assert!(src_path.is_file(), "required project-authored XLSX fixture missing");
    let bytes = std::fs::read(src_path).expect("read pinned xlsx");
    let archive = zen_opc::ZipArchive::read(&bytes).expect("read zip");
    let mut w = ZipWriter::new();
    for e in &archive.entries {
        if e.name == "xl/workbook.xml" {
            let xml_str = std::str::from_utf8(&e.data).expect("utf8");
            let modified = xml_str.replace("<workbook ", "<workbook mc:MustUnderstand=\"x15\" ");
            w.add(&e.name, modified.into_bytes());
        } else {
            w.add(&e.name, e.data.clone());
        }
    }
    let modified_bytes = w.finish().expect("finish zip");
    let res = ReadWorkbook::read(&modified_bytes, &WorkbookReadLimits::default());
    assert!(res.is_err(), "MustUnderstand='x15' injection into pinned XLSX must be rejected");
    match res {
        Err(ReadError::Unsupported(msg)) => {
            assert!(msg.contains("MustUnderstand") && msg.contains("x15"),
                "Expected error message mentioning MustUnderstand and x15, got: {msg}");
        }
        other => panic!("Expected ReadError::Unsupported, got: {:?}", other),
    }
}

