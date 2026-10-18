use std::path::PathBuf;
use zen_formats::{export_sheet, OutputFormat};
use zen_engine::calc::CellCoord;
use zen_engine::sheet::Sheet;
use zen_word::{Document as WordDoc, Paragraph as WordPara, Run as WordRun, Block as WordBlock, read_docx, write_docx};
use zen_slide::{Presentation as SlidePres, Slide, Shape as SlideShape, Paragraph as SlidePara, read_pptx_bytes, write_pptx, EMU_PER_INCH, SLIDE_WIDTH_16_9};
use zen_opc::ZipArchive;

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

#[test]
fn test_trinity_word_docx_generation_and_roundtrip() {
    let mut doc = WordDoc::default();
    doc.core.title = Some("Zen WordprocessingML Trinity Spec".into());
    doc.core.creator = Some("Zen Contributor".into());

    // 标题段落
    let mut title_p = WordPara::default();
    title_p.runs.push(WordRun::text("Zen Word Headless MVP\n"));
    doc.blocks.push(WordBlock::Paragraph(title_p));

    // 正文段落
    let mut body_p = WordPara::default();
    body_p.runs.push(WordRun::text("This document is generated strictly following ECMA-376 Part 1 Chapter 17."));
    doc.blocks.push(WordBlock::Paragraph(body_p));

    // 序列化为 DOCX
    let docx_bytes = write_docx(&doc).expect("DOCX write should succeed");
    assert!(docx_bytes.starts_with(b"PK\x03\x04"), "DOCX must be valid ZIP");

    let out_path = temp_file("zen_trinity_test.docx");
    std::fs::write(&out_path, &docx_bytes).expect("Write to disk should succeed");

    // 反序列化读取
    let outcome = read_docx(&docx_bytes).expect("DOCX read should succeed");
    assert_eq!(outcome.document.core.title.as_deref(), Some("Zen WordprocessingML Trinity Spec"));
    assert_eq!(outcome.document.blocks.len(), 2);

    // OPC 结构校验
    let archive = ZipArchive::read(&docx_bytes).expect("Zip read should succeed");
    assert!(archive.get("[Content_Types].xml").is_some());
    assert!(archive.get("_rels/.rels").is_some());
    assert!(archive.get("word/document.xml").is_some());
}

#[test]
fn test_trinity_sheet_xlsx_generation_and_evaluation() {
    let mut sheet = Sheet::new("TrinitySheet", 20, 10);

    // 写入单元格与公式 (使用 from_a1 准确映射)
    sheet.set_cell(CellCoord::from_a1("A1").unwrap(), "Product");
    sheet.set_cell(CellCoord::from_a1("B1").unwrap(), "Cost");
    sheet.set_cell(CellCoord::from_a1("A2").unwrap(), "Core Engine");
    sheet.set_cell(CellCoord::from_a1("B2").unwrap(), "150");
    sheet.set_cell(CellCoord::from_a1("A3").unwrap(), "Trinity Layer");
    sheet.set_cell(CellCoord::from_a1("B3").unwrap(), "350");
    sheet.set_cell(CellCoord::from_a1("A4").unwrap(), "Total");
    sheet.set_cell(CellCoord::from_a1("B4").unwrap(), "=SUM(B2:B3)");

    // 执行公式求值
    sheet.recalculate();

    // 验证公式求值结果 (B4 = 150 + 350 = 500)
    let display_val = sheet.get_formatted_display(CellCoord::from_a1("B4").unwrap());
    assert_eq!(display_val, "500");

    // 导出为标准 XLSX
    let export_res = export_sheet(&sheet, OutputFormat::Xlsx).expect("XLSX export should succeed");
    let xlsx_bytes = export_res.bytes;
    assert!(xlsx_bytes.starts_with(b"PK\x03\x04"), "XLSX must be valid ZIP");

    let out_path = temp_file("zen_trinity_test.xlsx");
    std::fs::write(&out_path, &xlsx_bytes).expect("Write to disk should succeed");

    // OPC 结构校验
    let archive = ZipArchive::read(&xlsx_bytes).expect("Zip read should succeed");
    assert!(archive.get("[Content_Types].xml").is_some());
    assert!(archive.get("_rels/.rels").is_some());
    assert!(archive.get("xl/workbook.xml").is_some());
    assert!(archive.get("xl/worksheets/sheet1.xml").is_some());
}

