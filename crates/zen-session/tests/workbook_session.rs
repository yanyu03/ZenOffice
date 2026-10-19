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
