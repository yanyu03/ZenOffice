use zen_engine::workbook::{CellAddress, TypedValue};
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};
use zen_session::{EditOutcome, HistoryBudget, SessionError, WorkbookSession};

fn a(row: u32, col: u16) -> CellAddress { CellAddress::new(row, col).unwrap() }
fn open(bytes: &[u8]) -> WorkbookSession {
    WorkbookSession::from_bytes(bytes, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap()
}
fn value(read: &ReadWorkbook, sheet: &str, at: CellAddress) -> f64 {
    match read.workbook().sheets().iter().find(|s| s.name() == sheet).unwrap().cell(at).unwrap().value.as_ref().unwrap() {
        TypedValue::Number { value, .. } => *value,
        other => panic!("expected numeric cache, got {other:?}"),
    }
}
fn expect(s: &WorkbookSession, sheet: &str, at: CellAddress, expected: f64) {
    assert!((value(s.read_workbook(), sheet, at) - expected).abs() < 1e-8);
    let reopened = ReadWorkbook::read(s.bytes(), &WorkbookReadLimits::default()).unwrap();
    assert!((value(&reopened, sheet, at) - expected).abs() < 1e-8);
}

/// Independent handwritten workbook: 3+7=10; twice that is 20; Report links it.
fn small_fixture(extra: &str) -> Vec<u8> {
    let mut z = zen_opc::ZipWriter::new();
    z.add("[Content_Types].xml", br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/s1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/s2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_vec());
    z.add("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="root" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec());
    z.add("xl/_rels/workbook.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="a" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/s1.xml"/><Relationship Id="b" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/s2.xml"/></Relationships>"#.to_vec());
    z.add("xl/workbook.xml", br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Inputs" sheetId="3" r:id="a"/><sheet name="Report" sheetId="9" r:id="b"/></sheets></workbook>"#.to_vec());
    z.add("xl/worksheets/s1.xml", format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="B1"><v>7</v></c></row><row r="2"><c r="A2"><f>A1+B1</f><v>10</v></c><c r="B2"><f>A2*2</f><v>20</v></c></row></sheetData>{extra}</worksheet>"#).into_bytes());
    z.add("xl/worksheets/s2.xml", br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f>Inputs!B2</f><v>20</v></c></row></sheetData></worksheet>"#.to_vec());
    z.finish().unwrap()
}
fn business_fixture() -> &'static [u8] {
    include_bytes!("../../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx")
}

/// Independent expected types: A2 links A1, B2 is text, C2 is boolean;
/// Report!A1 links A2 across sheets. C1 is an isolated styled blank input.
fn typed_fixture(extra: &str) -> Vec<u8> {
    let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&small_fixture("")).unwrap()).unwrap();
    p.put_text("/xl/worksheets/s1.xml", format!(r#"<x:worksheet xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><x:sheetData><x:row r="1"><x:c r="A1"><x:v>3</x:v></x:c><x:c r="B1"><x:v>7</x:v></x:c><x:c r="C1" s="2"/></x:row><x:row r="2"><x:c r="A2"><x:f>A1</x:f><x:v>3</x:v></x:c><x:c r="B2" t="str"><x:f>IF(A1=3,"yes","no")</x:f><x:v>yes</x:v></x:c><x:c r="C2" t="b"><x:f>A1=3</x:f><x:v>1</x:v></x:c></x:row></x:sheetData>{extra}</x:worksheet>"#)).unwrap();
    p.put_text("/xl/worksheets/s2.xml", r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f>Inputs!A2</f><v>3</v></c></row></sheetData></worksheet>"#).unwrap();
    p.to_bytes().unwrap()
}

fn typed(s: &WorkbookSession, sheet: &str, at: CellAddress) -> Option<TypedValue> {
    s.workbook().sheets().iter().find(|w| w.name() == sheet).unwrap().cell(at).unwrap().value.clone()
}

#[test]
fn formula_replacement_rebuilds_cross_sheet_dependencies_and_exact_history() {
    let mut s = open(&typed_fixture(""));
    let original = s.bytes().to_vec();
    s.set_formula("Report", a(0, 0), "=Inputs!B1*2").unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "14");
    let changed = s.bytes().to_vec();
    s.undo().unwrap(); assert_eq!(s.bytes(), original);
    let before = state(&s);
    assert_eq!(s.set_formula("Report", a(0, 0), "=Inputs!A2").unwrap(), EditOutcome::Unchanged);
    assert_eq!(before, state(&s)); assert!(s.can_redo());
    s.redo().unwrap(); assert_eq!(s.bytes(), changed);
    s.set_value("Inputs", a(0, 0), Some(TypedValue::Number { value: 9.0, lexical: "9".into() })).unwrap();
    assert_eq!(typed(&s, "Inputs", a(1, 0)).unwrap().text(), "9");
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "14", "old dependency released");
    s.set_value("Inputs", a(0, 1), Some(TypedValue::Number { value: 4.0, lexical: "4".into() })).unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "8", "new dependency active");
    s.set_formula("Report", a(0, 0), "Inputs!A2+1").unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "10");
    let reopened = open(s.bytes());
    assert_eq!(typed(&reopened, "Report", a(0, 0)).unwrap().text(), "10");
}

#[test]
fn formula_xml_text_is_distinct_from_string_cache_and_calc_configuration_is_preserved() {
    let raw = typed_fixture("");
    let mut package = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&raw).unwrap()).unwrap();
    let workbook = package.get_text("/xl/workbook.xml").unwrap().replace("</workbook>",
        "<calcPr calcMode=\"manual\" fullCalcOnLoad=\"0\" forceFullCalc=\"0\"/></workbook>");
    package.put_text("/xl/workbook.xml", workbook).unwrap();
    let source = package.to_bytes().unwrap();
    let mut s = open(&source);
    let formula = "IF(Inputs!A1=3,\"_x0041_ & <\",\"bad\")";
    s.set_formula("Inputs", a(0, 2), formula).unwrap();
    assert_eq!(typed(&s, "Inputs", a(0, 2)), Some(TypedValue::Text("_x0041_ & <".into())));
    assert_eq!(s.workbook().sheets()[0].cell(a(0, 2)).unwrap().formula.as_ref().unwrap().text, formula);
    assert_eq!(s.workbook().sheets()[0].cell(a(0, 2)).unwrap().style_index, Some(2));
    let zip = zen_opc::ZipArchive::read(s.bytes()).unwrap();
    let xml = std::str::from_utf8(zip.get("xl/worksheets/s1.xml").unwrap()).unwrap();
    assert!(xml.contains("<x:f>IF(Inputs!A1=3,\"_x0041_ &amp; &lt;\",\"bad\")</x:f>"));
    assert!(xml.contains("<x:v>_x005F_x0041_ &amp; &lt;</x:v>"));
    s.set_formula("Inputs", a(0, 2), "Inputs!B1>3").unwrap();
    assert_eq!(typed(&s, "Inputs", a(0, 2)), Some(TypedValue::Boolean(true)));
    let original = zen_opc::ZipArchive::read(&source).unwrap();
    let after = zen_opc::ZipArchive::read(s.bytes()).unwrap();
    for entry in &original.entries {
        if entry.name != "xl/worksheets/s1.xml" { assert_eq!(after.get(&entry.name), Some(entry.data.as_slice()), "{}", entry.name); }
    }
}

