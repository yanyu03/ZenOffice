use crate::calc::{CellCoord, CellValue, FormulaEngine};
use std::collections::HashMap;

#[path = "sheet_structure.rs"]
mod structure;
pub use structure::Axis;
#[path = "sheet_fill.rs"]
mod fill;
pub use fill::FillMode;
#[path = "sheet_clipboard.rs"]
mod clipboard;
pub use clipboard::{ClipboardBlock, PasteMode};
#[path = "sheet_pictures.rs"]
mod pictures;
pub use pictures::{PicturePlacement, SheetPicture};
#[path = "sheet_page.rs"]
mod page;
pub use page::{PageMargins, PageOrientation, PageSettings, PaperSize};
#[path = "sheet_functions.rs"]
mod functions;
pub use functions::BasicFunction;
#[path = "sheet_statistics.rs"]
mod statistics;
pub use statistics::RangeStatistics;
#[path = "sheet_search.rs"]
mod search;
pub use search::{
    ReplacePlan, ReplaceRequest, ReplaceTarget, SearchHit, SearchLookIn, SearchOrder, SearchRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    General,
    Currency,
    Percentage,
    Thousands,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub start: CellCoord,
    pub end: CellCoord,
}

impl CellRange {
    pub fn new(a: CellCoord, b: CellCoord) -> Self {
        Self {
            start: CellCoord::new(a.col.min(b.col), a.row.min(b.row)),
            end: CellCoord::new(a.col.max(b.col), a.row.max(b.row)),
        }
    }

    pub fn contains(self, coord: CellCoord) -> bool {
        (self.start.col..=self.end.col).contains(&coord.col)
            && (self.start.row..=self.end.row).contains(&coord.row)
    }

    pub fn intersects(self, other: Self) -> bool {
        self.start.col <= other.end.col
            && self.end.col >= other.start.col
            && self.start.row <= other.end.row
            && self.end.row >= other.start.row
    }

    pub fn coords(self) -> impl Iterator<Item = CellCoord> {
        (self.start.row..=self.end.row).flat_map(move |row| {
            (self.start.col..=self.end.col).map(move |col| CellCoord::new(col, row))
        })
    }

    /// Construct a single-cell range.
    pub fn single(coord: CellCoord) -> Self {
        Self { start: coord, end: coord }
    }

    pub fn min_row(self) -> usize { self.start.row }
    pub fn max_row(self) -> usize { self.end.row }
    pub fn min_col(self) -> usize { self.start.col }
    pub fn max_col(self) -> usize { self.end.col }