#[test]
fn test_trinity_slide_pptx_generation_and_roundtrip() {
    let mut pres = SlidePres::new("Zen PresentationML Trinity Spec");
    pres.author = "Zen Contributor".into();

    let mut slide1 = Slide::with_title("Zen Slide Headless MVP");
    let mut box1 = SlideShape::textbox(
        EMU_PER_INCH,
        EMU_PER_INCH * 2,
        SLIDE_WIDTH_16_9 - 2 * EMU_PER_INCH,
        EMU_PER_INCH,
    );
    box1.name = "DescBox".into();
    box1.add_paragraph(SlidePara::body("ECMA-376 PresentationML Part 1 Chapter 19 compliant."));
    slide1.add_shape(box1);
    pres.add_slide(slide1);

    // 序列化为 PPTX
    let pptx_bytes = write_pptx(&pres).expect("PPTX write should succeed");
    assert!(pptx_bytes.starts_with(b"PK\x03\x04"), "PPTX must be valid ZIP");

    let out_path = temp_file("zen_trinity_test.pptx");
    std::fs::write(&out_path, &pptx_bytes).expect("Write to disk should succeed");

    // 反序列化读取
    let read_pres = read_pptx_bytes(&pptx_bytes).expect("PPTX read should succeed");
    assert_eq!(read_pres.title, "Zen PresentationML Trinity Spec");
    assert_eq!(read_pres.slides.len(), 1);

    // OPC 结构校验
    let archive = ZipArchive::read(&pptx_bytes).expect("Zip read should succeed");
    assert!(archive.get("[Content_Types].xml").is_some());
    assert!(archive.get("_rels/.rels").is_some());
    assert!(archive.get("ppt/presentation.xml").is_some());
    assert!(archive.get("ppt/slides/slide1.xml").is_some());
}

#[test]
fn test_trinity_opc_container_uniformity() {
    // 验证三件套全部严格遵循 ECMA-376 Part 2 (OPC) 规范
    let mut word_doc = WordDoc::default();
    word_doc.blocks.push(WordBlock::paragraph("OPC Test"));
    let docx_bytes = write_docx(&word_doc).unwrap();

    let mut sheet = Sheet::new("Sheet1", 10, 10);
    sheet.set_cell(CellCoord::new(0, 0), "OPC Test");
    let xlsx_bytes = export_sheet(&sheet, OutputFormat::Xlsx).unwrap().bytes;

    let mut pres = SlidePres::new("OPC Test");
    pres.add_slide(Slide::with_title("Slide 1"));
    let pptx_bytes = write_pptx(&pres).unwrap();

    for (name, bytes) in [
        ("Word DOCX", &docx_bytes),
        ("Sheet XLSX", &xlsx_bytes),
        ("Slide PPTX", &pptx_bytes),
    ] {
        let archive = ZipArchive::read(bytes).unwrap_or_else(|e| panic!("{name} must be readable ZIP: {e}"));
        let ct_entry = archive.get("[Content_Types].xml").unwrap_or_else(|| panic!("{name} missing [Content_Types].xml"));
        let ct_str = std::str::from_utf8(ct_entry).unwrap();
        assert!(ct_str.contains("http://schemas.openxmlformats.org/package/2006/content-types"), "{name} ContentType NS invalid");

        let rels_entry = archive.get("_rels/.rels").unwrap_or_else(|| panic!("{name} missing _rels/.rels"));
        let rels_str = std::str::from_utf8(rels_entry).unwrap();
        assert!(rels_str.contains("http://schemas.openxmlformats.org/package/2006/relationships"), "{name} Relationships NS invalid");
    }
}
