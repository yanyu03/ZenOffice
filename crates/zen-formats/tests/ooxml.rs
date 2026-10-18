#[path = "../examples/support/fixture.rs"]
mod fixture;
use roxmltree::Document;
use std::{
    io::{Cursor, Read},
    path::Path,
};
use zen_engine::{
    calc::CellCoord,
    csv_io::CsvIO,
    sheet::{CellRange, Sheet},
};
use zen_formats::{OutputFormat, export_sheet, save_atomic, xlsx::export_workbook};

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut content = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    content
}
#[test]
fn valid_package_formula_cache_and_multiple_sheets() {
    let sheets = fixture::sample_workbook();
    let result = export_workbook(&sheets.iter().collect::<Vec<_>>()).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let mut zip = zip::ZipArchive::new(Cursor::new(&result.bytes)).unwrap();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        if file.name().ends_with(".xml") || file.name().ends_with(".rels") {
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            Document::parse(&text).unwrap();
        }
    }
    for name in [
        "[Content_Types].xml",
        "_rels/.rels",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/styles.xml",
        "xl/sharedStrings.xml",
        "xl/worksheets/sheet1.xml",
        "xl/worksheets/sheet2.xml",
        "xl/drawings/drawing1.xml",
        "xl/media/image1.png",
    ] {
        assert!(zip.by_name(name).is_ok(), "{name}");
    }
    let book = part(&result.bytes, "xl/workbook.xml");
    let doc = Document::parse(&book).unwrap();
    assert_eq!(
        doc.descendants()
            .filter(|n| n.has_tag_name("sheet"))
            .count(),
        2
    );
    assert!(
        doc.descendants()
            .any(|n| n.has_tag_name("calcPr") && n.attribute("fullCalcOnLoad") == Some("1"))
    );
    let sheet = part(&result.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&sheet).unwrap();
    let d7 = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("D7"))
        .unwrap();
    assert_eq!(
        d7.children().find(|n| n.has_tag_name("f")).unwrap().text(),
        Some("SUM(D4:D6)")
    );
    assert_eq!(
        d7.children().find(|n| n.has_tag_name("v")).unwrap().text(),
        Some("21760")
    );
    assert_eq!(sheets[0].get_raw(CellCoord::new(3, 6)), "=SUM(D4:D6)");
}
#[test]
fn styles_dimensions_merges_links_and_drawing() {
    let sheets = fixture::sample_workbook();
    let result = export_workbook(&sheets.iter().collect::<Vec<_>>()).unwrap();
    let sheet = part(&result.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&sheet).unwrap();
    assert!(
        doc.descendants()
            .any(|n| n.has_tag_name("mergeCell") && n.attribute("ref") == Some("A1:D1"))
    );
    assert!(doc.descendants().any(|n| n.has_tag_name("row")
        && n.attribute("r") == Some("15")
        && n.attribute("hidden") == Some("1")));
    assert!(doc.descendants().any(|n| n.has_tag_name("row")
        && n.attribute("r") == Some("1")
        && n.attribute("ht") == Some("36")));
    assert!(
        doc.descendants()
            .any(|n| n.has_tag_name("col") && n.attribute("hidden") == Some("1"))
    );
    assert!(
        doc.descendants().any(
            |n| n.has_tag_name("hyperlink") && n.attribute("location") == Some("'销售演示'!A4")
        )
    );
    assert!(
        part(&result.bytes, "xl/worksheets/_rels/sheet1.xml.rels")
            .contains("TargetMode=\"External\"")
    );
    let styles = part(&result.bytes, "xl/styles.xml");
    for expected in [
        "宋体",
        "Times New Roman",
        "bold",
        "wrapText",
        "thin",
        "FF167859",
    ] {
        // OOXML represents bold as <b/>.
        assert!(
            styles.contains(if expected == "bold" { "<b/>" } else { expected }),
            "{expected}"
        );
    }
    let drawing = part(&result.bytes, "xl/drawings/drawing1.xml");
    assert!(drawing.contains("editAs=\"oneCell\""));
    assert!(drawing.contains("Rust RGBA 图片测试"));
    assert!(drawing.contains("cx=\"1143000\" cy=\"571500\""));
}
#[test]
fn csv_tsv_precision_and_multiline_roundtrip() {
    let sheets = fixture::sample_workbook();
    for format in [OutputFormat::Csv, OutputFormat::Tsv] {
        let result = export_sheet(&sheets[1], format).unwrap();
        assert_eq!(&result.bytes[..3], &[0xef, 0xbb, 0xbf]);
        assert_eq!(result.warnings.len(), 1);
        let mut target = Sheet::new("Read back", 12, 4);
        CsvIO::import_delimited(
            &mut target,
            std::str::from_utf8(&result.bytes).unwrap(),
            if format == OutputFormat::Csv {
                ','
            } else {
                '\t'
            },
        )
        .unwrap();
        assert_eq!(target.get_raw(CellCoord::new(1, 2)), "  中文保留空格  ");
        assert_eq!(target.get_raw(CellCoord::new(1, 3)), "中文,逗号和\"引号\"");
        assert_eq!(target.get_raw(CellCoord::new(1, 4)), "第一行\r\n第二行");
        assert_eq!(target.get_raw(CellCoord::new(1, 5)), "1.23456789");
    }
}
#[test]
fn invalid_names_bounds_and_empty_workbook_rejected() {
    assert!(export_workbook(&[]).is_err());
    for name in ["", "bad/name", "abcdefghijklmnopqrstuvwxyz123456789"] {
        assert!(export_sheet(&Sheet::new(name, 2, 2), OutputFormat::Xlsx).is_err());
    }
    assert!(export_workbook(&[&Sheet::new("Same", 2, 2), &Sheet::new("same", 2, 2)]).is_err());
    assert!(export_sheet(&Sheet::new("Bad", 0, 2), OutputFormat::Xlsx).is_err());
    let mut sheet = Sheet::new("Bad", 2, 2);
    sheet.set_cell_unchecked(CellCoord::new(2, 0), "outside");
    assert!(export_sheet(&sheet, OutputFormat::Xlsx).is_err());
}
#[test]
fn error_cache_and_numeric_merged_anchor() {
    let mut sheet = Sheet::new("Test", 5, 5);
    sheet.set_cell(CellCoord::new(0, 0), "=10*2");
    sheet
        .merge_cells(
            CellRange::new(CellCoord::new(0, 0), CellCoord::new(1, 0)),
            false,
        )
        .unwrap();
    sheet.set_cell(CellCoord::new(0, 1), "=#REF!");
    let result = export_sheet(&sheet, OutputFormat::Xlsx).unwrap();
    assert_eq!(result.warnings.len(), 1);
    let xml = part(&result.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&xml).unwrap();
    let a1 = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("A1"))
        .unwrap();
    assert_eq!(
        a1.children().find(|n| n.has_tag_name("v")).unwrap().text(),
        Some("20")
    );
    let a2 = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("A2"))
        .unwrap();
    assert_eq!(a2.attribute("t"), Some("e"));
    assert_eq!(
        a2.children().find(|n| n.has_tag_name("v")).unwrap().text(),
        Some("#REF!")
    );
}
#[test]
fn save_replaces_atomically_and_failed_path_leaves_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.xlsx");
    save_atomic(&path, b"old").unwrap();
    save_atomic(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert!(save_atomic(&dir.path().join("missing/test.xlsx"), b"bad").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert!(save_atomic(Path::new(""), b"bad").is_err());
}

#[test]
fn text_values_formula_whitespace_and_avg_alias() {
    let mut sheet = Sheet::new("Test", 8, 5);
    sheet
        .set_cells_bulk(&[
            (CellCoord::new(0, 0), "  文本前后空格  ".into()),
            (CellCoord::new(0, 1), "   ".into()),
            (CellCoord::new(1, 0), "  =A1 ".into()),
            (CellCoord::new(1, 1), "  =AVG(2,4) ".into()),
        ])
        .unwrap();
    assert_eq!(
        sheet.get_value(CellCoord::new(1, 0)),
        zen_engine::calc::CellValue::Text("  文本前后空格  ".into())
    );
    let result = export_sheet(&sheet, OutputFormat::Xlsx).unwrap();
    let strings = part(&result.bytes, "xl/sharedStrings.xml");
    assert!(strings.contains("xml:space=\"preserve\">   </t>"));
    let xml = part(&result.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&xml).unwrap();
    let b1 = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("B1"))
        .unwrap();
    assert_eq!(b1.attribute("t"), Some("str"));
    assert_eq!(
        b1.children().find(|n| n.has_tag_name("v")).unwrap().text(),
        Some("  文本前后空格  ")
    );
    let b2 = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("B2"))
        .unwrap();
    assert_eq!(
        b2.children().find(|n| n.has_tag_name("f")).unwrap().text(),
        Some("AVERAGE(2,4)")
    );
    assert_eq!(
        b2.children().find(|n| n.has_tag_name("v")).unwrap().text(),
        Some("3")
    );
}

