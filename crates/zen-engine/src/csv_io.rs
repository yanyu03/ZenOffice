use crate::{
    calc::{CellCoord, CellValue},
    sheet::Sheet,
};

/// Delimited values interchange. Formulas export as results; objects and styles require XLSX.
pub struct CsvIO;
impl CsvIO {
    pub fn export_sheet(sheet: &Sheet) -> String {
        Self::export_delimited(sheet, ',')
    }
    pub fn export_delimited(sheet: &Sheet, separator: char) -> String {
        assert!(matches!(separator, ',' | '\t'));
        let (max_row, max_col) = sheet
            .used_cells()
            .filter(|(_, d)| !d.raw.is_empty())
            .fold((0, 0), |(r, c), (coord, _)| {
                (r.max(coord.row), c.max(coord.col))
            });
        let mut out = String::new();
        for row in 0..=max_row {
            for col in 0..=max_col {
                if col > 0 {
                    out.push(separator);
                }
                let value = match sheet.get_value(CellCoord::new(col, row)) {
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Text(text) => {
                        let raw = sheet.get_raw(CellCoord::new(col, row));
                        if raw.trim_start().starts_with('=') {
                            text
                        } else {
                            raw.to_owned()
                        }
                    }
                    other => other.display_string(),
                };
                if value.contains([separator, '"', '\r', '\n']) {
                    out.push('"');
                    out.push_str(&value.replace('"', "\"\""));
                    out.push('"');
                } else {
                    out.push_str(&value);
                }
            }
            out.push_str("\r\n");
        }
        out
    }
    pub fn import_sheet(sheet: &mut Sheet, content: &str) -> Result<(), String> {
        Self::import_delimited(sheet, content, ',')
    }
    /// Transactional parse, preserving whitespace/newlines/BOM, one recalculation.
    pub fn import_delimited(
        sheet: &mut Sheet,
        content: &str,
        separator: char,
    ) -> Result<(), String> {
        if !matches!(separator, ',' | '\t') {
            return Err("仅支持 CSV 或 TSV 分隔符".into());
        }
        let mut chars = content
            .strip_prefix('\u{feff}')
            .unwrap_or(content)
            .chars()
            .peekable();
        let mut cells = Vec::new();
        let (mut row, mut col) = (0, 0);
        let mut field = String::new();
        let (mut quoted, mut closed, mut pending) = (false, false, false);
        let emit = |row: usize,
                    col: usize,
                    field: &mut String,
                    cells: &mut Vec<(CellCoord, String)>|
         -> Result<(), String> {
            if row >= sheet.max_rows || col >= sheet.max_cols {
                return Err("文本数据超出工作表行列范围，未导入任何内容".into());
            }
            cells.push((CellCoord::new(col, row), std::mem::take(field)));
            Ok(())
        };
        while let Some(ch) = chars.next() {
            pending = true;
            if quoted {
                if ch == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        quoted = false;
                        closed = true;
                    }
                } else {
                    field.push(ch);
                }
                continue;
            }
            if ch == separator {
                emit(row, col, &mut field, &mut cells)?;
                col += 1;
                closed = false;
            } else if ch == '\n' || ch == '\r' {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                emit(row, col, &mut field, &mut cells)?;
                row += 1;
                col = 0;
                closed = false;
                pending = false;
            } else if closed {
                return Err("闭合引号后出现非分隔字符，未导入任何内容".into());
            } else if ch == '"' {
                if !field.is_empty() {
                    return Err("未加引号字段中出现引号，未导入任何内容".into());
                }
                quoted = true;
            } else {
                field.push(ch);
            }
        }
        if quoted {
            return Err("字段引号未闭合，未导入任何内容".into());
        }
        if pending {
            emit(row, col, &mut field, &mut cells)?;
        }
        sheet.set_cells_bulk(&cells).map_err(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiline_quotes_bom_and_whitespace_roundtrip() {
        let mut sheet = Sheet::new("Test", 5, 5);
        let values = [
            "  中文,尾部  ",
            "说\"你好\"",
            "行一\r\n行二",
            "\t制表符",
            "1.23456789",
        ];
        sheet
            .set_cells_bulk(
                &values
                    .iter()
                    .enumerate()
                    .map(|(c, v)| (CellCoord::new(c, 0), v.to_string()))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        for delimiter in [',', '\t'] {
            let csv = format!("\u{feff}{}", CsvIO::export_delimited(&sheet, delimiter));
            let mut other = Sheet::new("Imported", 5, 5);
            CsvIO::import_delimited(&mut other, &csv, delimiter).unwrap();
            for (c, value) in values.iter().enumerate() {
                assert_eq!(other.get_raw(CellCoord::new(c, 0)), *value);
            }
        }
    }
    #[test]
    fn malformed_and_oversized_input_is_transactional() {
        let mut sheet = Sheet::new("Test", 1, 2);
        sheet.set_cell(CellCoord::new(0, 0), "原值");
        for text in [
            "new,\"unclosed",
            "new,second,extra",
            "new\r\nsecond",
            "\"ok\"bad",
            "bad\"quote",
        ] {
            assert!(CsvIO::import_sheet(&mut sheet, text).is_err());
            assert_eq!(sheet.get_raw(CellCoord::new(0, 0)), "原值");
        }
    }
    #[test]
    fn bulk_dependency_calculation_and_bounds() {
        let mut sheet = Sheet::new("Test", 5, 5);
        sheet
            .set_cells_bulk(&[
                (CellCoord::new(1, 0), "=A1*2".into()),
                (CellCoord::new(0, 0), "3.5".into()),
            ])
            .unwrap();
        assert_eq!(sheet.get_value(CellCoord::new(1, 0)).as_f64(), Some(7.0));
        assert!(
            sheet
                .set_cells_bulk(&[
                    (CellCoord::new(0, 0), "bad".into()),
                    (CellCoord::new(5, 0), "bad".into())
                ])
                .is_err()
        );
        assert_eq!(sheet.get_raw(CellCoord::new(0, 0)), "3.5");
    }
    #[test]
    fn trailing_empty_fields_empty_input_and_auto_sum() {
        let mut sheet = Sheet::new("Test", 5, 5);
        CsvIO::import_sheet(&mut sheet, "10,20,\r\n30,,").unwrap();
        assert_eq!(sheet.get_raw(CellCoord::new(0, 1)), "30");
        CsvIO::import_sheet(&mut sheet, "").unwrap();
        sheet.auto_sum_above(CellCoord::new(0, 2));
        assert_eq!(sheet.get_value(CellCoord::new(0, 2)).as_f64(), Some(40.0));
    }
}
