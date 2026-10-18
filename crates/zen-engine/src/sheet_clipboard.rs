//! Selective paste of an immutable local cell block. Geometry is checked before mutation.
use super::{Axis, CellData, CellRange, CellValue, Sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    Values,
    Formats,
}

#[derive(Debug, Clone)]
pub struct ClipboardBlock {
    width: usize,
    height: usize,
    cells: Vec<CellData>,
}

impl Sheet {
    pub fn copy_block(&self, range: CellRange) -> Result<ClipboardBlock, &'static str> {
        if range.start.col > range.end.col
            || range.start.row > range.end.row
            || range.end.col >= self.max_cols
            || range.end.row >= self.max_rows
        {
            return Err("复制区域无效");
        }
        let width = range.end.col - range.start.col + 1;
        let height = range.end.row - range.start.row + 1;
        if width.checked_mul(height).is_none_or(|area| area > 262_144) {
            return Err("选择性粘贴的来源最多支持 262144 个单元格");
        }
        if self.merged_ranges.iter().any(|r| r.intersects(range)) {
            return Err("选择性粘贴暂不支持包含合并单元格的来源");
        }
        Ok(ClipboardBlock {
            width,
            height,
            cells: range
                .coords()
                .map(|c| self.cells.get(&c).cloned().unwrap_or_default())
                .collect(),
        })
    }

    pub fn paste_block(
        &mut self,
        source: &ClipboardBlock,
        selection: CellRange,
        mode: PasteMode,
    ) -> Result<CellRange, &'static str> {
        if selection.start.col > selection.end.col
            || selection.start.row > selection.end.row
            || selection.end.col >= self.max_cols
            || selection.end.row >= self.max_rows
        {
            return Err("粘贴选区无效");
        }
        let target = if selection.start == selection.end {
            let col = selection
                .start
                .col
                .checked_add(source.width - 1)
                .ok_or("粘贴区域越界")?;
            let row = selection
                .start
                .row
                .checked_add(source.height - 1)
                .ok_or("粘贴区域越界")?;
            CellRange::new(selection.start, super::CellCoord::new(col, row))
        } else {
            let width = selection.end.col - selection.start.col + 1;
            let height = selection.end.row - selection.start.row + 1;
            if width % source.width != 0 || height % source.height != 0 {
                return Err("目标选区的行数和列数需分别是复制区域的整数倍，或只选一个起始单元格");
            }
            selection
        };
        if target.end.col >= self.max_cols || target.end.row >= self.max_rows {
            return Err("粘贴区域超出工作表范围，未写入任何单元格");
        }
        if self.merged_ranges.iter().any(|r| r.intersects(target)) {
            return Err("选择性粘贴暂不支持合并单元格，请先取消合并");
        }
        if (target.start.row..=target.end.row).any(|i| self.axis_hidden(Axis::Row, i))
            || (target.start.col..=target.end.col).any(|i| self.axis_hidden(Axis::Column, i))
        {
            return Err("选择性粘贴暂不支持跨隐藏行列，请先取消隐藏");
        }
        // Formats-only never interprets formulas or cached values.
        let values = if mode == PasteMode::Values {
            Some(
                source
                    .cells
                    .iter()
                    .map(|cell| match &cell.evaluated {
                        CellValue::Number(n) if n.is_finite() => Ok(n.to_string()),
                        CellValue::Empty => Ok(String::new()),
                        CellValue::Text(text) if !text.trim_start().starts_with('=') => {
                            Ok(text.clone())
                        }
                        CellValue::Text(_) => {
                            Err("当前值模型暂不支持保留以等号开头的文字，请使用普通粘贴")
                        }
                        CellValue::Error(_) | CellValue::Number(_) => {
                            Err("来源含公式错误或无效数值，本次仅数值粘贴已取消")
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };
        for coord in target.coords() {
            let index = ((coord.row - target.start.row) % source.height) * source.width
                + (coord.col - target.start.col) % source.width;
            let destination = self.cells.entry(coord).or_default();
            match mode {
                PasteMode::Values => {
                    destination.raw.clone_from(&values.as_ref().unwrap()[index]);
                    destination.hyperlink = None;
                }
                PasteMode::Formats => {
                    destination.style.clone_from(&source.cells[index].style);
                    destination.format = source.cells[index].format;
                }
            }
        }
        if mode == PasteMode::Values {
            self.recalculate();
        }
        Ok(target)
    }
}
