//! Whole-row/column edits and display dimensions. All state belongs to Sheet snapshots.
use super::{CellCoord, CellData, CellRange, Sheet};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}
impl Axis {
    pub fn index(self, cell: CellCoord) -> usize {
        match self {
            Self::Row => cell.row,
            Self::Column => cell.col,
        }
    }
    fn with_index(self, mut cell: CellCoord, index: usize) -> CellCoord {
        match self {
            Self::Row => cell.row = index,
            Self::Column => cell.col = index,
        }
        cell
    }
}

impl Sheet {
    pub fn axis_len(&self, axis: Axis) -> usize {
        match axis {
            Axis::Row => self.max_rows,
            Axis::Column => self.max_cols,
        }
    }
    pub fn axis_size(&self, axis: Axis, index: usize) -> Option<f32> {
        match axis {
            Axis::Row => &self.row_heights,
            Axis::Column => &self.column_widths,
        }
        .get(&index)
        .copied()
    }
    pub fn set_axis_size(&mut self, axis: Axis, index: usize, size: Option<f32>) {
        if index >= self.axis_len(axis) {
            return;
        }
        let sizes = match axis {
            Axis::Row => &mut self.row_heights,
            Axis::Column => &mut self.column_widths,
        };
        if let Some(size) = size.filter(|v| v.is_finite() && *v > 0.0) {
            sizes.insert(index, size.clamp(1.0, 2048.0));
        } else {
            sizes.remove(&index);
        }
    }
    pub fn axis_hidden(&self, axis: Axis, index: usize) -> bool {
        match axis {
            Axis::Row => &self.hidden_rows,
            Axis::Column => &self.hidden_columns,
        }
        .contains(&index)
    }
    pub fn set_axis_hidden(
        &mut self,
        axis: Axis,
        start: usize,
        end: usize,
        hide: bool,
    ) -> Result<(), &'static str> {
        let len = self.axis_len(axis);
        let hidden = match axis {
            Axis::Row => &mut self.hidden_rows,
            Axis::Column => &mut self.hidden_columns,
        };
        if hide && (0..len).all(|i| hidden.contains(&i) || (start..=end).contains(&i)) {
            return Err("请至少保留一行和一列可见");
        }
        for index in start..=end.min(len - 1) {
            if hide {
                hidden.insert(index);
            } else {
                hidden.remove(&index);
            }
        }
        Ok(())
    }
    pub fn used_cells(&self) -> impl Iterator<Item = (CellCoord, &CellData)> {
        self.cells.iter().map(|(coord, data)| (*coord, data))
    }
    pub fn validate_axis_edit(
        &self,
        axis: Axis,
        start: usize,
        count: usize,
        insert: bool,
    ) -> Result<(), &'static str> {
        let len = self.axis_len(axis);
        if count == 0 || start > len || (!insert && start + count > len) {
            return Err("行列范围无效");
        }
        if !insert && count >= len {
            return Err("工作表必须至少保留一行和一列");
        }
        if insert
            && len + count
                > match axis {
                    Axis::Row => 4096,
                    Axis::Column => 256,
                }
        {
            return Err("当前版本最多支持 4096 行、256 列");
        }
        // Never silently rewrite unsupported references (sheets, names, string literals).
        if self.cells.values().any(|cell| {
            cell.raw.trim_start().starts_with('=')
                && cell
                    .raw
                    .replace("#REF!", "")
                    .chars()
                    .any(|c| matches!(c, '!' | '[' | ']' | '"' | '\'' | '$'))
        }) {
            return Err("公式含跨表、绝对或复杂引用，当前版本暂不支持安全移动，请先处理这些公式");
        }
        Ok(())
    }
    pub fn edit_axis(
        &mut self,
        axis: Axis,
        start: usize,
        count: usize,
        insert: bool,
    ) -> Result<(), &'static str> {
        self.validate_axis_edit(axis, start, count, insert)?;
        let len = self.axis_len(axis);
        let map = |i| map_index(i, start, count, insert);
        let source = if start > 0 { start - 1 } else { 0 };
        let inherited: Vec<_> = if insert {
            self.cells
                .iter()
                .filter(|(c, _)| axis.index(**c) == source)
                .map(|(c, d)| (*c, d.style.clone(), d.format))
                .collect()
        } else {
            Vec::new()
        };
        self.cells = std::mem::take(&mut self.cells)
            .into_iter()
            .filter_map(|(coord, mut data)| {
                let index = map(axis.index(coord))?;
                if data.raw.trim_start().starts_with('=') {
                    data.raw = rewrite_formula(&data.raw, axis, start, count, insert);
                }
                // Internal hyperlinks follow structural edits; a deleted target is explicit.
                if let Some(address) = &data.hyperlink {
                    if let Some(reference) = address.strip_prefix('#') {
                        if let Some((target, end)) = reference_at(reference, 0) {
                            if end == reference.len() {
                                data.hyperlink = Some(
                                    map(axis.index(target))
                                        .map(|i| format!("#{}", axis.with_index(target, i).to_a1()))
                                        .unwrap_or_else(|| "#REF!".to_owned()),
                                );
                            }
                        }
                    }
                }
                Some((axis.with_index(coord, index), data))
            })
            .collect();
        for (coord, style, format) in inherited {
            for i in start..start + count {
                self.cells.insert(
                    axis.with_index(coord, i),
                    CellData {
                        style: style.clone(),
                        format,
                        ..Default::default()
                    },
                );
            }
        }
        self.merged_ranges = std::mem::take(&mut self.merged_ranges)
            .into_iter()
            .filter_map(|range| {
                let (a, b) = map_span(
                    axis.index(range.start),
                    axis.index(range.end),
                    start,
                    count,
                    insert,
                )?;
                let range = CellRange::new(
                    axis.with_index(range.start, a),
                    axis.with_index(range.end, b),
                );
                (range.start != range.end).then_some(range)
            })
            .collect();
        let remap_sizes = |sizes: &mut HashMap<usize, f32>| {
            *sizes = std::mem::take(sizes)
                .into_iter()
                .filter_map(|(i, value)| map(i).map(|i| (i, value)))
                .collect();
        };
        let remap_hidden = |hidden: &mut HashSet<usize>| {
            *hidden = std::mem::take(hidden).into_iter().filter_map(map).collect();
        };
        match axis {
            Axis::Row => {
                remap_sizes(&mut self.row_heights);
                remap_hidden(&mut self.hidden_rows);
                self.max_rows = if insert { len + count } else { len - count };
            }
            Axis::Column => {
                remap_sizes(&mut self.column_widths);
                remap_hidden(&mut self.hidden_columns);
                self.max_cols = if insert { len + count } else { len - count };
            }
        }
        let index = map(axis.index(self.active_cell))
            .unwrap_or(start)
            .min(self.axis_len(axis) - 1);
        self.active_cell = axis.with_index(self.active_cell, index);
        for picture in &mut self.pictures {
            let i = map(axis.index(picture.placement.anchor))
                .unwrap_or(start)
                .min(if insert { len + count } else { len - count } - 1);
            picture.placement.anchor = axis.with_index(picture.placement.anchor, i);
        }
        // Deleting the only visible axis must leave a usable sheet.
        if (0..self.axis_len(axis)).all(|i| self.axis_hidden(axis, i)) {
            self.set_axis_hidden(axis, index, index, false)?;
        }
        self.recalculate();
        Ok(())
    }
}

