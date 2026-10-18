//! Independent approximate page/table services, with explicit layout limits.
//!
//! 职责：
//! 1. 节属性 (SectionProps) 与纸张规格 (A4, Letter 等) 几何解析
//! 2. 正文流式分页断页 (Page Pagination & Page Breaking)
//! 3. 段前分页 (Page Break Before)、孤行控制与段落间距处理
//! 4. 表格网格 (Grid)、单元格宽度分配与跨页断行排版
//! 5. 组装整篇文档的多页布局结构 (LayoutDocument & LayoutPage)

use crate::layout::line_services::{LayoutLine, LineServices, ParagraphFlow};
use crate::layout::font_metrics::{FontLineHeightProvider, LineHeightTrace};
use crate::model::{Block, BreakKind, Document, Paragraph, SectionProps, SectionStart, Table};
use crate::styles::StyleResolver;
use crate::{Result, WordError};

/// 单页布局产物
#[derive(Debug, Clone)]
pub struct LayoutPage {
    pub page_number: usize,
    pub section_index: usize,
    pub printed_page_number: Option<u32>,
    pub page_number_format: Option<String>,
    pub width_twips: i32,
    pub height_twips: i32,
    pub margin_left_twips: i32,
    pub margin_top_twips: i32,
    pub margin_right_twips: i32,
    pub margin_bottom_twips: i32,
    /// 页面正文绝对坐标放置的行列表：(x, y, line)
    pub placed_lines: Vec<(i32, i32, LayoutLine)>,
    /// 页面表格单元格渲染几何块
    pub placed_cells: Vec<PlacedTableCell>,
}

#[derive(Debug, Clone)]
pub struct PlacedTableCell {
    pub x_twips: i32,
    pub y_twips: i32,
    pub width_twips: u32,
    pub height_twips: u32,
    pub lines: Vec<(i32, i32, LayoutLine)>,
}

/// 整篇文档的完整排版结果
#[derive(Debug, Clone)]
pub struct LayoutDocument {
    pub pages: Vec<LayoutPage>,
    pub total_pages: usize,
    /// Approximate preview and unsupported-operation notices; not Office parity.
    pub diagnostics: Vec<String>,
}

pub struct PageTableServices;

struct PreparedParagraph {
    paragraph: Paragraph,
    flow: Vec<ParagraphFlow>,
}

#[derive(Clone, Copy)]
struct KeepRequirement {
    height: u64,
    end: usize,
}

impl PageTableServices {
    /// 执行整篇文档的流式分页排版
    pub fn layout_document(doc: &Document) -> LayoutDocument {
        match Self::layout_document_checked(doc) {
            Ok(layout) => layout,
            Err(error) => LayoutDocument {
                pages: Vec::new(),
                total_pages: 0,
                diagnostics: vec![error.to_string()],
            },
        }
    }

    pub fn layout_document_checked(doc: &Document) -> Result<LayoutDocument> {
        Self::layout_document_impl(doc, None, &mut Vec::new())
    }

    /// Opt-in font line heights shared by body, keepNext preparation and cells.
    /// Traces are in measurement order (prepared body, then cells per section),
    /// not document-story order. Widths/baselines remain approximate.
    pub fn layout_document_with_metrics_checked(
        doc: &Document, provider: &dyn FontLineHeightProvider,
    ) -> Result<(LayoutDocument, Vec<LineHeightTrace>)> {
        let mut traces = Vec::new();
        let layout = Self::layout_document_impl(doc, Some(provider), &mut traces)?;
        Ok((layout, traces))
    }

