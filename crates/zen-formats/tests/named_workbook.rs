use zen_engine::{
    recalc::{CalculationValue, WorkbookCalculator},
    workbook::CellAddress,
};
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};
use zen_opc::ZipWriter;

fn fixture(names: &str) -> Vec<u8> {
    let mut w = ZipWriter::new();
    w.add("[Content_Types].xml",br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/s1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/s2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_vec());
    w.add("_rels/.rels",br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="root" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec());
    w.add("xl/_rels/workbook.xml.rels",br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="a" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/s1.xml"/><Relationship Id="b" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/s2.xml"/></Relationships>"#.to_vec());
    w.add("xl/workbook.xml",format!(r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Source" sheetId="7" r:id="a"/><sheet name="Report" sheetId="42" r:id="b"/></sheets><definedNames>{names}</definedNames></workbook>"#).into_bytes());
    w.add("xl/worksheets/s1.xml",br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>3</v></c></row><row r="2"><c r="A2"><f>A1*2</f><v>9999</v></c></row></sheetData></worksheet>"#.to_vec());
    w.add("xl/worksheets/s2.xml",br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="B1"><f>SUM(Values)*Rate</f><v>-9999</v></c></row></sheetData></worksheet>"#.to_vec());
    w.finish().unwrap()
}

#[test]
fn local_sheet_index_is_resolved_to_sheet_identity_and_names_drive_recalc() {
    let bytes = fixture(
        r#"<definedName name="Values">Source!$A$1:$A$2</definedName><definedName name="Rate">10</definedName><definedName name="Rate" localSheetId="1">2</definedName>"#,
    );
    let read = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default()).unwrap();
    assert_eq!(
        read.workbook().defined_names()[2]
            .local_sheet
            .unwrap()
            .get(),
        42
    );
    let mut calc = WorkbookCalculator::new(read.workbook()).unwrap();
    calc.recalculate().unwrap();
    assert_eq!(
        calc.value("Report", CellAddress::new(0, 1).unwrap()),
        Some(&CalculationValue::Number(18.0))
    );
}

#[test]
fn invalid_scope_and_duplicate_scoped_names_are_rejected() {
    for names in [
        r#"<definedName name="Rate" localSheetId="2">2</definedName>"#,
        r#"<definedName name="Rate">2</definedName><definedName name="rate">3</definedName>"#,
    ] {
        assert!(ReadWorkbook::read(&fixture(names), &WorkbookReadLimits::default()).is_err());
    }
}
