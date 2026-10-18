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