    fn layout_document_impl(
        doc: &Document, provider: Option<&dyn FontLineHeightProvider>, traces: &mut Vec<LineHeightTrace>,
    ) -> Result<LayoutDocument> {
        doc.validate_sections()?;
        doc.settings.validate()?;
        if doc.settings.default_tab_stop_twips == 0 {
            return Err(WordError::UnsupportedEdit(
                "preview zero defaultTabStop is not implemented".into(),
            ));
        }
        let resolver = StyleResolver::new(&doc.styles)?;
        let mut style_diagnostics = Vec::new();
        let mut pages = Vec::new();
        let mut printed = 1u32;
        for section in doc.sections() {
            if section.properties.column_count != 1
                || (section.index > 0 && section.properties.start != SectionStart::NextPage)
            {
                return Err(WordError::UnsupportedEdit(format!(
                    "preview section {}: continuous, column and odd/even section pagination are not implemented",
                    section.index + 1
                )));
            }
            if let Some(start) = section.properties.page_number_start {
                printed = start;
            }
            let layout =
                Self::layout_section(&doc.blocks[section.blocks], section.properties, &resolver, provider, traces, doc.settings.default_tab_stop_twips)?;
            style_diagnostics.extend(layout.diagnostics);
            for mut page in layout.pages {
                page.page_number = pages.len() + 1;
                page.section_index = section.index;
                page.printed_page_number = Some(printed);
                page.page_number_format = section.properties.page_number_format.clone();
                printed = printed
                    .checked_add(1)
                    .ok_or_else(|| WordError::UnsupportedEdit("page numbering overflow".into()))?;
                pages.push(page);
            }
        }
        let mut diagnostics = vec!["Approximate preview: estimated glyph widths; not native Office pagination or complete story rendering.".into()];
        if doc.preservation.is_some() {
            diagnostics.push("Imported package retains objects that the preview does not render, including headers, fields, drawings and notes.".into());
        }
        style_diagnostics.sort();
        style_diagnostics.dedup();
        diagnostics.extend(style_diagnostics);
        Ok(LayoutDocument {
            total_pages: pages.len(),
            pages,
            diagnostics,
        })
    }