    /// Format range as A1 notation, e.g. "A1:C5".
    pub fn to_a1_range(self) -> String {
        format!("{}:{}", self.start.to_a1(), self.end.to_a1())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum CellFont {
    YaHei,
    #[default]
    SimSun,
    SimHei,
    Arial,
    TimesNewRoman,
    System(String),
}

impl CellFont {
    pub fn name(&self) -> &str {
        match self {
            Self::YaHei => "微软雅黑",
            Self::SimSun => "宋体",
            Self::SimHei => "黑体",
            Self::Arial => "Arial",
            Self::TimesNewRoman => "Times New Roman",
            Self::System(name) => name,
        }
    }
    pub fn family_key(&self, bold: bool, italic: bool) -> String {
        format!(
            "cell-font:{}:{}:{}",
            self.name(),
            u8::from(bold),
            u8::from(italic)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderMode {
    None,
    All,
    Outline,
    Bottom,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RangeStats {
    pub count: usize,
    pub sum: f64,
    pub avg: f64,
    pub min: f64,
    pub max: f64,
}

// Edge bits: left, right, top, bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellStyle {
    pub font: CellFont,
    pub text_color: [u8; 3],
    pub fill_color: Option<[u8; 3]>,
    pub borders: u8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub horizontal: HorizontalAlign,
    pub alignment_set: bool,
    pub vertical: VerticalAlign,
    pub indent: u8,
    pub wrap: bool,
    pub decimals: u8,
    pub font_size: u8,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            font: CellFont::default(),
            text_color: [0x26, 0x2e, 0x3e],
            fill_color: None,
            borders: 0,
            bold: false,
            italic: false,
            underline: false,
            horizontal: HorizontalAlign::Left,
            alignment_set: false,
            vertical: VerticalAlign::Bottom,
            indent: 0,
            wrap: false,
            decimals: 2,
            font_size: 12,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormatSnapshot {
    pub style: CellStyle,
    pub format: NumberFormat,
}

#[derive(Debug, Clone)]
pub struct CellData {
    pub raw: String,
    pub evaluated: CellValue,
    pub format: NumberFormat,
    pub style: CellStyle,
    pub hyperlink: Option<String>,
}

impl Default for CellData {
    fn default() -> Self {
        Self {
            raw: String::new(),
            evaluated: CellValue::Empty,
            format: NumberFormat::General,
            style: CellStyle::default(),
            hyperlink: None,
        }
    }
}

/// In-memory Spreadsheet Sheet model.
#[derive(Clone)]
pub struct Sheet {
    pub name: String,
    page_settings: PageSettings,
    cells: HashMap<CellCoord, CellData>,
    merged_ranges: Vec<CellRange>,
    pictures: Vec<SheetPicture>,
    next_picture_id: u64,
    row_heights: HashMap<usize, f32>,
    column_widths: HashMap<usize, f32>,
    hidden_rows: std::collections::HashSet<usize>,
    hidden_columns: std::collections::HashSet<usize>,
    pub max_rows: usize,
    pub max_cols: usize,
    pub active_cell: CellCoord,
}

impl Sheet {
    pub fn new(name: &str, rows: usize, cols: usize) -> Self {
        Self {
            name: name.to_string(),
            page_settings: PageSettings::default(),
            cells: HashMap::new(),
            merged_ranges: Vec::new(),
            pictures: Vec::new(),
            next_picture_id: 1,
            row_heights: HashMap::new(),
            column_widths: HashMap::new(),
            hidden_rows: Default::default(),
            hidden_columns: Default::default(),
            max_rows: rows,
            max_cols: cols,
            active_cell: CellCoord::new(0, 0),
        }
    }

    pub fn set_cell(&mut self, coord: CellCoord, text: &str) -> bool {
        if coord.row >= self.max_rows || coord.col >= self.max_cols {
            return false;
        }
        let entry = self.cells.entry(coord).or_default();
        entry.raw = text.to_string();
        entry.hyperlink = None;
        self.recalculate();
        true
    }

    /// 仅供内部/测试构造潜在脏数据以验证序列化器的防卫性检查
    pub fn set_cell_unchecked(&mut self, coord: CellCoord, text: &str) {
        let entry = self.cells.entry(coord).or_default();
        entry.raw = text.to_string();
        entry.hyperlink = None;
        self.recalculate();
    }

    /// Validate the entire edit before applying it, then calculate just once.
    /// Importers retain styles and avoid one whole-sheet recalculation per cell.
    pub fn set_cells_bulk(&mut self, cells: &[(CellCoord, String)]) -> Result<(), &'static str> {
        if cells
            .iter()
            .any(|(c, _)| c.row >= self.max_rows || c.col >= self.max_cols)
        {
            return Err("单元格超出工作表范围");
        }
        for (coord, text) in cells {
            let entry = self.cells.entry(*coord).or_default();
            entry.raw.clone_from(text);
            entry.hyperlink = None;
        }
        self.recalculate();
        Ok(())
    }

    pub fn merged_ranges(&self) -> &[CellRange] {
        &self.merged_ranges
    }

    pub fn merged_range(&self, coord: CellCoord) -> Option<CellRange> {
        self.merged_ranges
            .iter()
            .copied()
            .find(|range| range.contains(coord))
    }

    pub fn cell_anchor(&self, coord: CellCoord) -> CellCoord {
        self.merged_range(coord)
            .map(|range| range.start)
            .unwrap_or(coord)
    }

    /// A selection always contains every merged cell it touches, including chains of merges.
    pub fn expand_range(&self, mut range: CellRange) -> CellRange {
        loop {
            let previous = range;
            for merged in &self.merged_ranges {
                if range.intersects(*merged) {
                    range = CellRange::new(
                        CellCoord::new(
                            range.start.col.min(merged.start.col),
                            range.start.row.min(merged.start.row),
                        ),
                        CellCoord::new(
                            range.end.col.max(merged.end.col),
                            range.end.row.max(merged.end.row),
                        ),
                    );
                }
            }
            if range == previous {
                return range;
            }
        }
    }

    /// Merge changes geometry only: covered values/formulas remain available after unmerge.
    pub fn merge_cells(
        &mut self,
        range: CellRange,
        center: bool,
    ) -> Result<CellRange, &'static str> {
        let range = self.expand_range(range);
        if range.end.col >= self.max_cols || range.end.row >= self.max_rows {
            return Err("选区超出工作表范围");
        }
        if range.start == range.end {
            return Err("请至少选择两个单元格");
        }
        self.merged_ranges
            .retain(|merged| !merged.intersects(range));
        self.merged_ranges.push(range);
        if center {
            self.set_alignment(range.start, HorizontalAlign::Center);
        }
        self.active_cell = range.start;
        Ok(range)
    }

    pub fn unmerge_cells(&mut self, range: CellRange) -> usize {
        let count = self.merged_ranges.len();
        self.merged_ranges
            .retain(|merged| !merged.intersects(range));
        count - self.merged_ranges.len()
    }

    pub fn has_vertical_merges(&self) -> bool {
        self.merged_ranges
            .iter()
            .any(|range| range.start.row != range.end.row)
    }

    /// Clear values from a group of cells in one recalculation pass while keeping formats.
    pub fn clear_contents(&mut self, coords: &[CellCoord]) {
        for coord in coords {
            if let Some(cell) = self.cells.get_mut(coord) {
                cell.raw.clear();
                cell.hyperlink = None;
            }
        }
        self.recalculate();
    }

    pub fn set_format(&mut self, coord: CellCoord, format: NumberFormat) {
        let entry = self.cells.entry(coord).or_default();
        entry.format = format;
    }

    pub fn format(&self, coord: CellCoord) -> NumberFormat {
        self.cells
            .get(&coord)
            .map(|c| c.format)
            .unwrap_or(NumberFormat::General)
    }

    pub fn style(&self, coord: CellCoord) -> CellStyle {
        self.cells
            .get(&coord)
            .map(|c| c.style.clone())
            .unwrap_or_default()
    }

    pub fn styles(&self) -> impl Iterator<Item = &CellStyle> {
        self.cells.values().map(|cell| &cell.style)
    }

    pub fn update_styles(&mut self, coords: &[CellCoord], mut update: impl FnMut(&mut CellStyle)) {
        for &coord in coords {
            if coord.col < self.max_cols && coord.row < self.max_rows {
                let style = &mut self.cells.entry(coord).or_default().style;
                update(style);
                style.font_size = style.font_size.clamp(8, 96);
                style.indent = style.indent.min(15);
            }
        }
    }

    pub fn format_snapshot(&self, coord: CellCoord) -> FormatSnapshot {
        FormatSnapshot {
            style: self.style(coord),
            format: self.format(coord),
        }
    }

    pub fn apply_format(&mut self, coords: &[CellCoord], snapshot: FormatSnapshot) {
        for &coord in coords {
            if coord.col < self.max_cols && coord.row < self.max_rows {
                let cell = self.cells.entry(coord).or_default();
                cell.style = snapshot.style.clone();
                cell.format = snapshot.format;
            }
        }
    }

    pub fn clear_all(&mut self, coords: &[CellCoord]) {
        for coord in coords {
            self.cells.remove(coord);
        }
        self.recalculate();
    }

    pub fn set_borders(&mut self, coords: &[CellCoord], mode: BorderMode) {
        let Some(min_col) = coords.iter().map(|c| c.col).min() else {
            return;
        };
        let max_col = coords.iter().map(|c| c.col).max().unwrap();
        let min_row = coords.iter().map(|c| c.row).min().unwrap();
        let max_row = coords.iter().map(|c| c.row).max().unwrap();
        for &coord in coords {
            let borders = match mode {
                BorderMode::None => 0,
                BorderMode::All => 15,
                BorderMode::Outline => {
                    u8::from(coord.col == min_col)
                        | (u8::from(coord.col == max_col) << 1)
                        | (u8::from(coord.row == min_row) << 2)
                        | (u8::from(coord.row == max_row) << 3)
                }
                BorderMode::Bottom => {
                    if coord.row == max_row {
                        8
                    } else {
                        0
                    }
                }
            };
            self.cells.entry(coord).or_default().style.borders = borders;
        }
    }

    pub fn toggle_bold(&mut self, coord: CellCoord) -> bool {
        let style = &mut self.cells.entry(coord).or_default().style;
        style.bold = !style.bold;
        style.bold
    }

    pub fn toggle_italic(&mut self, coord: CellCoord) -> bool {
        let style = &mut self.cells.entry(coord).or_default().style;
        style.italic = !style.italic;
        style.italic
    }

    pub fn toggle_underline(&mut self, coord: CellCoord) -> bool {
        let style = &mut self.cells.entry(coord).or_default().style;
        style.underline = !style.underline;
        style.underline
    }

    pub fn set_alignment(&mut self, coord: CellCoord, alignment: HorizontalAlign) {
        let style = &mut self.cells.entry(coord).or_default().style;
        style.horizontal = alignment;
        style.alignment_set = true;
    }

    pub fn adjust_decimals(&mut self, coord: CellCoord, increase: bool) {
        let cell = self.cells.entry(coord).or_default();
        cell.format = NumberFormat::Thousands;
        cell.style.decimals = if increase {
            cell.style.decimals.saturating_add(1).min(8)
        } else {
            cell.style.decimals.saturating_sub(1)
        };
    }

    pub fn adjust_font_size(&mut self, coord: CellCoord, increase: bool) -> u8 {
        let style = &mut self.cells.entry(coord).or_default().style;
        style.font_size = if increase {
            style.font_size.saturating_add(1).min(96)
        } else {
            style.font_size.saturating_sub(1).max(8)
        };
        style.font_size
    }

    pub fn clear_format(&mut self, coord: CellCoord) {
        if let Some(cell) = self.cells.get_mut(&coord) {
            cell.format = NumberFormat::General;
            cell.style = CellStyle::default();
        }
    }

    pub fn max_used_row(&self) -> usize {
        self.cells.keys().map(|c| c.row).max().unwrap_or(0)
    }

    pub fn sort_rows_by_column(&mut self, col: usize, start_row: usize, ascending: bool) {
        let last_row = self.max_used_row();
        if start_row >= last_row {
            return;
        }
        let mut rows: Vec<Vec<(usize, CellData)>> = (start_row..=last_row)
            .map(|row| {
                self.cells
                    .iter()
                    .filter_map(|(coord, cell)| {
                        (coord.row == row).then_some((coord.col, cell.clone()))
                    })
                    .collect()
            })
            .collect();
        rows.sort_by(|a, b| {
            let key = |row: &Vec<(usize, CellData)>| {
                row.iter()
                    .find(|(c, _)| *c == col)
                    .map(|(_, d)| d.evaluated.display_string())
                    .unwrap_or_default()
            };
            let ka = key(a);
            let kb = key(b);
            let order = match (ka.is_empty(), kb.is_empty()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => match (ka.parse::<f64>(), kb.parse::<f64>()) {
                    (Ok(a), Ok(b)) => a.total_cmp(&b),
                    _ => ka.to_lowercase().cmp(&kb.to_lowercase()),
                },
            };
            if ka.is_empty() || kb.is_empty() || ascending {
                order
            } else {
                order.reverse()
            }
        });
        for row in start_row..=last_row {
            self.cells.retain(|coord, _| coord.row != row);
        }
        for (offset, cells) in rows.into_iter().enumerate() {
            for (col, data) in cells {
                self.cells
                    .insert(CellCoord::new(col, start_row + offset), data);
            }
        }
        self.recalculate();
    }

    pub fn get_raw(&self, coord: CellCoord) -> &str {
        self.cells.get(&coord).map(|c| c.raw.as_str()).unwrap_or("")
    }

    pub fn get_value(&self, coord: CellCoord) -> CellValue {
        if let Some(cell) = self.cells.get(&coord) {
            cell.evaluated.clone()
        } else {
            CellValue::Empty
        }
    }

    pub fn get_formatted_display(&self, coord: CellCoord) -> String {
        if let Some(cell) = self.cells.get(&coord) {
            match &cell.evaluated {
                CellValue::Number(n) => match cell.format {
                    NumberFormat::Currency => format!("¥{:.2}", n),
                    NumberFormat::Percentage => format!("{:.1}%", n * 100.0),
                    NumberFormat::General => cell.evaluated.display_string(),
                    NumberFormat::Thousands => format_number(*n, cell.style.decimals, true),
                },
                _ => cell.evaluated.display_string(),
            }
        } else {
            String::new()
        }
    }

    /// Auto-Sum: Analyze the column above coord to automatically construct `=SUM(...)`
    pub fn auto_sum_above(&mut self, coord: CellCoord) {
        if coord.row == 0 {
            return;
        }

        let mut start_row = coord.row;
        let mut r = coord.row;
        while r > 0 {
            r -= 1;
            let check_coord = CellCoord::new(coord.col, r);
            let val = self.get_value(check_coord);
            if val.as_f64().is_some() {
                start_row = r;
            } else {
                break;
            }
        }

        if start_row < coord.row {
            let start_coord = CellCoord::new(coord.col, start_row);
            let end_coord = CellCoord::new(coord.col, coord.row - 1);
            let formula = format!("=SUM({}:{})", start_coord.to_a1(), end_coord.to_a1());
            self.set_cell(coord, &formula);
        }
    }

    /// Batch paste Tab-Separated Values (TSV) from clipboard starting at start coordinate
    pub fn paste_tsv(&mut self, start: CellCoord, tsv: &str) {
        for (r_offset, line) in tsv.lines().enumerate() {
            let r = start.row + r_offset;
            if r >= self.max_rows {
                break;
            }
            for (c_offset, field) in line.split('\t').enumerate() {
                let c = start.col + c_offset;
                if c >= self.max_cols {
                    break;
                }
                let coord = CellCoord::new(c, r);
                let entry = self.cells.entry(coord).or_default();
                entry.raw = field.trim().to_string();
                entry.hyperlink = None;
            }
        }
        self.recalculate();
    }

    /// Recalculate dependencies before dependents across one or multiple sheets.
    pub fn recalculate_sheets(sheets: &mut [Sheet]) {
        use std::cell::RefCell;
        use std::collections::HashSet;

        let name_to_idx: HashMap<String, usize> = sheets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.to_lowercase(), i))
            .collect();

        fn evaluate(
            sheet_idx: usize,
            coord: CellCoord,
            sheets: &[Sheet],
            name_to_idx: &HashMap<String, usize>,
            cache: &RefCell<HashMap<(usize, CellCoord), CellValue>>,
            visiting: &RefCell<HashSet<(usize, CellCoord)>>,
        ) -> CellValue {
            if let Some(value) = cache.borrow().get(&(sheet_idx, coord)).cloned() {
                return value;
            }
            let Some(cell) = sheets[sheet_idx].cells.get(&coord) else {
                return CellValue::Empty;
            };
            if visiting.borrow().len() >= 512 {
                return CellValue::Error("DEPTH".into());
            }
            if !visiting.borrow_mut().insert((sheet_idx, coord)) {
                return CellValue::Error("CYCLE".into());
            }
            let raw = cell.raw.trim();
            let value = if raw.starts_with('=') {
                let error = RefCell::new(None);
                let resolver = |target_sheet: Option<&str>, target_coord: CellCoord| -> CellValue {
                    let target_idx = match target_sheet {
                        None => sheet_idx,
                        Some(name) => match name_to_idx.get(&name.to_lowercase()) {
                            Some(&idx) => idx,
                            None => return CellValue::Error("REF!".into()),
                        },
                    };
                    let value = evaluate(target_idx, target_coord, sheets, name_to_idx, cache, visiting);
                    if matches!(&value, CellValue::Error(_)) && error.borrow().is_none() {
                        *error.borrow_mut() = Some(value.clone());
                    }
                    value
                };
                let value = FormulaEngine::with_resolver(&resolver).eval(raw);
                error.into_inner().unwrap_or(value)
            } else if cell.raw.is_empty() {
                CellValue::Empty
            } else if let Ok(value) = raw.parse::<f64>() {
                CellValue::Number(value)
            } else {
                CellValue::Text(cell.raw.clone())
            };
            visiting.borrow_mut().remove(&(sheet_idx, coord));
            cache.borrow_mut().insert((sheet_idx, coord), value.clone());
            value
        }

        let cache = RefCell::new(HashMap::new());
        let visiting = RefCell::new(HashSet::new());
        for sheet_idx in 0..sheets.len() {
            let coords: Vec<CellCoord> = sheets[sheet_idx].cells.keys().copied().collect();
            for coord in coords {
                evaluate(sheet_idx, coord, sheets, &name_to_idx, &cache, &visiting);
            }
        }
        for ((sheet_idx, coord), value) in cache.into_inner() {
            if let Some(cell) = sheets[sheet_idx].cells.get_mut(&coord) {
                cell.evaluated = value;
            }
        }
    }

    /// Recalculate dependencies before dependents, independently of HashMap order.
    pub fn recalculate(&mut self) {
        Self::recalculate_sheets(std::slice::from_mut(self));
    }

    // ---- Row / column manipulation ----------------------------------------

    /// Insert a blank row before `row`, shifting existing rows down.
    pub fn insert_row(&mut self, row: usize) {
        let updated: HashMap<CellCoord, CellData> = self
            .cells
            .drain()
            .map(|(coord, data)| {
                if coord.row >= row {
                    (CellCoord::new(coord.col, coord.row + 1), data)
                } else {
                    (coord, data)
                }
            })
            .collect();
        self.cells = updated;
        self.recalculate();
    }

    /// Delete `row`, shifting rows above it up.
    /// Returns the deleted cells as `(coord, raw_content)` for undo purposes.
    pub fn delete_row(&mut self, row: usize) -> Vec<(CellCoord, String)> {
        let deleted: Vec<(CellCoord, String)> = self
            .cells
            .iter()
            .filter(|(coord, _)| coord.row == row)
            .map(|(coord, data)| (*coord, data.raw.clone()))
            .collect();
        let updated: HashMap<CellCoord, CellData> = self
            .cells
            .drain()
            .filter_map(|(coord, data)| {
                if coord.row == row {
                    None
                } else if coord.row > row {
                    Some((CellCoord::new(coord.col, coord.row - 1), data))
                } else {
                    Some((coord, data))
                }
            })
            .collect();
        self.cells = updated;
        self.recalculate();
        deleted
    }

    /// Insert a blank column before `col`, shifting existing columns right.
    pub fn insert_col(&mut self, col: usize) {
        let updated: HashMap<CellCoord, CellData> = self
            .cells
            .drain()
            .map(|(coord, data)| {
                if coord.col >= col {
                    (CellCoord::new(coord.col + 1, coord.row), data)
                } else {
                    (coord, data)
                }
            })
            .collect();
        self.cells = updated;
        self.recalculate();
    }

    /// Delete `col`, shifting columns to the right leftward.
    /// Returns the deleted cells as `(coord, raw_content)` for undo purposes.
    pub fn delete_col(&mut self, col: usize) -> Vec<(CellCoord, String)> {
        let deleted: Vec<(CellCoord, String)> = self
            .cells
            .iter()
            .filter(|(coord, _)| coord.col == col)
            .map(|(coord, data)| (*coord, data.raw.clone()))
            .collect();
        let updated: HashMap<CellCoord, CellData> = self
            .cells
            .drain()
            .filter_map(|(coord, data)| {
                if coord.col == col {
                    None
                } else if coord.col > col {
                    Some((CellCoord::new(coord.col - 1, coord.row), data))
                } else {
                    Some((coord, data))
                }
            })
            .collect();
        self.cells = updated;
        self.recalculate();
        deleted
    }

    /// Sort rows within `range` by `sort_col`, returning the pre-sort snapshot for undo.
    pub fn sort_range(
        &mut self,
        range: CellRange,
        sort_col: usize,
        ascending: bool,
    ) -> Vec<(CellCoord, String)> {
        // Snapshot for undo
        let old_cells: Vec<(CellCoord, String)> = self
            .cells
            .iter()
            .filter(|(coord, _)| range.contains(**coord))
            .map(|(coord, data)| (*coord, data.raw.clone()))
            .collect();

        self.sort_rows_by_column(sort_col, range.min_row(), ascending);
        old_cells
    }

    // ---- Alignment aliases ------------------------------------------------

    /// Alias: set horizontal alignment (backwards compat with `set_align`).
    #[inline]
    pub fn set_align(&mut self, coord: CellCoord, align: HorizontalAlign) {
        self.set_alignment(coord, align);
    }

    /// Read horizontal alignment of a cell (alias for `.style(coord).horizontal`).
    #[inline]
    pub fn get_align(&self, coord: CellCoord) -> HorizontalAlign {
        self.style(coord).horizontal
    }

    /// Read cell style (alias for `.style(coord)` to match legacy API).
    #[inline]
    pub fn get_style(&self, coord: CellCoord) -> CellStyle {
        self.style(coord)
}


    /// Calculate summary statistics for cells within a range.
    pub fn calc_range_stats(&self, range: CellRange) -> Option<RangeStats> {
        let mut count = 0;
        let mut sum = 0.0;
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut has_num = false;

        for coord in range.coords() {
            if let Some(cell) = self.cells.get(&coord) {
                if let Some(n) = cell.evaluated.as_f64() {
                    count += 1;
                    sum += n;
                    if n < min { min = n; }
                    if n > max { max = n; }
                    has_num = true;
                }
            }
        }

        if !has_num {
            None
        } else {
            Some(RangeStats {
                count,
                sum,
                avg: sum / (count as f64),
                min,
                max,
            })
        }
    }
    }

fn format_number(value: f64, decimals: u8, grouping: bool) -> String {
    let raw = format!("{:.*}", decimals as usize, value);
    if !grouping {
        return raw;
    }
    let (sign, unsigned) = if let Some(rest) = raw.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", raw.as_str())
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let mut grouped = String::new();
    for (index, ch) in whole.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let whole: String = grouped.chars().rev().collect();
    if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

#[cfg(test)]
mod formatting_tests {
    use super::*;

    #[test]
    fn format_paint_preserves_target_formula_and_value() {
        let mut sheet = Sheet::new("Sheet1", 10, 10);
        let source = CellCoord::new(0, 0);
        let target = CellCoord::new(1, 0);
        sheet.set_cell(source, "3");
        sheet.set_cell(target, "=A1*2");
        sheet.set_format(source, NumberFormat::Currency);
        sheet.update_styles(&[source], |style| {
            style.font = CellFont::SimSun;
            style.bold = true;
            style.font_size = 24;
            style.text_color = [220, 55, 65];
            style.fill_color = Some([255, 230, 100]);
            style.borders = 15;
        });
        sheet.apply_format(&[target], sheet.format_snapshot(source));
        assert_eq!(sheet.get_raw(target), "=A1*2");
        assert_eq!(sheet.get_value(target).as_f64(), Some(6.0));
        assert_eq!(sheet.style(target), sheet.style(source));
        assert_eq!(sheet.get_formatted_display(target), "¥6.00");
    }

    #[test]
    fn clear_modes_keep_only_the_requested_cell_parts() {
        let mut sheet = Sheet::new("Sheet1", 10, 10);
        let a = CellCoord::new(0, 0);
        let b = CellCoord::new(1, 0);
        let c = CellCoord::new(2, 0);
        for coord in [a, b, c] {
            sheet.set_cell(coord, "42");
            sheet.set_format(coord, NumberFormat::Currency);
        }
        sheet.update_styles(&[a, b, c], |style| {
            style.bold = true;
            style.fill_color = Some([1, 2, 3]);
        });
        sheet.clear_contents(&[a]);
        sheet.clear_format(b);
        sheet.clear_all(&[c]);
        assert_eq!(sheet.get_raw(a), "");
        assert!(sheet.style(a).bold);
        assert_eq!(sheet.format(a), NumberFormat::Currency);
        assert_eq!(sheet.get_raw(b), "42");
        assert_eq!(sheet.style(b), CellStyle::default());
        assert_eq!(sheet.format(b), NumberFormat::General);
        assert_eq!(sheet.get_raw(c), "");
        assert_eq!(sheet.style(c), CellStyle::default());
        assert_eq!(sheet.format(c), NumberFormat::General);
        sheet.set_cell(a, "7");
        assert_eq!(sheet.get_formatted_display(a), "¥7.00");
    }

    #[test]
    fn outline_borders_leave_interior_cells_unbordered() {
        let mut sheet = Sheet::new("Sheet1", 10, 10);
        let coords: Vec<_> = (1..=3)
            .flat_map(|row| (1..=3).map(move |col| CellCoord::new(col, row)))
            .collect();
        sheet.set_borders(&coords, BorderMode::Outline);
        assert_eq!(sheet.style(CellCoord::new(1, 1)).borders, 1 | 4);
        assert_eq!(sheet.style(CellCoord::new(3, 3)).borders, 2 | 8);
        assert_eq!(sheet.style(CellCoord::new(2, 2)).borders, 0);
        sheet.set_borders(&coords, BorderMode::All);
        assert_eq!(sheet.style(CellCoord::new(2, 2)).borders, 15);
        sheet.set_borders(&coords, BorderMode::Bottom);
        assert_eq!(sheet.style(CellCoord::new(2, 3)).borders, 8);
        assert_eq!(sheet.style(CellCoord::new(2, 1)).borders, 0);
        sheet.set_borders(&coords, BorderMode::None);
        assert!(coords.iter().all(|&coord| sheet.style(coord).borders == 0));
    }

    #[test]
    fn test_cross_sheet_recalculate() {
        let mut s1 = Sheet::new("Balance_Sheet_Consolidation", 20, 10);
        s1.set_cell(CellCoord::new(1, 1), "50000000");

        let mut s2 = Sheet::new("Cash_Flow_Statement", 20, 10);
        s2.set_cell(CellCoord::new(1, 11), "50000000");
        s2.set_cell(CellCoord::new(1, 12), "=B12-Balance_Sheet_Consolidation!B2");

        let mut sheets = [s1, s2];
        Sheet::recalculate_sheets(&mut sheets);

        assert_eq!(sheets[1].get_value(CellCoord::new(1, 11)), CellValue::Number(50000000.0));
        assert_eq!(sheets[1].get_value(CellCoord::new(1, 12)), CellValue::Number(0.0));
    }
}
