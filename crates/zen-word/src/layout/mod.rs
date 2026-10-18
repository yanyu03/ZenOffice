//! Zen Word 排版服务架构 (Layout Services Subsystem)
//!
//! Independent line/page/table services. Names describe responsibilities; they
//! do not establish equivalence to Office's private modules. Glyph metrics and
//! pagination remain approximate; checked layout refuses unsupported geometry.

pub mod arena;
pub mod font_metrics;
pub mod line_services;
pub mod page_table_services;

pub use arena::Arena;
pub use font_metrics::{FontLineHeight, FontLineHeightProvider, LineHeightTrace, FontLineHeightPolicy, ParsedFontLineHeight};
pub use line_services::{LayoutLine, LayoutRun, LineServices, ParagraphFlow};
pub use page_table_services::{LayoutDocument, LayoutPage, PageTableServices, PlacedTableCell};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Document, Paragraph, SectionProps, Table, TableRow};

    #[test]
    fn test_line_services_folding() {
        let text = "This is a long sentence formatted by Zen Word LineServices patterned after Microsoft Line Services MSLS70 engine. It folds cleanly across multiple lines.";
        let para = Paragraph::text(text);
        // 版心宽 3000 twips (约 2.08 英寸)
        let lines = LineServices::format_paragraph(&para, 3000);
        assert!(lines.len() >= 3, "Expected at least 3 folded lines, got {}", lines.len());
        for line in &lines {
            assert!(line.width_twips <= 3000, "Line width {} exceeded budget 3000", line.width_twips);
            assert!(line.height_twips > 0);
            assert!(line.ascent_twips > 0);
        }
    }

    #[test]
    fn test_page_table_services_pagination() {
        let mut doc = Document::default();
        doc.section = SectionProps {
            page_width_twips: 11906, // A4
            page_height_twips: 16838,
            margin_top_twips: 1440,
            margin_bottom_twips: 1440,
            margin_left_twips: 1440,
            margin_right_twips: 1440,
            ..Default::default()
        };

        // 插入 100 个段落，迫使触发多页分页
        for i in 0..100 {
            doc.blocks.push(Block::paragraph(format!(
                "Paragraph #{i:03}: Enterprise high-throughput deterministic layout engine verification."
            )));
        }

        let layout = PageTableServices::layout_document(&doc);
        assert!(layout.total_pages >= 2, "Expected multiple pages for 100 paragraphs, got {}", layout.total_pages);
        assert_eq!(layout.pages.len(), layout.total_pages);
        for page in &layout.pages {
            assert!(!page.placed_lines.is_empty());
        }
    }

    #[test]
    fn test_page_table_services_table_layout() {
        let mut doc = Document::default();
        // Keep this simple-table fixture inside the default 8306-twip body.
        let mut tbl = Table::new(vec![2500, 2500, 2500]);
        for r in 0..5 {
            tbl.rows.push(TableRow::from_texts(vec![
                format!("R{r}C0"),
                format!("R{r}C1"),
                format!("R{r}C2"),
            ]));
        }
        doc.blocks.push(Block::Table(tbl));

        let layout = PageTableServices::layout_document(&doc);
        assert_eq!(layout.total_pages, 1);
        assert_eq!(layout.pages[0].placed_cells.len(), 15);
    }
}

