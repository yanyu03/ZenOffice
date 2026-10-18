use zen_engine::workbook::CellAddress;
use zen_formats::spreadsheet::{ReadWorkbook, WorkbookReadLimits};
fn fixture() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../RCD/red-team-corpus/office-targets-2026-10/xlsx/zen_spreadsheet_multiscenario.xlsx",
    ))
    .unwrap()
}
#[test]
fn existing_number_edit_preserves_styles_chart_and_formula_definitions() {
    let bytes = fixture();
    let read = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default()).unwrap();
    let out = read
        .write_number_edit("Transactions", CellAddress::new(3, 3).unwrap(), 42.0)
        .unwrap();
    let a = zen_opc::ZipArchive::read(&bytes).unwrap();
    let b = zen_opc::ZipArchive::read(&out).unwrap();
    assert_eq!(a.names().len(), b.names().len());
    for e in &a.entries {
        if ![
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
            "xl/charts/chart1.xml",
        ]
        .contains(&e.name.as_str())
        {
            assert_eq!(b.get(&e.name), Some(e.data.as_slice()), "{}", e.name);
        }
    }
    let reopened = ReadWorkbook::read(&out, &WorkbookReadLimits::default()).unwrap();
    for (before, after) in read
        .workbook()
        .sheets()
        .iter()
        .zip(reopened.workbook().sheets())
    {
        for (addr, cell) in before.cells() {
            assert_eq!(
                cell.formula.as_ref().map(|f| &f.text),
                after.cell(*addr).unwrap().formula.as_ref().map(|f| &f.text)
            );
        }
    }
    let chart = std::str::from_utf8(b.get("xl/charts/chart1.xml").unwrap()).unwrap();
    assert!(chart.contains("1587.9"));
    assert!(
        read.write_number_edit("Transactions", CellAddress::new(3, 5).unwrap(), 1.0)
            .is_err()
    );
    assert!(
        read.write_number_edit("Transactions", CellAddress::new(3, 3).unwrap(), f64::NAN)
            .is_err()
    );
}
#[test]
fn protected_unknown_and_dependency_extensions_fail_closed() {
    for extra in [
        "<sheetProtection sheet=\"1\"/>",
        "<extLst><ext uri=\"unknown\"/></extLst>",
        "<x:dependency xmlns:x=\"urn:unknown\"/>",
    ] {
        let bytes = fixture();
        let mut pkg =
            zen_opc::Package::from_archive(&zen_opc::ZipArchive::read(&bytes).unwrap()).unwrap();
        let part = "/xl/worksheets/sheet2.xml";
        let xml = pkg
            .get_text(part)
            .unwrap()
            .replace("</worksheet>", &format!("{extra}</worksheet>"));
        pkg.put_text(part, xml).unwrap();
        let read =
            ReadWorkbook::read(&pkg.to_bytes().unwrap(), &WorkbookReadLimits::default()).unwrap();
        assert!(
            read.write_number_edit("Transactions", CellAddress::new(3, 3).unwrap(), 42.0)
                .is_err()
        );
    }
}
#[test]
fn large_tier_remains_bounded() {
    let small = WorkbookReadLimits::default();
    let large = WorkbookReadLimits::office_standard();
    assert_eq!(small.max_cells, 100_000);
    assert_eq!(large.max_cells, 500_000);
    assert!(large.max_xml_bytes <= large.package.max_part_bytes);
}

fn budget_fixture() -> &'static [u8] {
    include_bytes!("../../../RCD/engine-acceptance/res-002-v1/budget.xlsx")
}

#[test]
fn unrelated_unchanged_error_cache_does_not_block_numeric_edit() {
    let bytes = budget_fixture();
    let read = ReadWorkbook::read(bytes, &WorkbookReadLimits::default()).unwrap();
    let out = read.write_number_edit("Budget", CellAddress::new(3, 2).unwrap(), 9800.0).unwrap();
    let before = zen_opc::ZipArchive::read(bytes).unwrap();
    let after = zen_opc::ZipArchive::read(&out).unwrap();
    for entry in &before.entries {
        if entry.name != "xl/worksheets/sheet1.xml" {
            assert_eq!(after.get(&entry.name), Some(entry.data.as_slice()), "{}", entry.name);
        }
    }
    fn error_cell(xml: &[u8]) -> String {
        let xml = std::str::from_utf8(xml).unwrap();
        let document = roxmltree::Document::parse(xml).unwrap();
        let cell = document.descendants().find(|n| n.is_element() && n.attribute("r") == Some("B12")).unwrap();
        xml[cell.range()].to_owned()
    }
    assert_eq!(error_cell(before.get("xl/worksheets/sheet1.xml").unwrap()),
               error_cell(after.get("xl/worksheets/sheet1.xml").unwrap()));
    let reopened = ReadWorkbook::read(&out, &WorkbookReadLimits::default()).unwrap();
    let sheet = &reopened.workbook().sheets()[0];
    assert_eq!(sheet.cell(CellAddress::new(7, 3).unwrap()).unwrap().value.as_ref().unwrap().text(), "400");
    assert_eq!(sheet.cell(CellAddress::new(11, 1).unwrap()).unwrap().value.as_ref().unwrap().text(), "#DIV/0!");
}

#[test]
fn changed_error_or_error_to_number_cache_remains_rejected() {
    for formula in ["B4/C4", "1+&quot;x&quot;"] {
        let archive = zen_opc::ZipArchive::read(budget_fixture()).unwrap();
        let mut package = zen_opc::Package::from_archive(&archive).unwrap();
        let part = "/xl/worksheets/sheet1.xml";
        let mut xml = package.get_text(part).unwrap().to_owned();
        let formula_range = {
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let cell = doc.descendants().find(|n| n.is_element() && n.attribute("r") == Some("B12")).unwrap();
            let f = cell.children().find(|n| n.tag_name().name() == "f").unwrap();
            assert_eq!(f.text(), Some("1/0"));
            f.children().find(|n| n.is_text()).unwrap().range()
        };
        // Keep the original namespace-qualified f element; change only its text.
        xml.replace_range(formula_range, formula);
        package.put_text(part, xml).unwrap();
        let bytes = package.to_bytes().unwrap();
        let read = ReadWorkbook::read(&bytes, &WorkbookReadLimits::default()).unwrap();
        assert_eq!(read.workbook().sheets()[0].cell(CellAddress::new(11, 1).unwrap()).unwrap()
            .formula.as_ref().unwrap().text, formula.replace("&quot;", "\""));
        assert!(read.write_number_edit("Budget", CellAddress::new(3, 2).unwrap(), 9800.0).is_err());
    }
}