#[test]
fn formula_rejections_and_budget_preserve_state_with_live_redo() {
    let mut s = open(&typed_fixture(""));
    s.set_formula("Report", a(0, 0), "Inputs!B1*2").unwrap();
    s.undo().unwrap();
    for formula in ["", "=", "==1", "MISSING+1", "[other.xlsx]Sheet!A1", "Report!A1+1", "1/0", "1\0+2", "INDIRECT(\"A1\")"] {
        let before = state(&s);
        assert!(s.set_formula("Report", a(0, 0), formula).is_err(), "{formula}");
        assert_eq!(before, state(&s));
    }
    let before = state(&s);
    assert!(s.set_formula("Report", a(9, 9), "1+2").is_err());
    assert_eq!(before, state(&s));
    s.redo().unwrap();
    let source = typed_fixture("");
    let mut s = WorkbookSession::from_bytes(&source, &WorkbookReadLimits::default(), HistoryBudget { max_frames: 1, ..HistoryBudget::default() }).unwrap();
    let before = state(&s);
    assert!(matches!(s.set_formula("Report", a(0, 0), "Inputs!B1*2"), Err(SessionError::HistoryBudget)));
    assert_eq!(before, state(&s));
    let mut package = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&source).unwrap()).unwrap();
    let xml = package.get_text("/xl/worksheets/s1.xml").unwrap().replace("<x:f>A1</x:f>", "<x:f t=\"array\" ref=\"A2:A2\">A1</x:f>");
    package.put_text("/xl/worksheets/s1.xml", xml).unwrap();
    let mut s = open(&package.to_bytes().unwrap()); let before = state(&s);
    assert!(s.set_formula("Inputs", a(1, 0), "B1").is_err()); assert_eq!(before, state(&s));
    let mut package = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&source).unwrap()).unwrap();
    let xml = package.get_text("/xl/worksheets/s1.xml").unwrap().replace("<x:f>A1</x:f>", "<x:f ca=\"1\">A1</x:f>");
    package.put_text("/xl/worksheets/s1.xml", xml).unwrap();
    let mut s = open(&package.to_bytes().unwrap()); let before = state(&s);
    let error = s.set_formula("Inputs", a(1, 0), "B1").unwrap_err();
    assert!(error.to_string().contains("formula metadata"), "{error}"); assert_eq!(before, state(&s));
}

fn formula_chain_fixture() -> Vec<u8> {
    let source = typed_fixture("");
    let original = zen_opc::ZipArchive::read(&source).unwrap();
    let mut z = zen_opc::ZipWriter::new();
    for entry in &original.entries {
        let bytes = if entry.name == "[Content_Types].xml" {
            std::str::from_utf8(&entry.data).unwrap().replace("</Types>", "<Override PartName=\"/xl/calcChain.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml\"/></Types>").into_bytes()
        } else if entry.name == "xl/_rels/workbook.xml.rels" {
            std::str::from_utf8(&entry.data).unwrap().replace("</Relationships>", "<Relationship Id=\"chain\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain\" Target=\"calcChain.xml\"/></Relationships>").into_bytes()
        } else { entry.data.clone() };
        z.add(&entry.name, bytes);
    }
    z.add("xl/calcChain.xml", br#"<calcChain xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><c r="A2" i="3"/><c r="B2"/><c r="C2"/><c r="A1" i="9"/></calcChain>"#.to_vec());
    z.finish().unwrap()
}

fn unsupported_chain_fixture() -> Vec<u8> {
    let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&formula_chain_fixture()).unwrap()).unwrap();
    let xml = p.get_text("/xl/calcChain.xml").unwrap().replace("</calcChain>", "<extLst/></calcChain>");
    p.put_text("/xl/calcChain.xml", xml).unwrap(); p.to_bytes().unwrap()
}

