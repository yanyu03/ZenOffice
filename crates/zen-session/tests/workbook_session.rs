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
