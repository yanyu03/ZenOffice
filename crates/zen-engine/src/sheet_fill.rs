//! Downward copies, numeric series and independent content/format filling.
use super::{Axis, CellCoord, CellRange, Sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillMode {
    Auto,
    Copy,
    Series,
    FormatsOnly,
    WithoutFormatting,
}

impl FillMode {
    fn uses_series(self, height: usize) -> bool {
        self == Self::Series || matches!(self, Self::Auto | Self::WithoutFormatting) && height >= 2
    }
}

impl Sheet {
    /// Strict single-cell reference, bounded before using the prototype A1 parser.
    pub fn internal_link_target(&self, address: &str) -> Result<CellCoord, &'static str> {
        let reference = address.strip_prefix('#').unwrap_or(address);
        let Some((cell, end)) = super::structure::reference_at(reference, 0) else {
            return Err("请输入本表单元格地址，如 A1 或 D20");
        };
        if end != reference.len() || cell.col >= self.max_cols || cell.row >= self.max_rows {
            return Err("链接目标无效或超出当前工作表范围");
        }
        Ok(cell)
    }
    pub fn validate_hyperlink(&self, address: &str) -> Result<(), &'static str> {
        if address.starts_with('#') {
            self.internal_link_target(address).map(|_| ())
        } else {
            Self::validate_web_link(address)
        }
    }
    pub fn hyperlink(&self, cell: CellCoord) -> Option<&str> {
        self.cells
            .get(&cell)
            .and_then(|data| data.hyperlink.as_deref())
    }
    pub fn validate_web_link(address: &str) -> Result<(), &'static str> {
        let Some((scheme, rest)) = address.split_once("://") else {
            return Err("请输入完整的 http:// 或 https:// 网页地址");
        };
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return Err("网页链接仅支持 HTTP/HTTPS 地址");
        }
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        if host.is_empty()
            || host.starts_with(':')
            || host.ends_with(':')
            || address
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        {
            return Err("网页地址格式无效，不能包含空白或反斜杠");
        }
        Ok(())
    }
    pub fn set_hyperlink(
        &mut self,
        cell: CellCoord,
        label: &str,
        address: &str,
    ) -> Result<(), &'static str> {
        self.validate_hyperlink(address)?;
        if label.trim().is_empty() || label.trim_start().starts_with('=') {
            return Err("显示文字不能为空或以等号开头；公式单元格链接稍后接入");
        }
        let entry = self.cells.entry(cell).or_default();
        entry.raw = label.to_owned();
        entry.hyperlink = Some(address.to_owned());
        entry.style.text_color = [5, 99, 193];
        entry.style.underline = true;
        self.recalculate();
        Ok(())
    }
    pub fn remove_hyperlink(&mut self, cell: CellCoord) {
        if let Some(data) = self.cells.get_mut(&cell) {
            data.hyperlink = None;
        }
    }
    pub fn fill_series_available(&self, source: CellRange) -> bool {
        (source.start.col..=source.end.col).all(|col| self.numeric_step(source, col).is_some())
    }
    fn numeric_step(&self, source: CellRange, col: usize) -> Option<(f64, f64)> {
        let mut values = Vec::new();
        for row in source.start.row..=source.end.row {
            let raw = self.get_raw(CellCoord::new(col, row)).trim();
            if raw.is_empty() || raw.starts_with('=') {
                return None;
            }
            let value = raw.parse::<f64>().ok().filter(|value| value.is_finite())?;
            values.push(value);
        }
        let first = *values.first()?;
        let step = if values.len() == 1 {
            1.0
        } else {
            values[1] - first
        };
        if !step.is_finite() {
            return None;
        }
        for (index, value) in values.iter().enumerate() {
            let expected = first + index as f64 * step;
            if !expected.is_finite()
                || (value - expected).abs() > 1e-9 * value.abs().max(expected.abs()).max(1.0)
            {
                return None;
            }
        }
        Some((first, step))
    }
    pub fn validate_fill_down(
        &self,
        source: CellRange,
        end_row: usize,
        mode: FillMode,
    ) -> Result<(), &'static str> {
        if source.start.col > source.end.col
            || source.start.row > source.end.row
            || source.end.col >= self.max_cols
            || source.end.row >= self.max_rows
            || end_row <= source.end.row
            || end_row >= self.max_rows
        {
            return Err("请从选区右下角向下拖到工作表中的目标行");
        }
        let all = CellRange::new(source.start, CellCoord::new(source.end.col, end_row));
        if self.merged_ranges.iter().any(|range| range.intersects(all)) {
            return Err("填充范围含合并单元格，请先取消合并");
        }
        if (all.start.row..=all.end.row).any(|row| self.axis_hidden(Axis::Row, row))
            || (all.start.col..=all.end.col).any(|col| self.axis_hidden(Axis::Column, col))
        {
            return Err("本轮填充暂不跨越隐藏行列，请先取消隐藏");
        }
        if mode != FillMode::FormatsOnly
            && source.coords().any(|coord| {
                let raw = self.get_raw(coord).trim();
                raw.starts_with('=')
                    && raw
                        .replace("#REF!", "")
                        .chars()
                        .any(|c| matches!(c, '!' | '$' | '[' | ']' | '"' | '\''))
            })
        {
            return Err("本轮只支持普通 A1 公式的相对引用填充，复杂引用暂不支持");
        }
        if mode == FillMode::Series && !self.fill_series_available(source) {
            return Err("填充序列需要单个数值或各列的等差数字样本");
        }
        if mode.uses_series(source.end.row - source.start.row + 1) {
            for col in source.start.col..=source.end.col {
                if let Some((first, step)) = self.numeric_step(source, col) {
                    if !(first + (end_row - source.start.row) as f64 * step).is_finite() {
                        return Err("填充序列超出可表示的数值范围");
                    }
                }
            }
        }
        Ok(())
    }
    pub fn fill_down(
        &mut self,
        source: CellRange,
        end_row: usize,
        mode: FillMode,
    ) -> Result<usize, &'static str> {
        self.validate_fill_down(source, end_row, mode)?;
        let height = source.end.row - source.start.row + 1;
        let mut writes = Vec::new();
        for col in source.start.col..=source.end.col {
            let series = if mode.uses_series(height) {
                self.numeric_step(source, col)
            } else {
                None
            };
            for row in source.end.row + 1..=end_row {
                let from =
                    CellCoord::new(col, source.start.row + (row - source.start.row) % height);
                let source_cell = self.cells.get(&from).cloned().unwrap_or_default();
                let target = CellCoord::new(col, row);
                // Formatting-only preserves target formulas, cached values and hyperlinks.
                if mode == FillMode::FormatsOnly {
                    let mut cell = self.cells.get(&target).cloned().unwrap_or_default();
                    cell.style = source_cell.style;
                    cell.format = source_cell.format;
                    writes.push((target, cell));
                    continue;
                }
                let mut cell = source_cell;
                if mode == FillMode::WithoutFormatting {
                    let destination = self.cells.get(&target).cloned().unwrap_or_default();
                    cell.style = destination.style;
                    cell.format = destination.format;
                }
                if let Some((first, step)) = series {
                    cell.raw = (first + (row - source.start.row) as f64 * step).to_string();
                } else if cell.raw.trim_start().starts_with('=') {
                    cell.raw =
                        translate_formula(&cell.raw, row as i64 - from.row as i64, self.max_rows);
                }
                writes.push((target, cell));
            }
        }
        let count = writes.len();
        for (coord, cell) in writes {
            self.cells.insert(coord, cell);
        }
        if mode != FillMode::FormatsOnly {
            self.recalculate();
        }
        Ok(count)
    }
}

fn translate_formula(raw: &str, delta_row: i64, rows: usize) -> String {
    let mut result = String::new();
    let mut index = 0;
    while index < raw.len() {
        if let Some((mut coord, end)) = super::structure::reference_at(raw, index) {
            let row = coord.row as i64 + delta_row;
            if row < 0 || row >= rows as i64 {
                result.push_str("#REF!");
            } else {
                coord.row = row as usize;
                result.push_str(&coord.to_a1());
            }
            index = end;
        } else {
            let ch = raw[index..].chars().next().unwrap();
            result.push(ch);
            index += ch.len_utf8();
        }
    }
    result
}