fn expect_chain_absent(bytes: &[u8]) {
    let zip = zen_opc::ZipArchive::read(bytes).unwrap();
    assert!(zip.get("xl/calcChain.xml").is_none());
    let rels = std::str::from_utf8(zip.get("xl/_rels/workbook.xml.rels").unwrap()).unwrap();
    assert!(!rels.contains("/calcChain"));
    let types = std::str::from_utf8(zip.get("[Content_Types].xml").unwrap()).unwrap();
    assert!(!types.contains("/xl/calcChain.xml"));
    let read = ReadWorkbook::read(bytes, &WorkbookReadLimits::default()).unwrap();
    assert!(!read.package().diagnostics().iter().any(|d| matches!(d.code,
        zen_formats::opc::DiagnosticCode::DanglingTarget | zen_formats::opc::DiagnosticCode::UnmappedZipItem)));
}

#[test]
fn chain_formula_replacement_invalidates_part_relationship_type_and_exact_history() {
    let source = formula_chain_fixture(); let original = zen_opc::ZipArchive::read(&source).unwrap();
    let mut s = open(&source);
    s.set_formula("Report", a(0, 0), "Inputs!B1*2").unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "14");
    expect_chain_absent(s.bytes()); let changed = s.bytes().to_vec();
    let after = zen_opc::ZipArchive::read(&changed).unwrap();
    assert_eq!(after.entries.len() + 1, original.entries.len());
    for e in &original.entries {
        if !["xl/calcChain.xml", "xl/_rels/workbook.xml.rels", "[Content_Types].xml", "xl/worksheets/s2.xml"].contains(&e.name.as_str()) {
            assert_eq!(after.get(&e.name), Some(e.data.as_slice()), "{}", e.name);
        }
    }
    s.undo().unwrap(); assert_eq!(s.bytes(), source);
    assert!(zen_opc::ZipArchive::read(s.bytes()).unwrap().get("xl/calcChain.xml").is_some());
    s.redo().unwrap(); assert_eq!(s.bytes(), changed);
    s.set_number("Inputs", a(0, 1), 8.0).unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "16"); expect_chain_absent(s.bytes());
}

#[test]
fn chain_clear_and_manual_calc_policy_preserve_workbook_and_rebuild_from_formula_bytes() {
    let raw = formula_chain_fixture();
    let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&raw).unwrap()).unwrap();
    let workbook = p.get_text("/xl/workbook.xml").unwrap().replace("</workbook>", "<calcPr calcMode=\"manual\" fullCalcOnLoad=\"0\" forceFullCalc=\"0\"/></workbook>");
    p.put_text("/xl/workbook.xml", workbook).unwrap();
    let source = p.to_bytes().unwrap(); let before = zen_opc::ZipArchive::read(&source).unwrap();
    let mut s = open(&source); s.clear_formula("Report", a(0, 0)).unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)), None); expect_chain_absent(s.bytes());
    let after = zen_opc::ZipArchive::read(s.bytes()).unwrap();
    assert_eq!(after.get("xl/workbook.xml"), before.get("xl/workbook.xml"));
    s.set_formula("Report", a(0, 0), "Inputs!A2+1").unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "4"); expect_chain_absent(s.bytes());
    let reopened = open(s.bytes());
    assert_eq!(typed(&reopened, "Report", a(0, 0)).unwrap().text(), "4");
}

#[test]
fn chain_unknown_metadata_shared_ownership_and_duplicate_links_refuse_with_redo() {
    let source = formula_chain_fixture();
    for (part, from, to) in [
        ("/xl/calcChain.xml", "</calcChain>", "<extLst/></calcChain>"),
        ("/xl/calcChain.xml", "r=\"A2\" i=\"3\"", "r=\"A2\" i=\"3\" s=\"1\""),
        ("/xl/calcChain.xml", "i=\"3\"", "i=\"999\""),
        ("/xl/calcChain.xml", "r=\"A2\"", "r=\"B1\""),
        ("/xl/_rels/workbook.xml.rels", "</Relationships>", "<Relationship Id=\"extra-chain\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain\" Target=\"calcChain.xml\"/></Relationships>"),
        ("/xl/_rels/workbook.xml.rels", "</Relationships>", "<Relationship Id=\"shared\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"calcChain.xml\"/></Relationships>"),
    ] {
        let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&source).unwrap()).unwrap();
        let xml = p.get_text(part).unwrap(); assert!(xml.contains(from));
        p.put_text(part, xml.replace(from, to)).unwrap();
        let mut s = open(&p.to_bytes().unwrap());
        s.set_number("Inputs", a(0, 1), 8.0).unwrap(); s.undo().unwrap(); let before = state(&s);
        assert!(s.set_formula("Report", a(0, 0), "Inputs!B1*2").is_err()); assert_eq!(before, state(&s));
        assert!(s.clear_formula("Report", a(0, 0)).is_err()); assert_eq!(before, state(&s));
    }
}