fn map_index(index: usize, start: usize, count: usize, insert: bool) -> Option<usize> {
    if index < start {
        Some(index)
    } else if insert {
        Some(index + count)
    } else if index < start + count {
        None
    } else {
        Some(index - count)
    }
}

fn map_span(
    a: usize,
    b: usize,
    start: usize,
    count: usize,
    insert: bool,
) -> Option<(usize, usize)> {
    let lo = a.min(b);
    let hi = a.max(b);
    let (first, last) = if insert {
        (
            map_index(lo, start, count, true)?,
            map_index(hi, start, count, true)?,
        )
    } else {
        let first = if (start..start + count).contains(&lo) {
            start + count
        } else {
            lo
        };
        let last = if (start..start + count).contains(&hi) {
            start.checked_sub(1)?
        } else {
            hi
        };
        if first > last || first > hi || last < lo {
            return None;
        }
        (
            map_index(first, start, count, false)?,
            map_index(last, start, count, false)?,
        )
    };
    Some(if a <= b { (first, last) } else { (last, first) })
}

// Tokenize A1 references with identifier boundaries. Ranges shrink on deletion;
// scalar references to deleted cells become #REF!. Function names remain intact.
pub(super) fn reference_at(text: &str, start: usize) -> Option<(CellCoord, usize)> {
    let bytes = text.as_bytes();
    if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        return None;
    }
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
        end += 1;
    }
    let letters = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if letters == start
        || end == letters
        || end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'('))
    {
        return None;
    }
    // Avoid unbounded column arithmetic in the prototype parser.
    if letters - start > 3 {
        return None;
    }
    Some((CellCoord::from_a1(&text[start..end])?, end))
}
fn rewrite_formula(text: &str, axis: Axis, start: usize, count: usize, insert: bool) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        if let Some((a, end)) = reference_at(text, i) {
            let mut colon = end;
            while text
                .as_bytes()
                .get(colon)
                .is_some_and(u8::is_ascii_whitespace)
            {
                colon += 1;
            }
            if text.as_bytes().get(colon) == Some(&b':') {
                let mut second = colon + 1;
                while text
                    .as_bytes()
                    .get(second)
                    .is_some_and(u8::is_ascii_whitespace)
                {
                    second += 1;
                }
                if let Some((b, tail)) = reference_at(text, second) {
                    if let Some((x, y)) =
                        map_span(axis.index(a), axis.index(b), start, count, insert)
                    {
                        result.push_str(&format!(
                            "{}:{}",
                            axis.with_index(a, x).to_a1(),
                            axis.with_index(b, y).to_a1()
                        ));
                    } else {
                        result.push_str("#REF!");
                    }
                    i = tail;
                    continue;
                }
            }
            if let Some(index) = map_index(axis.index(a), start, count, insert) {
                result.push_str(&axis.with_index(a, index).to_a1());
            } else {
                result.push_str("#REF!");
            }
            i = end;
        } else {
            let ch = text[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}