#[test]
fn merged_covered_content_warns_and_recalculation_updates_cache() {
    let mut sheet = Sheet::new("O'Brien sales", 8, 5);
    sheet.set_cell(CellCoord::new(0, 0), "3");
    sheet.set_cell(CellCoord::new(1, 0), "=A1*2");
    sheet.set_cell(CellCoord::new(1, 2), "covered");
    sheet
        .merge_cells(
            CellRange::new(CellCoord::new(0, 2), CellCoord::new(1, 2)),
            false,
        )
        .unwrap();
    sheet
        .set_hyperlink(CellCoord::new(0, 4), "跳转", "#A1")
        .unwrap();
    let first = export_sheet(&sheet, OutputFormat::Xlsx).unwrap();
    assert_eq!(first.warnings.len(), 1);
    sheet.set_cell(CellCoord::new(0, 0), "10");
    let second = export_sheet(&sheet, OutputFormat::Xlsx).unwrap();
    let xml = part(&second.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&xml).unwrap();
    let cell = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("B1"))
        .unwrap();
    assert_eq!(
        cell.children()
            .find(|n| n.has_tag_name("v"))
            .unwrap()
            .text(),
        Some("20")
    );
    assert!(
        doc.descendants().any(|n| n.has_tag_name("hyperlink")
            && n.attribute("location") == Some("'O''Brien sales'!A1"))
    );
    assert!(part(&second.bytes, "xl/sharedStrings.xml").contains("covered"));
}