#[test]
fn chain_candidate_budget_failure_and_create_new_save_preserve_source_old_output_and_chain() {
    let source = formula_chain_fixture();
    let mut s = WorkbookSession::from_bytes(&source, &WorkbookReadLimits::default(), HistoryBudget { max_frames: 1, ..HistoryBudget::default() }).unwrap();
    let before = state(&s);
    assert!(matches!(s.set_formula("Report", a(0, 0), "Inputs!B1*2"), Err(SessionError::HistoryBudget)));
    assert_eq!(before, state(&s));
    let dir = tempfile::tempdir().unwrap(); let input = dir.path().join("input.xlsx");
    let output = dir.path().join("no-chain.xlsx"); let old = dir.path().join("old.xlsx");
    std::fs::write(&input, &source).unwrap(); std::fs::write(&old, b"old output").unwrap();
    let mut s = WorkbookSession::open(&input, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    s.set_formula("Report", a(0, 0), "Inputs!B1*2").unwrap(); let before = state(&s);
    assert!(s.save_as_new(&old).is_err()); assert_eq!(before, state(&s));
    assert_eq!(std::fs::read(&old).unwrap(), b"old output");
    s.save_as_new(&output).unwrap(); assert!(!s.is_dirty());
    expect_chain_absent(&std::fs::read(&output).unwrap());
    let reopened = WorkbookSession::open(&output, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    assert_eq!(typed(&reopened, "Report", a(0, 0)).unwrap().text(), "14");
    assert_eq!(std::fs::read(&input).unwrap(), source);
    s.undo().unwrap(); assert_eq!(s.bytes(), source); assert!(s.is_dirty());
    s.redo().unwrap(); assert!(!s.is_dirty()); expect_chain_absent(s.bytes());
}

#[test]
fn formula_calc_chain_rejection_and_new_file_save_are_explicit() {
    let source = typed_fixture("");
    let chained = unsupported_chain_fixture();
    let mut s = open(&chained); let before = state(&s);
    let error = s.set_formula("Report", a(0, 0), "Inputs!B1*2").unwrap_err();
    assert!(error.to_string().contains("calcChain"), "{error}"); assert_eq!(before, state(&s));
    let dir = tempfile::tempdir().unwrap(); let input = dir.path().join("input.xlsx");
    let output = dir.path().join("formula.xlsx"); let old = dir.path().join("old.xlsx");
    std::fs::write(&input, &source).unwrap(); std::fs::write(&old, b"old output").unwrap();
    let mut s = WorkbookSession::open(&input, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    s.set_formula("Report", a(0, 0), "Inputs!B1*2").unwrap(); let before = state(&s);
    assert!(s.save_as_new(&old).is_err()); assert_eq!(before, state(&s));
    assert_eq!(std::fs::read(&old).unwrap(), b"old output");
    s.save_as_new(&output).unwrap(); assert!(!s.is_dirty());
    let reopened = WorkbookSession::open(&output, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    assert_eq!(typed(&reopened, "Report", a(0, 0)).unwrap().text(), "14");
    assert_eq!(std::fs::read(&input).unwrap(), source);
    s.undo().unwrap(); assert!(s.is_dirty()); s.redo().unwrap(); assert!(!s.is_dirty());
}

#[test]
fn clear_formula_removes_definition_and_cache_recalculates_and_restores_exact_frames() {
    let raw = typed_fixture("");
    let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&raw).unwrap()).unwrap();
    let xml = p.get_text("/xl/worksheets/s1.xml").unwrap().replace("r=\"A2\"", "r=\"A2\" s=\"2\"");
    p.put_text("/xl/worksheets/s1.xml", xml).unwrap();
    let source = p.to_bytes().unwrap();
    let mut s = open(&source);
    s.set_formula("Report", a(0, 0), "Inputs!A2+1").unwrap();
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "4");
    let before = s.bytes().to_vec();
    s.clear_formula("Inputs", a(1, 0)).unwrap();
    let cell = s.workbook().sheets()[0].cell(a(1, 0)).unwrap();
    assert_eq!(cell.value, None); assert!(cell.formula.is_none()); assert_eq!(cell.style_index, Some(2));
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "1");
    let cleared = s.bytes().to_vec();
    let zip = zen_opc::ZipArchive::read(&cleared).unwrap();
    let xml = std::str::from_utf8(zip.get("xl/worksheets/s1.xml").unwrap()).unwrap();
    assert!(xml.contains("<x:c r=\"A2\" s=\"2\"></x:c>"));
    let prior = zen_opc::ZipArchive::read(&before).unwrap();
    for e in &prior.entries {
        if !["xl/worksheets/s1.xml", "xl/worksheets/s2.xml"].contains(&e.name.as_str()) {
            assert_eq!(zip.get(&e.name), Some(e.data.as_slice()), "{}", e.name);
        }
    }
    s.undo().unwrap(); assert_eq!(s.bytes(), before);
    s.redo().unwrap(); assert_eq!(s.bytes(), cleared);
    s.set_value("Inputs", a(0, 0), Some(TypedValue::Number { value: 9.0, lexical: "9".into() })).unwrap();
    assert_eq!(typed(&s, "Inputs", a(1, 0)), None, "cleared cell has no dependency on A1");
    assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "1");
    assert_eq!(typed(&open(s.bytes()), "Report", a(0, 0)).unwrap().text(), "1");
    let mut direct = open(&typed_fixture(""));
    direct.clear_formula("Inputs", a(1, 0)).unwrap();
    assert_eq!(typed(&direct, "Report", a(0, 0)).unwrap().text(), "0", "direct blank reference evaluates to zero");
}

