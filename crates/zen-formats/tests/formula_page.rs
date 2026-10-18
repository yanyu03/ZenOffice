use std::io::{Cursor, Read};
use zen_engine::{
    calc::CellCoord,
    sheet::{BasicFunction, PageOrientation, PaperSize, Sheet},
};
use zen_formats::{OutputFormat, export_sheet};

#[test]
fn wizard_formula_cache_and_all_paper_orientations_export() {
    for paper in PaperSize::ALL {
        for orientation in [PageOrientation::Portrait, PageOrientation::Landscape] {
            let mut sheet = Sheet::new("日常办公", 8, 8);
            sheet.set_cell(CellCoord::new(0, 0), "10");
            sheet.set_cell(CellCoord::new(0, 1), "20");
            let (mut candidate, _, _) = sheet
                .function_candidate(CellCoord::new(1, 3), BasicFunction::Sum, "A1:A2")
                .unwrap();
            candidate.set_page_orientation(orientation);
            candidate.set_paper_size(paper);
            let result = export_sheet(&candidate, OutputFormat::Xlsx).unwrap();
            let mut zip = zip::ZipArchive::new(Cursor::new(result.bytes)).unwrap();
            let mut xml = String::new();
            zip.by_name("xl/worksheets/sheet1.xml")
                .unwrap()
                .read_to_string(&mut xml)
                .unwrap();
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let page = doc
                .descendants()
                .find(|n| n.has_tag_name("pageSetup"))
                .unwrap();
            assert_eq!(
                page.attribute("paperSize"),
                Some(paper.ooxml_id().to_string().as_str())
            );
            assert_eq!(
                page.attribute("orientation"),
                Some(if orientation == PageOrientation::Landscape {
                    "landscape"
                } else {
                    "portrait"
                })
            );
            let cell = doc
                .descendants()
                .find(|n| n.has_tag_name("c") && n.attribute("r") == Some("B4"))
                .unwrap();
            assert_eq!(
                cell.children()
                    .find(|n| n.has_tag_name("f"))
                    .unwrap()
                    .text(),
                Some("SUM(A1:A2)")
            );
            assert_eq!(
                cell.children()
                    .find(|n| n.has_tag_name("v"))
                    .unwrap()
                    .text(),
                Some("30")
            );
        }
    }
}

#[test]
fn text_export_keeps_result_and_warns_about_page_settings() {
    let mut sheet = Sheet::new("日常办公", 8, 8);
    sheet.set_cell(CellCoord::new(0, 0), "10");
    let (candidate, _, _) = sheet
        .function_candidate(CellCoord::new(1, 0), BasicFunction::Sum, "A1,20")
        .unwrap();
    for format in [OutputFormat::Csv, OutputFormat::Tsv] {
        let result = export_sheet(&candidate, format).unwrap();
        let text = String::from_utf8(result.bytes).unwrap();
        assert!(text.contains("30") && !text.contains("SUM("));
        assert!(result.warnings.iter().any(|w| w.contains("页面设置")));
    }
}