    fn layout_section(
        blocks: &[Block],
        sec: &SectionProps,
        resolver: &StyleResolver<'_>,
        provider: Option<&dyn FontLineHeightProvider>,
        traces: &mut Vec<LineHeightTrace>, default_tab_twips: u32,
    ) -> Result<LayoutDocument> {
        let mut diagnostics = Vec::new();
        let page_w = sec.page_width_twips;
        let page_h = sec.page_height_twips;
        let margin_l = sec.margin_left_twips;
        let margin_r = sec.margin_right_twips;
        let margin_t = sec.margin_top_twips;
        let margin_b = sec.margin_bottom_twips;

        let content_w = (i64::from(page_w)
            - i64::from(margin_l)
            - i64::from(margin_r)
            - i64::from(sec.gutter_twips)) as u32;
        let content_h = (page_h - margin_t - margin_b) as u32;
        if sec.gutter_twips != 0 {
            return Err(WordError::UnsupportedEdit(
                "preview gutter placement is not implemented".into(),
            ));
        }

        // Resolve and measure once. Reverse suffix requirements avoid repeatedly
        // walking long chains of headings. A terminal paragraph contributes only
        // its first line; its remaining text can flow onto subsequent pages.
        let mut prepared = Vec::with_capacity(blocks.len());
        for block in blocks {
            prepared.push(if let Block::Paragraph(source) = block {
                let effective = resolver.resolve_paragraph(source)?;
                diagnostics.extend(effective.diagnostics);
                let flow = LineServices::format_paragraph_flow_impl(&effective.paragraph, content_w, provider, traces, default_tab_twips);
                Some(PreparedParagraph {
                    paragraph: effective.paragraph,
                    flow,
                })
            } else {
                None
            });
        }
        let mut keep: Vec<Option<KeepRequirement>> = vec![None; blocks.len()];
        for index in (0..prepared.len()).rev() {
            let Some(current) = &prepared[index] else {
                continue;
            };
            let props = &current.paragraph.props;
            let before = u64::from(props.space_before_twips.unwrap_or(0));
            let first = match current.flow.first() {
                Some(ParagraphFlow::Lines(lines)) => lines.first(),
                _ => None,
            };
            keep[index] = first.map(|line| KeepRequirement {
                height: before + u64::from(line.fit_height_twips),
                end: index,
            });
            if !props.keep_next || index + 1 == prepared.len() {
                continue;
            }
            let Some(next) = &prepared[index + 1] else {
                diagnostics.push(
                    "Keep-with-next across a table is not implemented in approximate pagination."
                        .into(),
                );
                continue;
            };
            if next.paragraph.props.page_break_before {
                continue;
            }
            let Some(suffix) = keep[index + 1] else {
                continue;
            };
            if current
                .flow
                .iter()
                .any(|flow| !matches!(flow, ParagraphFlow::Lines(_)))
            {
                diagnostics.push(
                    "Keep-with-next spanning a hard break is relaxed in approximate pagination."
                        .into(),
                );
                continue;
            }
            let height = current
                .flow
                .iter()
                .filter_map(|flow| match flow {
                    ParagraphFlow::Lines(lines) => Some(lines),
                    _ => None,
                })
                .flatten()
                .fold(before, |sum, line| {
                    sum.saturating_add(u64::from(line.height_twips))
                })
                .saturating_add(u64::from(props.space_after_twips.unwrap_or(0)))
                .saturating_add(suffix.height);
            keep[index] = Some(KeepRequirement {
                height,
                end: suffix.end,
            });
        }

        let mut pages = Vec::new();
        let mut curr_page_num = 1;
        let mut curr_y = margin_t;

        let mut curr_placed_lines = Vec::new();
        let mut curr_placed_cells = Vec::new();

        let new_page = |num: usize,
                        lines: Vec<(i32, i32, LayoutLine)>,
                        cells: Vec<PlacedTableCell>|
         -> LayoutPage {
            LayoutPage {
                page_number: num,
                section_index: 0,
                printed_page_number: None,
                page_number_format: None,
                width_twips: page_w,
                height_twips: page_h,
                margin_left_twips: margin_l,
                margin_top_twips: margin_t,
                margin_right_twips: margin_r,
                margin_bottom_twips: margin_b,
                placed_lines: lines,
                placed_cells: cells,
            }
        };

        let mut relaxed_through = None;
        for (index, block) in blocks.iter().enumerate() {
            match block {
                Block::Paragraph(_) => {
                    let current = prepared[index].take().expect("paragraph prepared above");
                    let para = &current.paragraph;
                    // 段前分页
                    if para.props.page_break_before
                        && (!curr_placed_lines.is_empty() || !curr_placed_cells.is_empty())
                    {
                        pages.push(new_page(
                            curr_page_num,
                            std::mem::take(&mut curr_placed_lines),
                            std::mem::take(&mut curr_placed_cells),
                        ));
                        curr_page_num += 1;
                        curr_y = margin_t;
                    }

                    if let Some(required) = keep[index]
                        && required.end > index
                        && !relaxed_through.is_some_and(|end| index <= end)
                    {
                        if required.height > u64::from(content_h) {
                            // Relax the entire oversized chain once, rather than
                            // repeatedly moving its shorter suffix to fresh pages.
                            relaxed_through = Some(required.end);
                            diagnostics.push("Keep-with-next group exceeds the section body; constraints for this group are relaxed.".into());
                        } else if i64::from(curr_y) + required.height as i64
                            > i64::from(margin_t) + i64::from(content_h)
                            && (!curr_placed_lines.is_empty() || !curr_placed_cells.is_empty())
                        {
                            pages.push(new_page(
                                curr_page_num,
                                std::mem::take(&mut curr_placed_lines),
                                std::mem::take(&mut curr_placed_cells),
                            ));
                            curr_page_num += 1;
                            curr_y = margin_t;
                        }
                    }

                    // 段前距
                    let space_before = para.props.space_before_twips.unwrap_or(0);
                    if space_before > content_h {
                        return Err(WordError::UnsupportedEdit(
                            "paragraph spacing exceeds section height".into(),
                        ));
                    }
                    let space_before = space_before as i32;
                    curr_y = curr_y.saturating_add(space_before);

                    // 行级折叠排版
                    for flow in current.flow {
                        let lines = match flow {
                            ParagraphFlow::Lines(lines) => lines,
                            ParagraphFlow::Break(BreakKind::Page) => {
                                pages.push(new_page(
                                    curr_page_num,
                                    std::mem::take(&mut curr_placed_lines),
                                    std::mem::take(&mut curr_placed_cells),
                                ));
                                curr_page_num += 1;
                                curr_y = margin_t;
                                continue;
                            }
                            ParagraphFlow::Break(_) => {
                                return Err(WordError::UnsupportedEdit(
                                    "column break preview is not implemented".into(),
                                ));
                            }
                        };
                        for line in lines {
                            if line.fit_height_twips == 0 || line.fit_height_twips > line.height_twips
                                || line.fit_height_twips > content_h || line.height_twips > i32::MAX as u32
                                || line.runs.iter().any(|r| {
                                    i64::from(r.x_twips) + i64::from(r.width_twips)
                                        > i64::from(content_w)
                                })
                            {
                                return Err(WordError::UnsupportedEdit(
                                    "preview line does not fit the section body".into(),
                                ));
                            }
                            let line_h = line.height_twips as i32;
                            // 检查是否超出当前页底边距
                            if i64::from(curr_y) + i64::from(line.fit_height_twips) > i64::from(margin_t) + i64::from(content_h)
                                && (!curr_placed_lines.is_empty() || !curr_placed_cells.is_empty())
                            {
                                // 换页
                                pages.push(new_page(
                                    curr_page_num,
                                    std::mem::take(&mut curr_placed_lines),
                                    std::mem::take(&mut curr_placed_cells),
                                ));
                                curr_page_num += 1;
                                curr_y = margin_t;
                            }

                            let line_x = margin_l;
                            curr_placed_lines.push((line_x, curr_y, line));
                            curr_y = curr_y.saturating_add(line_h);
                        }
                    }

                    // 段后距
                    let space_after = para.props.space_after_twips.unwrap_or(0);
                    if space_after > content_h {
                        return Err(WordError::UnsupportedEdit(
                            "paragraph spacing exceeds section height".into(),
                        ));
                    }
                    let space_after = space_after as i32;
                    curr_y = curr_y.saturating_add(space_after);
                }
                Block::Table(tbl) => {
                    if tbl.props.style_id.is_some() {
                        diagnostics.push("Conditional table-style formatting is not resolved; cell text uses document, paragraph and character styles.".into());
                    }
                    // 表格排版
                    Self::layout_table(
                        tbl,
                        content_w,
                        margin_l,
                        margin_t,
                        content_h,
                        &mut curr_y,
                        &mut curr_page_num,
                        &mut curr_placed_lines,
                        &mut curr_placed_cells,
                        &mut pages,
                        &new_page,
                        resolver,
                        &mut diagnostics,
                        provider,
                        traces,
                        default_tab_twips,
                    )?;
                }
            }
        }

        // 压入最后一页
        pages.push(new_page(
            curr_page_num,
            curr_placed_lines,
            curr_placed_cells,
        ));

        let total_pages = pages.len();
        Ok(LayoutDocument {
            pages,
            total_pages,
            diagnostics,
        })
    }