#[test]
fn clear_formula_rejects_chain_metadata_nonformula_and_changed_error_cache_atomically() {
    let mut s = open(&typed_fixture(""));
    s.set_formula("Report", a(0, 0), "1/Inputs!A2").unwrap();
    s.clear_formula("Report", a(0, 0)).unwrap(); s.undo().unwrap();
    for (sheet, at) in [("Inputs", a(0, 0)), ("Inputs", a(1, 0)), ("Inputs", a(9, 9))] {
        let before = state(&s);
        assert!(s.clear_formula(sheet, at).is_err()); assert_eq!(before, state(&s));
    }
    let mut s = open(&unsupported_chain_fixture());
    s.set_number("Inputs", a(0, 1), 8.0).unwrap(); s.undo().unwrap();
    let before = state(&s);
    let error = s.clear_formula("Report", a(0, 0)).unwrap_err();
    assert!(error.to_string().contains("calcChain"), "{error}"); assert_eq!(before, state(&s));
    for replacement in ["<x:f ca=\"1\">A1</x:f>", "<x:f t=\"array\" ref=\"A2:A2\">A1</x:f>"] {
        let raw = typed_fixture("");
        let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&raw).unwrap()).unwrap();
        let xml = p.get_text("/xl/worksheets/s1.xml").unwrap().replace("<x:f>A1</x:f>", replacement);
        p.put_text("/xl/worksheets/s1.xml", xml).unwrap();
        let mut s = open(&p.to_bytes().unwrap()); let before = state(&s);
        assert!(s.clear_formula("Inputs", a(1, 0)).is_err()); assert_eq!(before, state(&s));
    }
}

#[test]
fn clear_formula_blank_noop_keeps_redo_and_budget_failure_keeps_history() {
    let source = typed_fixture("");
    let mut s = open(&source);
    s.clear_formula("Report", a(0, 0)).unwrap();
    s.set_number("Inputs", a(0, 1), 8.0).unwrap(); s.undo().unwrap();
    let before = state(&s);
    assert_eq!(s.clear_formula("report", a(0, 0)).unwrap(), EditOutcome::Unchanged);
    assert_eq!(before, state(&s)); assert!(s.can_redo());
    s.set_formula("Report", a(0, 0), "Inputs!B1*2").unwrap();
    assert!(!s.can_redo()); assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "14");
    for budget in [HistoryBudget { max_frames: 1, ..HistoryBudget::default() },
                   HistoryBudget { max_retained_archive_bytes: source.len(), ..HistoryBudget::default() }] {
        let mut s = WorkbookSession::from_bytes(&source, &WorkbookReadLimits::default(), budget).unwrap();
        let before = state(&s);
        assert!(matches!(s.clear_formula("Report", a(0, 0)), Err(SessionError::HistoryBudget)));
        assert_eq!(before, state(&s));
    }
}

#[test]
fn clear_formula_save_reopens_blank_and_protects_source_old_output_and_savepoint() {
    let dir = tempfile::tempdir().unwrap(); let source = typed_fixture("");
    let input = dir.path().join("input.xlsx"); let output = dir.path().join("clear.xlsx");
    let old = dir.path().join("old.xlsx");
    std::fs::write(&input, &source).unwrap(); std::fs::write(&old, b"old bytes").unwrap();
    let mut s = WorkbookSession::open(&input, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    s.clear_formula("Report", a(0, 0)).unwrap(); let before = state(&s);
    assert!(s.save_as_new(&old).is_err()); assert_eq!(before, state(&s));
    assert_eq!(std::fs::read(&old).unwrap(), b"old bytes");
    s.save_as_new(&output).unwrap(); assert!(!s.is_dirty());
    let reopened = WorkbookSession::open(&output, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    let cell = reopened.workbook().sheets()[1].cell(a(0, 0)).unwrap();
    assert_eq!(cell.value, None); assert!(cell.formula.is_none()); assert_eq!(reopened.bytes(), s.bytes());
    assert_eq!(std::fs::read(&input).unwrap(), source);
    s.undo().unwrap(); assert!(s.is_dirty()); assert_eq!(typed(&s, "Report", a(0, 0)).unwrap().text(), "3");
    s.redo().unwrap(); assert!(!s.is_dirty());
}

#[test]
fn typed_literals_distinguish_blank_empty_false_zero_and_escape_round_trip() {
    let raw = typed_fixture("");
    let mut package = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&raw).unwrap()).unwrap();
    let xml = package.get_text("/xl/worksheets/s1.xml").unwrap()
        .replace("r=\"C1\" s=\"2\"/>", "r=\"C1\" s=\"2\" xmlns:local=\"urn:>zen\"/>");
    assert!(xml.contains("xmlns:local=\"urn:>zen\""));
    package.put_text("/xl/worksheets/s1.xml", xml).unwrap();
    let source = package.to_bytes().unwrap();
    let original = zen_opc::ZipArchive::read(&source).unwrap();
    let mut s = open(&source);
    let at = a(0, 2);
    for text in ["=A1", "  <&>\"' 中文😀\r\n\t  ", "_x0041_ _x005F_ _xD800_", ""] {
        let before = s.bytes().to_vec();
        s.set_value("Inputs", at, Some(TypedValue::Text(text.into()))).unwrap();
        assert_eq!(typed(&s, "Inputs", at), Some(TypedValue::Text(text.into())));
        let changed = s.bytes().to_vec();
        s.undo().unwrap(); assert_eq!(s.bytes(), before);
        let revision = s.revision();
        s.redo().unwrap(); assert_eq!(s.bytes(), changed);
        assert_eq!(s.revision(), revision + 1);
        let before = state(&s);
        assert_eq!(s.set_value("inputs", at, Some(TypedValue::Text(text.into()))).unwrap(), EditOutcome::Unchanged);
        assert_eq!(state(&s), before);
    }
    for next in [Some(TypedValue::Boolean(false)), Some(TypedValue::Number { value: 0.0, lexical: "garbage".into() }), None] {
        s.set_value("Inputs", at, next.clone()).unwrap();
        let expected = next.map(|v| match v { TypedValue::Number { value, .. } => TypedValue::Number { value, lexical: "0".into() }, v => v });
        assert_eq!(typed(&s, "Inputs", at), expected);
        assert_eq!(s.workbook().sheets()[0].cell(at).unwrap().style_index, Some(2));
    }
    let final_zip = zen_opc::ZipArchive::read(s.bytes()).unwrap();
    for entry in &original.entries {
        if entry.name != "xl/worksheets/s1.xml" {
            assert_eq!(final_zip.get(&entry.name), Some(entry.data.as_slice()), "{}", entry.name);
        }
    }
    let before = state(&s);
    assert_eq!(s.set_value("Inputs", at, None).unwrap(), EditOutcome::Unchanged);
    assert_eq!(before, state(&s));
}

