//! OOXML Transitional spreadsheet packages produced by rust_xlsxwriter.
use crate::{ExportResult, FormatError};
use image::{DynamicImage, ImageFormat, RgbaImage};
use rust_xlsxwriter::{
    Color, Format, FormatAlign, FormatBorder, FormatUnderline, Formula, Image, ObjectMovement, Url,
    Workbook,
};
use std::{collections::HashSet, io::Cursor};
use zen_engine::{
    calc::CellValue,
    sheet::{Axis, CellData, HorizontalAlign, NumberFormat, PageOrientation, Sheet, VerticalAlign},
};

fn color(rgb: [u8; 3]) -> Color {
    Color::RGB((rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32)
}
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkbookExportOptions {
    /// Opt-in delivery layout. Ordinary editor exports retain explicit geometry.
    pub fit_columns_to_content: bool,
    pub fit_print_width: bool,
    pub repeat_first_row: bool,
    pub usd_currency: bool,
}
impl WorkbookExportOptions {
    pub fn delivery() -> Self {
        Self { fit_columns_to_content:true, fit_print_width:true, repeat_first_row:true, usd_currency:false }
    }
}
fn cell_format(data: &CellData) -> Format {
    let s = &data.style;
    let mut f = Format::new()
        .set_font_name(s.font.name())
        .set_font_size(s.font_size)
        .set_font_color(color(s.text_color));
    if s.bold {
        f = f.set_bold();
    }
    if s.italic {
        f = f.set_italic();
    }
    if s.underline {
        f = f.set_underline(FormatUnderline::Single);
    }
    if let Some(rgb) = s.fill_color {
        f = f.set_background_color(color(rgb));
    }
    if s.wrap {
        f = f.set_text_wrap();
    }
    if s.alignment_set {
        f = f.set_align(match s.horizontal {
            HorizontalAlign::Left => FormatAlign::Left,
            HorizontalAlign::Center => FormatAlign::Center,
            HorizontalAlign::Right => FormatAlign::Right,
        });
    }
    f = f.set_align(match s.vertical {
        VerticalAlign::Top => FormatAlign::Top,
        VerticalAlign::Center => FormatAlign::VerticalCenter,
        VerticalAlign::Bottom => FormatAlign::Bottom,
    });
    if s.indent > 0 {
        f = f.set_indent(s.indent);
    }
    if s.borders & 1 != 0 {
        f = f.set_border_left(FormatBorder::Thin);
    }
    if s.borders & 2 != 0 {
        f = f.set_border_right(FormatBorder::Thin);
    }
    if s.borders & 4 != 0 {
        f = f.set_border_top(FormatBorder::Thin);
    }
    if s.borders & 8 != 0 {
        f = f.set_border_bottom(FormatBorder::Thin);
    }
    let decimal = if s.decimals == 0 {
        String::new()
    } else {
        format!(".{}", "0".repeat(s.decimals as usize))
    };
    f.set_num_format(match data.format {
        NumberFormat::General => "General".to_owned(),
        NumberFormat::Currency => format!("\"¥\"#,##0{decimal}"),
        NumberFormat::Percentage => format!("0{decimal}%"),
        NumberFormat::Thousands => format!("#,##0{decimal}"),
    })
}
fn cache_value(value: &CellValue) -> String {
    match value {
        CellValue::Number(n) if n.is_finite() => n.to_string(),
        CellValue::Number(_) => "#NUM!".into(),
        CellValue::Empty => String::new(),
        CellValue::Text(text) => text.clone(),
        CellValue::Error(e) => match e.as_str() {
            "DIV/0" => "#DIV/0!",
            "REF!" => "#REF!",
            "NAME?" => "#NAME?",
            "NUM!" => "#NUM!",
            "N/A" => "#N/A",
            "NULL!" => "#NULL!",
            _ => "#VALUE!",
        }
        .into(),
    }
}
fn delivery_format(data:&CellData,options:WorkbookExportOptions) -> Format {
    let format=cell_format(data);
    if options.usd_currency && data.format==NumberFormat::Currency {
        let decimal=if data.style.decimals==0 { String::new() } else { format!(".{}","0".repeat(data.style.decimals as usize)) };
        format.set_num_format(format!("\"$\"#,##0{decimal}"))
    } else { format }
}

/// Sparse cells, native styles, cached formulas and drawing relationships.
/// Multiple sheets are supported here; the editor currently saves its active sheet.
pub fn export_workbook(sheets: &[&Sheet]) -> Result<ExportResult, FormatError> {
    export_workbook_with_options(sheets,WorkbookExportOptions::default())
}
pub fn export_workbook_with_options(sheets: &[&Sheet], options:WorkbookExportOptions) -> Result<ExportResult, FormatError> {
    if sheets.is_empty() {
        return Err(FormatError::Invalid("工作簿至少需要一个工作表".into()));
    }
    let mut names = HashSet::new();
    let mut workbook = Workbook::new();
    let mut warnings = Vec::new();
    let mut text_caches = Vec::new();
    for source in sheets {
        if source.max_rows == 0
            || source.max_cols == 0
            || source.max_rows > 1_048_576
            || source.max_cols > 16_384
        {
            return Err(FormatError::Invalid("工作表尺寸超出 XLSX 范围".into()));
        }
        if !names.insert(source.name.to_lowercase()) {
            return Err(FormatError::Invalid("工作表名称重复".into()));
        }
    }

    let mut snapshots: Vec<Sheet> = sheets.iter().map(|s| (*s).clone()).collect();
    Sheet::recalculate_sheets(&mut snapshots);

    for (sheet_index, sheet) in snapshots.iter().enumerate() {
        let worksheet = workbook.add_worksheet();
        worksheet.set_name(&sheet.name)?;
        let page = sheet.page_settings();
        let [left, right, top, bottom] = page.margins.inches();
        // Header/footer spacing stays at the writer's existing defaults.
        worksheet.set_margins(left, right, top, bottom, -1.0, -1.0);
        worksheet.set_print_gridlines(page.print_gridlines);
        worksheet.set_print_headings(page.print_headings);
        worksheet.set_paper_size(page.paper.ooxml_id());
        match page.orientation {
            PageOrientation::Portrait => {
                worksheet.set_portrait();
            }
            PageOrientation::Landscape => {
                worksheet.set_landscape();
            }
        }
        worksheet.set_default_row_height_pixels(28);
        let mut fitted=std::collections::HashMap::<usize,f64>::new();
        let mut last_cell=None::<(usize,usize)>;
        if options.fit_columns_to_content || options.fit_print_width {
            for (coord,data) in sheet.used_cells() {
                last_cell=Some(last_cell.map_or((coord.row,coord.col),|(r,c)|(r.max(coord.row),c.max(coord.col))));
                if options.fit_columns_to_content {
                    let text=sheet.get_formatted_display(coord);
                    let chars=text.lines().map(|line|line.chars().map(|c|if c.is_ascii(){1.0}else{2.0}).sum::<f64>()).fold(0.0,f64::max);
                    // Account for numeric grouping separators and font metrics.
                    let grouping=if matches!(data.evaluated,CellValue::Number(_)){5.0}else{0.0};
                    let width=((chars+grouping)*data.style.font_size as f64*0.85+24.0).clamp(108.0,560.0);
                    fitted.entry(coord.col).and_modify(|w|*w=w.max(width)).or_insert(width);
                }
            }
        }
        // UI logical pixels mapped to 96-dpi spreadsheet pixels; do not use screen DPI.
        for col in 0..sheet.max_cols {
            worksheet.set_column_width_pixels(
                col as u16,
                fitted.get(&col).copied().unwrap_or_else(||sheet.axis_size(Axis::Column,col).unwrap_or(108.0) as f64).round() as u32,
            )?;
            if sheet.axis_hidden(Axis::Column, col) {
                worksheet.set_column_hidden(col as u16)?;
            }
        }
        if options.fit_print_width {
            if let Some((row,col))=last_cell {
                worksheet.set_print_area(0,0,row as u32,col as u16)?;
                if fitted.values().sum::<f64>()>720.0 { worksheet.set_landscape(); }
                worksheet.set_print_fit_to_pages(1,0);
            }
        }
        if options.repeat_first_row { worksheet.set_repeat_rows(0,0)?; }
        for row in 0..sheet.max_rows {
            if let Some(size) = sheet.axis_size(Axis::Row, row) {
                worksheet.set_row_height_pixels(row as u32, size.round() as u32)?;
            }
            if sheet.axis_hidden(Axis::Row, row) {
                worksheet.set_row_hidden(row as u32)?;
            }
        }
        // Restore typed contents after merge_range writes blanks. Covered values
        // remain in XML, although Office hides them in its merged presentation.
        for range in sheet.merged_ranges() {
            let data = CellData {
                style: sheet.style(range.start),
                format: sheet.format(range.start),
                ..Default::default()
            };
            worksheet.merge_range(
                range.start.row as u32,
                range.start.col as u16,
                range.end.row as u32,
                range.end.col as u16,
                "",
                &delivery_format(&data,options),
            )?;
            if range
                .coords()
                .any(|c| c != range.start && !sheet.get_raw(c).is_empty())
            {
                warnings.push(format!(
                    "{}!{}:{}：合并区域内非首格数据保留在 XML 中，Office 合并视图会隐藏这些值。",
                    sheet.name,
                    range.start.to_a1(),
                    range.end.to_a1()
                ));
            }
        }
        let mut formula_errors = 0;
        let mut cells: Vec<_> = sheet.used_cells().collect();
        cells.sort_unstable_by_key(|(coord, _)| (coord.row, coord.col));
        for (coord, data) in cells {
            if coord.row >= sheet.max_rows || coord.col >= sheet.max_cols {
                return Err(FormatError::Invalid("工作表中存在越界单元格".into()));
            }
            let (row, col) = (coord.row as u32, coord.col as u16);
            let mut format = delivery_format(data,options);
            if options.fit_columns_to_content && coord.row==0 { format=format.set_text_wrap(); }
            if data.raw.trim_start().starts_with('=') {
                // The writer infers cache types from text. A referenced literal
                // "TRUE" or "#REF!" is still a string, not a boolean/error.
                if let CellValue::Text(text) = &data.evaluated {
                    if crate::formula_cache::needs_string_type(text) {
                        text_caches.push((sheet_index + 1, coord.to_a1(), text.clone()));
                    }
                }
                if matches!(data.evaluated, CellValue::Error(_)) {
                    formula_errors += 1;
                }
                // AVG is an editor alias; Excel's function name is AVERAGE.
                let original = data.raw.trim();
                let expression = original.strip_prefix('=').unwrap_or(original).trim_start();
                let formula = if expression
                    .get(..3)
                    .is_some_and(|p| p.eq_ignore_ascii_case("AVG"))
                    && expression[3..].trim_start().starts_with('(')
                {
                    format!("=AVERAGE{}", expression[3..].trim_start())
                } else {
                    original.to_owned()
                };
                worksheet.write_formula_with_format(
                    row,
                    col,
                    Formula::new(formula).set_result(cache_value(&data.evaluated)),
                    &format,
                )?;
            } else if let Some(address) = &data.hyperlink {
                let target = if let Some(a1) = address.strip_prefix('#') {
                    let escaped_name = sheet.name.replace('\'', "''");
                    format!("internal:'{escaped_name}'!{a1}")
                } else {
                    address.clone()
                };
                worksheet.write_url_with_format(
                    row,
                    col,
                    Url::new(target).set_text(&data.raw),
                    &format,
                )?;
            } else {
                match &data.evaluated {
                    CellValue::Number(n) if n.is_finite() => {
                        worksheet.write_number_with_format(row, col, *n, &format)?;
                    }
                    CellValue::Empty => {
                        worksheet.write_blank(row, col, &format)?;
                    }
                    _ => {
                        worksheet.write_string_with_format(row, col, &data.raw, &format)?;
                    }
                }
            }
        }
        if formula_errors > 0 {
            warnings.push(format!("{}：{formula_errors} 个公式当前计算为错误；已保留公式与标准错误缓存，Office 打开后重新计算。", sheet.name));
        }
        for picture in sheet.pictures() {
            sheet
                .validate_picture_placement(picture.placement)
                .map_err(|e| FormatError::Invalid(e.into()))?;
            let rgba = RgbaImage::from_raw(
                picture.dimensions[0],
                picture.dimensions[1],
                picture.pixels.as_ref().clone(),
            )
            .ok_or_else(|| FormatError::Invalid("图片像素数据无效".into()))?;
            let mut png = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(rgba).write_to(&mut png, ImageFormat::Png)?;
            let image = Image::new_from_buffer(png.get_ref())?
                .set_scale_width(picture.placement.size[0] as f64 / picture.dimensions[0] as f64)
                .set_scale_height(picture.placement.size[1] as f64 / picture.dimensions[1] as f64)
                .set_alt_text(&picture.name)
                .set_object_movement(ObjectMovement::MoveButDontSizeWithCells);
            worksheet.insert_image_with_offset(
                picture.placement.anchor.row as u32,
                picture.placement.anchor.col as u16,
                &image,
                picture.placement.offset[0].round() as u32,
                picture.placement.offset[1].round() as u32,
            )?;
        }
    }
    Ok(ExportResult {
        bytes: crate::formula_cache::preserve_string_types(
            workbook.save_to_buffer()?,
            &text_caches,
        )?,
        warnings,
    })
}
