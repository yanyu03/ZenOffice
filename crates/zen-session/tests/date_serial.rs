use zen_engine::workbook::{CellAddress, DateSystem, TypedValue};
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};
use zen_session::{EditOutcome, HistoryBudget, SessionError, WorkbookSession};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
fn a() -> CellAddress { CellAddress::new(0, 0).unwrap() }
fn open(bytes: &[u8]) -> WorkbookSession {
    WorkbookSession::from_bytes(bytes, &WorkbookReadLimits::default(), HistoryBudget::default()).unwrap()
}
fn number(s: &WorkbookSession, sheet: &str, col: u16) -> f64 {
    let cell = s.workbook().sheets().iter().find(|s| s.name() == sheet).unwrap()
        .cell(CellAddress::new(0, col).unwrap()).unwrap();
    match cell.value.as_ref().unwrap() { TypedValue::Number { value, .. } => *value, other => panic!("{other:?}") }
}
fn state(s: &WorkbookSession) -> (Vec<u8>, u64, usize, usize, bool, bool, bool) {
    (s.bytes().to_vec(), s.revision(), s.history_frames(), s.retained_archive_bytes(), s.is_dirty(), s.can_undo(), s.can_redo())
}

/// Independently authored boundary packages: explicit 1900/1904, date input,
/// local +1 and cross-sheet link. Constants are not computed by the new writer.
fn fixture(system: DateSystem, format: u32, prefix: bool) -> Vec<u8> {
    let mut z = zen_opc::ZipWriter::new();
    z.add("[Content_Types].xml", br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/s1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/s2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#.to_vec());
    z.add("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="root" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec());
    z.add("xl/_rels/workbook.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="a" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/s1.xml"/><Relationship Id="b" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/s2.xml"/><Relationship Id="style" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec());
    let is_1904 = if system == DateSystem::Excel1904 { "true" } else { "false" };
    z.add("xl/workbook.xml", format!(r#"<workbook xmlns="{NS}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><workbookPr date1904="{is_1904}"/><sheets><sheet name="Dates" sheetId="1" r:id="a"/><sheet name="Report" sheetId="2" r:id="b"/></sheets><calcPr calcMode="manual" fullCalcOnLoad="0" forceFullCalc="0"/></workbook>"#).into_bytes());
    let custom = match format {
        164 => r#"<numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy-mm-dd"/></numFmts>"#,
        165 => r#"<numFmts count="1"><numFmt numFmtId="165" formatCode="yyyy-mm-dd hh:mm:ss"/></numFmts>"#,
        _ => "",
    };
    z.add("xl/styles.xml", format!(r#"<styleSheet xmlns="{NS}">{custom}<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="{format}" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#).into_bytes());
    let sheet = if prefix {
        format!(r#"<x:worksheet xmlns:x="{NS}"><x:sheetData><x:row r="1"><x:c r="A1" s="1"><x:v>59.000</x:v></x:c><x:c r="B1"><x:f>A1+1</x:f><x:v>60</x:v></x:c></x:row></x:sheetData></x:worksheet>"#)
    } else {
        format!(r#"<worksheet xmlns="{NS}"><sheetData><row r="1"><c r="A1" s="1"><v>59.000</v></c><c r="B1"><f>A1+1</f><v>60</v></c></row></sheetData></worksheet>"#)
    };
    z.add("xl/worksheets/s1.xml", sheet.into_bytes());
    z.add("xl/worksheets/s2.xml", format!(r#"<worksheet xmlns="{NS}"><sheetData><row r="1"><c r="A1"><f>Dates!B1</f><v>60</v></c></row></sheetData></worksheet>"#).into_bytes());
    z.finish().unwrap()
}
fn changed(bytes: &[u8], name: &str, from: &str, to: &str) -> Vec<u8> {
    let mut p = zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(bytes).unwrap()).unwrap();
    let old = String::from_utf8(p.get(name).unwrap().to_vec()).unwrap();
    assert!(old.contains(from), "fixture mutation missing: {from}");
    p.put_text(name, old.replace(from, to)).unwrap();
    p.to_bytes().unwrap()
}
fn preserved(source: &[u8], result: &[u8]) {
    let before = ReadWorkbook::read(source, &WorkbookReadLimits::default()).unwrap();
    let after = ReadWorkbook::read(result, &WorkbookReadLimits::default()).unwrap();
    assert_eq!(before.workbook().date_system(), after.workbook().date_system());
    for part in before.package().parts() {
        if !["/xl/worksheets/s1.xml", "/xl/worksheets/s2.xml"].contains(&part.name().as_str()) {
            assert_eq!(part.bytes(), after.package().part(part.name()).unwrap().bytes(), "{}", part.name().as_str());
        }
    }
    assert_eq!(after.workbook().sheets()[0].cell(a()).unwrap().style_index, Some(1));
}

#[test]
fn date_serial_boundaries_recalculate_and_preserve_both_systems() {
    for (system, format, prefix, end) in [(DateSystem::Excel1900,14,false,2_958_466.0), (DateSystem::Excel1904,165,true,2_957_004.0)] {
        let source = fixture(system, format, prefix); let mut s = open(&source);
        for serial in [0.0,1.0,59.0,60.0,61.0,61.5,end-0.5] {
            assert_eq!(s.set_date_serial("Dates",a(),serial,system).unwrap(), EditOutcome::Changed);
            assert_eq!(number(&s,"Dates",0),serial); assert_eq!(number(&s,"Dates",1),serial+1.0);
            assert_eq!(number(&s,"Report",0),serial+1.0); preserved(&source,s.bytes());
            let reopened = open(s.bytes()); assert_eq!(number(&reopened,"Report",0),serial+1.0);
            assert_eq!(reopened.workbook().date_system(),system);
        }
        let last = s.bytes().to_vec(); s.undo().unwrap(); assert_eq!(number(&s,"Dates",0),61.5);
        s.redo().unwrap(); assert_eq!(s.bytes(),last);
    }
}

#[test]
fn date_serial_admitted_formats_and_literal_type_transitions() {
    for format in [14,15,16,17,22,164,165] {
        let source=fixture(DateSystem::Excel1900,format,false); let mut s=open(&source);
        s.set_date_serial("Dates",a(),60.5,DateSystem::Excel1900).unwrap();
        preserved(&source,s.bytes()); assert_eq!(number(&s,"Report",0),61.5);
    }
    let base=fixture(DateSystem::Excel1904,164,false);
    for cell in [r#"<c r="A1" s="1"/>"#,r#"<c r="A1" s="1" t="b"><v>0</v></c>"#,r#"<c r="A1" s="1" t="inlineStr"><is><t>not a date</t></is></c>"#] {
        let source=changed(&base,"/xl/worksheets/s1.xml",r#"<c r="A1" s="1"><v>59.000</v></c>"#,cell);
        let mut s=open(&source); s.set_date_serial("Dates",a(),61.5,DateSystem::Excel1904).unwrap();
        assert_eq!(number(&s,"Report",0),62.5); preserved(&source,s.bytes());
        s.undo().unwrap(); assert_eq!(s.bytes(),source);
    }
}

#[test]
fn date_serial_validation_precedes_noop_and_preserves_redo_on_refusal() {
    let source=fixture(DateSystem::Excel1900,14,false); let mut s=open(&source);
    s.set_date_serial("Dates",a(),61.5,DateSystem::Excel1900).unwrap(); s.undo().unwrap();
    let before=state(&s);
    for serial in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,-0.5,2_958_466.0] {
        assert!(s.set_date_serial("Dates",a(),serial,DateSystem::Excel1900).is_err()); assert_eq!(state(&s),before);
    }
    assert!(s.set_date_serial("Dates",a(),59.0,DateSystem::Excel1904).is_err()); assert_eq!(state(&s),before);
    assert_eq!(s.set_date_serial("Dates",a(),59.0,DateSystem::Excel1900).unwrap(),EditOutcome::Unchanged);
    assert_eq!(state(&s),before); assert_eq!(s.bytes(),source); s.redo().unwrap();
    assert_eq!(number(&s,"Dates",0),61.5);
    let mut other=open(&fixture(DateSystem::Excel1904,14,false)); let before=state(&other);
    assert!(other.set_date_serial("Dates",a(),2_957_004.0,DateSystem::Excel1904).is_err()); assert_eq!(state(&other),before);
}

#[test]
fn date_serial_ambiguous_styles_and_unsupported_targets_refuse_atomically() {
    let source=fixture(DateSystem::Excel1900,164,false);
    let variants = [
        ("/xl/worksheets/s1.xml", " s=\"1\"", ""),
        ("/xl/worksheets/s1.xml", " s=\"1\"", " s=\"99\""),
        ("/xl/styles.xml", "applyNumberFormat=\"1\"", "applyNumberFormat=\"0\""),
        ("/xl/styles.xml", " applyNumberFormat=\"1\"", ""),
        ("/xl/styles.xml", "xfId=\"0\" applyNumberFormat", "xfId=\"1\" applyNumberFormat"),
        ("/xl/styles.xml", "yyyy-mm-dd", "dd/mm/yyyy"),
        ("/xl/styles.xml", "cellXfs count=\"2\"", "cellXfs count=\"3\""),
        ("/xl/styles.xml", "applyNumberFormat=\"1\"/>", "applyNumberFormat=\"1\"><alignment/></xf>"),
        ("/xl/styles.xml", "applyNumberFormat=\"1\"", "applyNumberFormat=\"1\" quotePrefix=\"1\""),
        ("/xl/styles.xml", "</styleSheet>", "<extLst/></styleSheet>"),
        ("/xl/styles.xml", "</styleSheet>", "<unknown/></styleSheet>"),
        ("/xl/styles.xml", "<cellXfs count=\"2\">", "<cellXfs count=\"2\">unknown"),
        ("/xl/styles.xml", "</numFmts>", "<numFmt numFmtId=\"164\" formatCode=\"yyyy-mm-dd\"/></numFmts>"),
        ("/xl/styles.xml", "numFmtId=\"164\"", "numFmtId=\"14\""),
        ("/xl/_rels/workbook.xml.rels", "relationships/styles", "relationships/notStyles"),
        ("/xl/_rels/workbook.xml.rels", "</Relationships>", "<Relationship Id=\"dup\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>"),
        ("/xl/worksheets/s1.xml", "</worksheet>", "<conditionalFormatting sqref=\"A1\"/></worksheet>"),
        ("/xl/worksheets/s1.xml", "</worksheet>", "<tableParts count=\"0\"/></worksheet>"),
        ("/xl/worksheets/s1.xml", "</worksheet>", "<mergeCells count=\"1\"><mergeCell ref=\"A1:B1\"/></mergeCells></worksheet>"),
        ("/xl/worksheets/s1.xml", "<v>59.000</v>", "<f>59</f><v>59.000</v>"),
        ("/xl/worksheets/s1.xml", "<c r=\"A1\" s=\"1\"><v>59.000</v></c>", "<c r=\"A1\" s=\"1\" t=\"e\"><v>#DIV/0!</v></c>"),
    ];
    for (part, from, to) in variants {
        let bytes=changed(&source,part,from,to); let mut s=open(&bytes); let before=state(&s);
        assert!(s.set_date_serial("Dates",a(),61.5,DateSystem::Excel1900).is_err(),"{part}: {to}"); assert_eq!(state(&s),before);
        // Bad format/system admission must also refuse an equal-value intent.
        if part == "/xl/styles.xml" { assert!(s.set_date_serial("Dates",a(),59.0,DateSystem::Excel1900).is_err()); assert_eq!(state(&s),before); }
    }
    for format in [0,18,19,20,21,46,170] {
        let mut s=open(&fixture(DateSystem::Excel1900,format,false)); let before=state(&s);
        assert!(s.set_date_serial("Dates",a(),59.0,DateSystem::Excel1900).is_err()); assert_eq!(state(&s),before);
    }
}

#[test]
fn date_serial_history_and_xml_limits_refuse_without_state_change() {
    let source=fixture(DateSystem::Excel1900,14,false);
    let mut s=WorkbookSession::from_bytes(&source,&WorkbookReadLimits::default(),HistoryBudget { max_frames:1,..HistoryBudget::default() }).unwrap();
    let before=state(&s); assert!(matches!(s.set_date_serial("Dates",a(),61.5,DateSystem::Excel1900),Err(SessionError::HistoryBudget))); assert_eq!(state(&s),before);
    let huge=changed(&source,"/xl/styles.xml","</styleSheet>",&format!("<!--{}--></styleSheet>","x".repeat(2*1024*1024)));
    let mut s=open(&huge); let before=state(&s);
    assert!(s.set_date_serial("Dates",a(),61.5,DateSystem::Excel1900).is_err()); assert_eq!(state(&s),before);
}

#[cfg(windows)]
#[test]
fn date_serial_windows_save_reopen_preserves_source_style_system_and_savepoint() {
    for system in [DateSystem::Excel1900,DateSystem::Excel1904] {
        let directory=tempfile::tempdir().unwrap(); let source=fixture(system,164,false);
        let input=directory.path().join("source.xlsx"); let output=directory.path().join("saved.xlsx");
        std::fs::write(&input,&source).unwrap(); let mut s=WorkbookSession::open(&input,&WorkbookReadLimits::default(),HistoryBudget::default()).unwrap();
        s.set_date_serial("Dates",a(),60.5,system).unwrap(); s.save_as_new(&output).unwrap(); assert!(!s.is_dirty());
        assert_eq!(std::fs::read(&input).unwrap(),source); let saved=std::fs::read(&output).unwrap(); preserved(&source,&saved);
        let reopened=open(&saved); assert_eq!(number(&reopened,"Dates",0),60.5); assert_eq!(number(&reopened,"Report",0),61.5);
        s.undo().unwrap(); assert!(s.is_dirty()); s.redo().unwrap(); assert!(!s.is_dirty());
        let before=state(&s); assert!(s.save_as_new(&output).is_err()); assert_eq!(state(&s),before); assert_eq!(std::fs::read(&output).unwrap(),saved);
    }
}