#[test]
fn typed_input_updates_text_boolean_and_cross_sheet_cache_types_without_changing_formulas() {
    let source = typed_fixture("");
    let mut s = open(&source);
    for next in [TypedValue::Text("done & _x0041_".into()), TypedValue::Text("".into()), TypedValue::Boolean(true),
                 TypedValue::Number { value: 7.0, lexical: "not trusted".into() }] {
        let before = s.bytes().to_vec();
        s.set_value("Inputs", a(0, 0), Some(next.clone())).unwrap();
        let expected = match next { TypedValue::Number { value, .. } => TypedValue::Number { value, lexical: "7".into() }, v => v };
        for (sheet, at) in [("Inputs", a(0, 0)), ("Inputs", a(1, 0)), ("Report", a(0, 0))] {
            assert_eq!(typed(&s, sheet, at), Some(expected.clone()), "{sheet} {at:?}");
        }
        assert_eq!(typed(&s, "Inputs", a(1, 1)), Some(TypedValue::Text("no".into())));
        assert_eq!(typed(&s, "Inputs", a(1, 2)), Some(TypedValue::Boolean(false)));
        for (sheet, at, formula) in [("Inputs", a(1, 0), "A1"), ("Inputs", a(1, 1), "IF(A1=3,\"yes\",\"no\")"),
                                    ("Inputs", a(1, 2), "A1=3"), ("Report", a(0, 0), "Inputs!A2")] {
            assert_eq!(s.workbook().sheets().iter().find(|w| w.name() == sheet).unwrap().cell(at).unwrap().formula.as_ref().unwrap().text, formula);
        }
        let after = s.bytes().to_vec();
        s.undo().unwrap(); assert_eq!(s.bytes(), before);
        s.redo().unwrap(); assert_eq!(s.bytes(), after);
    }
}

#[test]
fn typed_rejections_preserve_full_state_and_live_redo() {
    let mut s = open(&typed_fixture(""));
    s.set_value("Inputs", a(0, 2), Some(TypedValue::Text("branch".into()))).unwrap();
    s.undo().unwrap();
    for (at, next) in [(a(9, 9), None), (a(1, 0), None), (a(0, 2), Some(TypedValue::Error("#DIV/0!".into()))),
                      (a(0, 2), Some(TypedValue::Text("invalid\0".into()))),
                      (a(0, 2), Some(TypedValue::Number { value: f64::INFINITY, lexical: "0".into() }))] {
        let before = state(&s);
        assert!(s.set_value("Inputs", at, next).is_err());
        assert_eq!(state(&s), before);
    }
    assert!(s.can_redo()); s.redo().unwrap();
    let mut s = open(&small_fixture(""));
    let before = state(&s);
    // A1+B1 cannot become a text cache here: it becomes an unsupported error.
    assert!(s.set_value("Inputs", a(0, 0), Some(TypedValue::Text("word".into()))).is_err());
    assert_eq!(state(&s), before);
}

#[test]
fn typed_budget_and_merged_protected_or_opaque_targets_fail_atomically() {
    let source = business_fixture();
    let zip = zen_opc::ZipArchive::read(source).unwrap();
    assert!(std::str::from_utf8(zip.get("xl/charts/chart1.xml").unwrap()).unwrap().contains("strRef"));
    let mut s = open(source);
    let before = state(&s);
    let error = s.set_value("Transactions", a(3, 3), Some(TypedValue::Number { value: 42.0, lexical: "42".into() })).unwrap_err();
    assert!(error.to_string().contains("chart string-reference caches"), "{error}");
    assert_eq!(before, state(&s));
    for extra in [r#"<x:mergeCells><x:mergeCell ref="B1:C1"/></x:mergeCells>"#,
                  r#"<x:sheetProtection sheet="1"/>"#] {
        let mut s = open(&typed_fixture(extra));
        let before = state(&s);
        assert!(s.set_value("Inputs", a(0, 2), Some(TypedValue::Boolean(false))).is_err());
        assert_eq!(before, state(&s));
    }
    let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&typed_fixture("")).unwrap()).unwrap();
    let xml = p.get_text("/xl/worksheets/s1.xml").unwrap().replace("r=\"C1\" s=\"2\"/>", "r=\"C1\" s=\"2\"><x:extLst/></x:c>");
    p.put_text("/xl/worksheets/s1.xml", xml).unwrap();
    let mut s = open(&p.to_bytes().unwrap());
    let before = state(&s);
    assert!(s.set_value("Inputs", a(0, 2), Some(TypedValue::Boolean(false))).is_err());
    assert_eq!(before, state(&s));
    for budget in [HistoryBudget { max_frames: 1, ..HistoryBudget::default() },
                   HistoryBudget { max_retained_archive_bytes: typed_fixture("").len(), ..HistoryBudget::default() }] {
        let mut s = WorkbookSession::from_bytes(&typed_fixture(""), &WorkbookReadLimits::default(), budget).unwrap();
        let before = state(&s);
        assert!(matches!(s.set_value("Inputs", a(0, 2), Some(TypedValue::Boolean(false))), Err(SessionError::HistoryBudget)));
        assert_eq!(before, state(&s));
    }
    let limits = WorkbookReadLimits { max_text_bytes: 1000, ..WorkbookReadLimits::default() };
    let mut s = WorkbookSession::from_bytes(&typed_fixture(""), &limits, HistoryBudget::default()).unwrap();
    for (at, text) in [(a(0, 2), "a".repeat(1001)), (a(0, 0), "b".repeat(500))] {
        let before = state(&s);
        assert!(matches!(s.set_value("Inputs", at, Some(TypedValue::Text(text))), Err(SessionError::Read(_))));
        assert_eq!(before, state(&s));
    }
}