    /// 表格流式排版
    fn layout_table<F>(
        tbl: &Table,
        content_w: u32,
        margin_l: i32,
        margin_t: i32,
        content_h: u32,
        curr_y: &mut i32,
        curr_page_num: &mut usize,
        curr_placed_lines: &mut Vec<(i32, i32, LayoutLine)>,
        curr_placed_cells: &mut Vec<PlacedTableCell>,
        pages: &mut Vec<LayoutPage>,
        new_page_fn: &F,
        resolver: &StyleResolver<'_>,
        diagnostics: &mut Vec<String>,
        provider: Option<&dyn FontLineHeightProvider>,
        traces: &mut Vec<LineHeightTrace>, default_tab_twips: u32,
    ) -> Result<()>
    where
        F: Fn(usize, Vec<(i32, i32, LayoutLine)>, Vec<PlacedTableCell>) -> LayoutPage,
    {
        if tbl.rows.is_empty() {
            return Ok(());
        }

        // 列宽计算：根据 grid 或 cell 数量均分
        let col_count = tbl
            .rows
            .iter()
            .map(|r| r.cells.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let default_col_w = content_w / col_count as u32;
        if !tbl.grid_twips.is_empty() && tbl.grid_twips.len() != col_count {
            return Err(WordError::UnsupportedEdit(
                "preview table grid does not match its rows".into(),
            ));
        }
        if tbl.props.width_twips.is_some_and(|w| w > content_w) {
            return Err(WordError::UnsupportedEdit(
                "preview table width exceeds section width".into(),
            ));
        }

        for row in &tbl.rows {
            // 计算本行内各单元格排版
            let mut row_cells = Vec::new();
            let mut max_cell_h: u32 = 0;
            let mut cell_x = margin_l;

            if row.cells.len() != col_count
                || row
                    .cells
                    .iter()
                    .any(|c| c.grid_span != 1 || c.v_merge.is_some())
            {
                return Err(WordError::UnsupportedEdit(
                    "preview merged or irregular table rows are not implemented".into(),
                ));
            }
            let mut used_width = 0u64;
            for (column, cell) in row.cells.iter().enumerate() {
                let cell_w = cell.width_twips.unwrap_or_else(|| {
                    tbl.grid_twips.get(column).copied().unwrap_or(default_col_w)
                });
                used_width += u64::from(cell_w);
                if cell_w <= 200 || used_width > u64::from(content_w) {
                    return Err(WordError::UnsupportedEdit(
                        "preview cells exceed the section width or leave no text area".into(),
                    ));
                }
                let inner_padding = 100; // 单元格内边距 (100 twips ~ 5pt)
                let inner_w = cell_w - inner_padding * 2;

                let mut cell_lines = Vec::new();
                let mut cell_curr_y = inner_padding as i32;

                for blk in &cell.blocks {
                    if let Block::Paragraph(source) = blk {
                        let effective = resolver.resolve_paragraph(source)?;
                        diagnostics.extend(effective.diagnostics);
                        let p = &effective.paragraph;
                        let before = p.props.space_before_twips.unwrap_or(0);
                        let after = p.props.space_after_twips.unwrap_or(0);
                        if before > content_h || after > content_h {
                            return Err(WordError::UnsupportedEdit(
                                "cell paragraph spacing exceeds section height".into(),
                            ));
                        }
                        cell_curr_y += before as i32;
                        let mut lines = Vec::new();
                        for flow in LineServices::format_paragraph_flow_impl(p, inner_w, provider, traces, default_tab_twips) {
                            match flow {
                                ParagraphFlow::Lines(value) => lines.extend(value),
                                _ => return Err(WordError::UnsupportedEdit("preview hard page/column breaks inside cells are not implemented".into())),
                            }
                        }
                        for line in lines {
                            if line.height_twips > content_h
                                || line.runs.iter().any(|r| {
                                    i64::from(r.x_twips) + i64::from(r.width_twips)
                                        > i64::from(inner_w)
                                })
                            {
                                return Err(WordError::UnsupportedEdit(
                                    "preview cell content does not fit".into(),
                                ));
                            }
                            let lh = line.height_twips as i32;
                            if i64::from(cell_curr_y) + i64::from(lh) + i64::from(inner_padding)
                                > i64::from(content_h)
                            {
                                return Err(WordError::UnsupportedEdit(
                                    "preview tall cell needs row splitting".into(),
                                ));
                            }
                            cell_lines.push((inner_padding as i32, cell_curr_y, line));
                            cell_curr_y += lh;
                        }
                        cell_curr_y += after as i32;
                    } else {
                        return Err(WordError::UnsupportedEdit(
                            "preview nested tables are not implemented".into(),
                        ));
                    }
                }

                let total_cell_h = (cell_curr_y as u32 + inner_padding).max(280);
                if total_cell_h > max_cell_h {
                    max_cell_h = total_cell_h;
                }

                row_cells.push((cell_x, cell_w, cell_lines));
                cell_x += cell_w as i32;
            }

            // 检查跨页
            let row_h = max_cell_h as i32;
            if max_cell_h > content_h {
                return Err(WordError::UnsupportedEdit(
                    "preview splitting a tall row across pages is not implemented".into(),
                ));
            }
            if *curr_y + row_h > margin_t + content_h as i32
                && (!curr_placed_lines.is_empty() || !curr_placed_cells.is_empty())
            {
                pages.push(new_page_fn(
                    *curr_page_num,
                    std::mem::take(curr_placed_lines),
                    std::mem::take(curr_placed_cells),
                ));
                *curr_page_num += 1;
                *curr_y = margin_t;
            }

            // 放置该行单元格
            for (cx, cw, clines) in row_cells {
                curr_placed_cells.push(PlacedTableCell {
                    x_twips: cx,
                    y_twips: *curr_y,
                    width_twips: cw,
                    height_twips: max_cell_h,
                    lines: clines,
                });
            }

            *curr_y += row_h;
        }

        // 表后微小缓冲
        *curr_y += 120;
        Ok(())
    }
}