#[test]
fn referenced_boolean_and_error_literals_remain_string_caches() {
    let mut sheet = Sheet::new("Cache types", 12, 3);
    let literals = [
        "TRUE",
        "FALSE",
        "#REF!",
        "#DIV/0!",
        "#VALUE!",
        "#GETTING_DATA",
    ];
    let mut edits = Vec::new();
    for (row, literal) in literals.iter().enumerate() {
        edits.push((CellCoord::new(0, row), (*literal).into()));
        edits.push((CellCoord::new(1, row), format!("=A{}", row + 1)));
    }
    sheet.set_cells_bulk(&edits).unwrap();
    let result = export_sheet(&sheet, OutputFormat::Xlsx).unwrap();
    assert!(result.warnings.is_empty());
    let xml = part(&result.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&xml).unwrap();
    for (row, literal) in literals.iter().enumerate() {
        let reference = format!("B{}", row + 1);
        let cell = doc
            .descendants()
            .find(|n| n.has_tag_name("c") && n.attribute("r") == Some(&reference))
            .unwrap();
        assert_eq!(cell.attribute("t"), Some("str"));
        assert_eq!(
            cell.children()
                .find(|n| n.has_tag_name("v"))
                .unwrap()
                .text(),
            Some(*literal)
        );
    }
    // Repackaging must preserve the other valid XML and relationships.
    assert!(Document::parse(&part(&result.bytes, "xl/styles.xml")).is_ok());
    assert!(Document::parse(&part(&result.bytes, "xl/_rels/workbook.xml.rels")).is_ok());
}

#[test]
fn sparse_blank_formats_and_locked_destination() {
    let mut sheet = Sheet::new("Sparse", 4096, 256);
    sheet.update_styles(&[CellCoord::new(255, 4095)], |s| {
        s.fill_color = Some([128, 255, 128]);
        s.borders = 15;
    });
    let result = export_sheet(&sheet, OutputFormat::Xlsx).unwrap();
    let xml = part(&result.bytes, "xl/worksheets/sheet1.xml");
    let doc = Document::parse(&xml).unwrap();
    assert_eq!(doc.descendants().filter(|n| n.has_tag_name("c")).count(), 1);
    assert!(doc.descendants().any(|n| n.has_tag_name("c")
        && n.attribute("r") == Some("IV4096")
        && n.attribute("s").is_some()));
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.xlsx");
        std::fs::write(&path, b"old").unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(save_atomic(&path, &result.bytes).is_err());
        drop(locked);
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
    }
}