#[test]
fn typed_new_branch_clears_redo_only_after_success() {
    let mut s = open(&typed_fixture(""));
    s.set_value("Inputs", a(0, 2), Some(TypedValue::Text("one".into()))).unwrap();
    s.set_value("Inputs", a(0, 2), Some(TypedValue::Boolean(false))).unwrap();
    s.undo().unwrap();
    let before = state(&s);
    assert_eq!(s.set_value("Inputs", a(0, 2), Some(TypedValue::Text("one".into()))).unwrap(), EditOutcome::Unchanged);
    assert_eq!(before, state(&s));
    s.set_value("Inputs", a(0, 2), None).unwrap();
    assert!(!s.can_redo()); assert_eq!(s.history_frames(), 3);
    assert_eq!(typed(&s, "Inputs", a(0, 2)), None);
}

#[test]
fn typed_save_as_new_and_failed_old_output_preserve_source_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.xlsx");
    let output = dir.path().join("typed.xlsx");
    let old = dir.path().join("old.xlsx");
    let bytes = typed_fixture("");
    std::fs::write(&source, &bytes).unwrap();
    std::fs::write(&old, b"old bytes").unwrap();
    let mut s = WorkbookSession::open(&source, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    s.set_value("Inputs", a(0, 0), Some(TypedValue::Text("literal =A1".into()))).unwrap();
    s.set_value("Inputs", a(0, 2), Some(TypedValue::Boolean(false))).unwrap();
    let before = state(&s);
    assert!(s.save_as_new(&old).is_err()); assert_eq!(before, state(&s));
    assert_eq!(std::fs::read(&old).unwrap(), b"old bytes");
    s.save_as_new(&output).unwrap(); assert!(!s.is_dirty());
    let reopened = WorkbookSession::open(&output, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap();
    assert_eq!(reopened.bytes(), s.bytes());
    assert_eq!(typed(&reopened, "Report", a(0, 0)), Some(TypedValue::Text("literal =A1".into())));
    assert_eq!(typed(&reopened, "Inputs", a(0, 2)), Some(TypedValue::Boolean(false)));
    s.undo().unwrap(); assert!(s.is_dirty()); s.redo().unwrap(); assert!(!s.is_dirty());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
}

#[derive(Debug, PartialEq)]
struct State {
    bytes: Vec<u8>, model: String, diagnostics: String, revision: u64,
    frames: usize, archive_bytes: usize, dirty: bool, undo: bool, redo: bool,
}
fn state(s: &WorkbookSession) -> State {
    State { bytes: s.bytes().to_vec(), model: format!("{:?}", s.workbook()),
            diagnostics: format!("{:?}", s.diagnostics()), revision: s.revision(),
            frames: s.history_frames(), archive_bytes: s.retained_archive_bytes(),
            dirty: s.is_dirty(), undo: s.can_undo(), redo: s.can_redo() }
}

#[test]
fn independent_small_workbook_two_edits_history_and_cross_sheet_caches() {
    let source = small_fixture("");
    let original = source.clone();
    let mut s = open(&source);
    assert_eq!(s.undo().unwrap(), EditOutcome::Unchanged);
    assert_eq!(s.redo().unwrap(), EditOutcome::Unchanged);
    assert_eq!(s.revision(), 0);
    assert_eq!(s.set_number("Inputs", a(0,0), 11.0).unwrap(), EditOutcome::Changed);
    expect(&s, "Inputs", a(1,0), 18.0);
    expect(&s, "Report", a(0,0), 36.0);
    let first = s.bytes().to_vec();
    s.set_number("Inputs", a(0,1), 5.0).unwrap();
    expect(&s, "Inputs", a(1,0), 16.0);
    expect(&s, "Report", a(0,0), 32.0);
    let second = s.bytes().to_vec();
    assert_eq!(s.history_frames(), 3);
    assert!(s.is_dirty());
    s.undo().unwrap(); assert_eq!(s.bytes(), first); expect(&s,"Report",a(0,0),36.0);
    s.undo().unwrap(); assert_eq!(s.bytes(), original); assert!(!s.is_dirty());
    s.redo().unwrap(); assert_eq!(s.bytes(), first);
    s.redo().unwrap(); assert_eq!(s.bytes(), second); expect(&s,"Report",a(0,0),32.0);
    assert_eq!(s.revision(), 6);
    assert_eq!(source, original);
}

#[test]
fn independent_business_workbook_two_edits_and_byte_exact_history() {
    let source = business_fixture();
    let original = source.to_vec();
    let mut s = open(source);
    s.set_number("Transactions",a(3,3),10.0).unwrap();
    expect(&s,"Transactions",a(3,5),199.5);
    expect(&s,"Dashboard",a(3,1),3753.2);
    let first = s.bytes().to_vec();
    s.set_number("Transactions",a(3,4),20.0).unwrap();
    expect(&s,"Transactions",a(3,5),200.0);
    expect(&s,"Dashboard",a(3,1),3753.7);
    let second = s.bytes().to_vec();
    s.undo().unwrap(); assert_eq!(s.bytes(),first);
    s.undo().unwrap(); assert_eq!(s.bytes(),original); assert!(!s.is_dirty());
    s.redo().unwrap(); s.redo().unwrap(); assert_eq!(s.bytes(),second);
    expect(&s,"Dashboard",a(3,1),3753.7);
    assert_eq!(source, original);
}

#[test]
fn no_op_preserves_redo_and_new_edit_discards_it_on_both_inputs() {
    for (source,sheet,first,second,new_value) in [
        (small_fixture(""),"Inputs",a(0,0),a(0,1),11.0),
        (business_fixture().to_vec(),"Transactions",a(3,3),a(3,4),10.0),
    ] {
        let mut s = open(&source);
        s.set_number(sheet,first,new_value).unwrap();
        let original_second = value(s.read_workbook(),sheet,second);
        s.set_number(sheet,second,99.0).unwrap();
        s.undo().unwrap();
        let before = state(&s);
        assert_eq!(s.set_number(&sheet.to_lowercase(),second,original_second).unwrap(),EditOutcome::Unchanged);
        assert_eq!(state(&s),before);
        s.set_number(sheet,second,88.0).unwrap();
        assert!(!s.can_redo());
        assert_eq!(s.redo().unwrap(),EditOutcome::Unchanged);
        expect(&s,sheet,second,88.0);
    }
}

#[test]
fn rejected_inputs_preserve_state_with_live_redo_on_both_workbooks() {
    for (source,sheet,input,formula,new_value) in [
        (small_fixture(""),"Inputs",a(0,0),a(1,0),11.0),
        (business_fixture().to_vec(),"Transactions",a(3,3),a(3,5),10.0),
    ] {
        let mut s=open(&source);
        s.set_number(sheet,input,new_value).unwrap(); s.undo().unwrap();
        for (target,at,v) in [(sheet,input,f64::NAN),(sheet,input,f64::INFINITY),
                             (sheet,input,f64::NEG_INFINITY),(sheet,formula,10.0),
                             ("absent",input,1.0),(sheet,a(999,12),1.0)] {
            let before=state(&s);
            assert!(s.set_number(target,at,v).is_err());
            assert_eq!(state(&s),before);
        }
        assert_eq!(s.redo().unwrap(),EditOutcome::Changed);
    }
}

#[test]
fn protected_and_unknown_dependencies_refuse_without_state_change() {
    for extra in ["<sheetProtection sheet=\"1\"/>","<extLst><ext uri=\"unknown\"/></extLst>",
                  "<x:dependency xmlns:x=\"urn:unknown\"/>"] {
        let source=small_fixture(extra); let mut s=open(&source); let before=state(&s);
        assert!(s.set_number("Inputs",a(0,0),11.0).is_err());
        assert_eq!(state(&s),before); assert_eq!(s.bytes(),source);
    }
}

#[test]
fn frame_and_byte_budgets_refuse_without_evicting_history() {
    let source=small_fixture("");
    for budget in [HistoryBudget {max_frames:1,..HistoryBudget::default()},
                   HistoryBudget {max_retained_archive_bytes:source.len(),..HistoryBudget::default()}] {
        let mut s=WorkbookSession::from_bytes(&source,&WorkbookReadLimits::default(),budget).unwrap();
        let before=state(&s);
        assert!(matches!(s.set_number("Inputs",a(0,0),11.0),Err(SessionError::HistoryBudget)));
        assert_eq!(state(&s),before);
    }
    let budget=HistoryBudget{max_frames:2,..HistoryBudget::default()};
    let mut s=WorkbookSession::from_bytes(&source,&WorkbookReadLimits::default(),budget).unwrap();
    s.set_number("Inputs",a(0,0),11.0).unwrap();
    let before=state(&s);
    assert!(matches!(s.set_number("Inputs",a(0,1),5.0),Err(SessionError::HistoryBudget)));
    assert_eq!(state(&s),before); s.undo().unwrap();
    // The discarded redo frame must not count against the new branch's budget.
    s.set_number("Inputs",a(0,0),12.0).unwrap();
    assert_eq!(s.history_frames(),2); assert!(!s.can_redo());
}

#[test]
fn invalid_open_and_empty_history_budget_fail_explicitly() {
    assert!(WorkbookSession::from_bytes(b"bad",&WorkbookReadLimits::default(),HistoryBudget::default()).is_err());
    assert!(matches!(WorkbookSession::from_bytes(&small_fixture(""),&WorkbookReadLimits::default(),
        HistoryBudget{max_frames:0,..HistoryBudget::default()}),Err(SessionError::HistoryBudget)));
}
