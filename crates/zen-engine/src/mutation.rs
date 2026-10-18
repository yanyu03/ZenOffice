//! # zen-engine :: mutation
//!
//! 事务级撤销/重做模块 (ZEN-007)
//!
//! 提供 [`SheetMutation`] 枚举（每种可撤销操作的变更描述），以及
//! [`UndoStack`]（双栈撤销/重做调用栈，容量上限 128 步）。
//!
//! ## 设计原则
//! - 所有变更携带充足的回滚信息（old_raw / old_cells 等）。
//! - `apply` 直接作用于 `&mut Sheet`；`unapply` 执行逆操作。
//! - 无外部依赖，纯内存操作。

use crate::calc::CellCoord;
use crate::sheet::{CellRange, CellStyle, HorizontalAlign, NumberFormat, Sheet};

/// 一次可撤销的工作表变更。
#[derive(Debug, Clone)]
pub enum SheetMutation {
    /// 设置单个单元格内容
    SetCell {
        coord: CellCoord,
        old_raw: String,
        new_raw: String,
    },
    /// 设置单元格数字格式
    SetFormat {
        coord: CellCoord,
        old_fmt: NumberFormat,
        new_fmt: NumberFormat,
    },
    /// 设置单元格对齐方式
    SetAlign {
        coord: CellCoord,
        old_align: HorizontalAlign,
        new_align: HorizontalAlign,
    },
    /// 设置单元格完整样式（用于 Bold/Italic/Underline）
    SetStyle {
        coord: CellCoord,
        old_style: CellStyle,
        new_style: CellStyle,
    },
    /// 在 `row` 行前插入新行
    InsertRow { row: usize },
    /// 删除 `row` 行（携带旧数据用于撤销）
    DeleteRow {
        row: usize,
        old_cells: Vec<(CellCoord, String)>,
    },
    /// 在 `col` 列前插入新列
    InsertCol { col: usize },
    /// 删除 `col` 列（携带旧数据用于撤销）
    DeleteCol {
        col: usize,
        old_cells: Vec<(CellCoord, String)>,
    },
    /// 矩阵粘贴（Tab 分隔 TSV）
    PasteMatrix {
        start: CellCoord,
        old_cells: Vec<(CellCoord, String)>,
        new_tsv: String,
    },
    /// 清除选区内容
    ClearRange {
        range: CellRange,
        old_cells: Vec<(CellCoord, String)>,
    },
    /// 对选区按列排序（old_cells 保存排序前快照）
    SortRange {
        range: CellRange,
        sort_col: usize,
        ascending: bool,
        old_cells: Vec<(CellCoord, String)>,
    },
}

impl SheetMutation {
    /// 将本次变更应用到 sheet。
    pub fn apply(&self, sheet: &mut Sheet) {
        match self {
            SheetMutation::SetCell { coord, new_raw, .. } => {
                sheet.set_cell(*coord, new_raw);
            }
            SheetMutation::SetFormat { coord, new_fmt, .. } => {
                sheet.set_format(*coord, *new_fmt);
            }
            SheetMutation::SetAlign { coord, new_align, .. } => {
                sheet.set_alignment(*coord, *new_align);
            }
            SheetMutation::SetStyle { coord, new_style, .. } => {
                let s = new_style.clone();
                sheet.update_styles(&[*coord], |style| *style = s.clone());
            }
            SheetMutation::InsertRow { row } => {
                sheet.insert_row(*row);
            }
            SheetMutation::DeleteRow { row, .. } => {
                let _ = sheet.delete_row(*row);
            }
            SheetMutation::InsertCol { col } => {
                sheet.insert_col(*col);
            }
            SheetMutation::DeleteCol { col, .. } => {
                let _ = sheet.delete_col(*col);
            }
            SheetMutation::PasteMatrix { start, new_tsv, .. } => {
                sheet.paste_tsv(*start, new_tsv);
            }
            SheetMutation::ClearRange { range, .. } => {
                let coords: Vec<_> = range.coords().collect();
                sheet.clear_contents(&coords);
            }
            SheetMutation::SortRange { range, sort_col, ascending, .. } => {
                sheet.sort_range(*range, *sort_col, *ascending);
            }
        }
    }

    /// 撤销本次变更（逆操作）。
    pub fn unapply(&self, sheet: &mut Sheet) {
        match self {
            SheetMutation::SetCell { coord, old_raw, .. } => {
                sheet.set_cell(*coord, old_raw);
            }
            SheetMutation::SetFormat { coord, old_fmt, .. } => {
                sheet.set_format(*coord, *old_fmt);
            }
            SheetMutation::SetAlign { coord, old_align, .. } => {
                sheet.set_alignment(*coord, *old_align);
            }
            SheetMutation::SetStyle { coord, old_style, .. } => {
                let s = old_style.clone();
                sheet.update_styles(&[*coord], |style| *style = s.clone());
            }
            SheetMutation::InsertRow { row } => {
                let _ = sheet.delete_row(*row);
            }
            SheetMutation::DeleteRow { row, old_cells } => {
                sheet.insert_row(*row);
                for (coord, raw) in old_cells {
                    sheet.set_cell(*coord, raw);
                }
            }
            SheetMutation::InsertCol { col } => {
                let _ = sheet.delete_col(*col);
            }
            SheetMutation::DeleteCol { col, old_cells } => {
                sheet.insert_col(*col);
                for (coord, raw) in old_cells {
                    sheet.set_cell(*coord, raw);
                }
            }
            SheetMutation::PasteMatrix { old_cells, .. }
            | SheetMutation::ClearRange { old_cells, .. }
            | SheetMutation::SortRange { old_cells, .. } => {
                for (coord, raw) in old_cells {
                    sheet.set_cell(*coord, raw);
                }
                sheet.recalculate();
            }
        }
    }
}

/// 双栈撤销/重做调用栈，容量上限 128 步。
#[derive(Debug, Default)]
pub struct UndoStack {
    undo: Vec<SheetMutation>,
    redo: Vec<SheetMutation>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// 推入一次新操作（清空 redo 栈）。
    pub fn push(&mut self, m: SheetMutation) {
        self.undo.push(m);
        if self.undo.len() > 128 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// 撤销最近一次操作，返回是否成功。
    pub fn undo(&mut self, sheet: &mut Sheet) -> bool {
        if let Some(m) = self.undo.pop() {
            m.unapply(sheet);
            self.redo.push(m);
            true
        } else {
            false
        }
    }

    /// 重做最近一次被撤销的操作，返回是否成功。
    pub fn redo(&mut self, sheet: &mut Sheet) -> bool {
        if let Some(m) = self.redo.pop() {
            m.apply(sheet);
            self.undo.push(m);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
