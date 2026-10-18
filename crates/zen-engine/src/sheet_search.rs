//! Literal search and planned replacement for the existing Sheet editor.
//! No native WPS code; Unicode lowercase matching is an explicit Zen policy.
use super::{CellCoord, CellRange, Sheet};

const MAX_STORED_CELLS: usize = 100_000;
const MAX_SCAN_BYTES: usize = 16 * 1024 * 1024;
const MAX_CELL_BYTES: usize = 256 * 1024;
const MAX_QUERY_BYTES: usize = 4096;
const MAX_REPLACEMENT_BYTES: usize = 128 * 1024;
const MAX_CELL_CHARACTERS: usize = 32_767;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOrder {
    #[default]
    Rows,
    Columns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchLookIn {
    #[default]
    Contents,
    DisplayedValues,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchRequest {
    pub query: String,
    pub scope: Option<CellRange>,
    pub match_case: bool,
    pub whole_cell: bool,
    pub order: SearchOrder,
    pub look_in: SearchLookIn,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub coord: CellCoord,
    pub occurrences: usize,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct ReplaceRequest {
    pub search: SearchRequest,
    pub replacement: String,
    pub allow_formulas: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ReplaceTarget {
    All,
    Cell(CellCoord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementEdit {
    pub coord: CellCoord,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone)]
pub struct ReplacePlan {
    request: ReplaceRequest,
    target: ReplaceTarget,
    edits: Vec<ReplacementEdit>,
    matched_cells: usize,
    occurrences: usize,
    skipped_formulas: usize,
}

impl ReplacePlan {
    pub fn edits(&self) -> &[ReplacementEdit] {
        &self.edits
    }
    pub fn matched_cells(&self) -> usize {
        self.matched_cells
    }
    pub fn occurrences(&self) -> usize {
        self.occurrences
    }
    pub fn skipped_formulas(&self) -> usize {
        self.skipped_formulas
    }
}

struct Pattern<'a> {
    request: &'a SearchRequest,
    folded: String,
}

impl<'a> Pattern<'a> {
    fn new(request: &'a SearchRequest) -> Result<Self, String> {
        if request.query.is_empty() {
            return Err("请输入查找内容".into());
        }
        if request.query.len() > MAX_QUERY_BYTES {
            return Err("查找内容超过 4096 字节".into());
        }
        Ok(Self {
            request,
            folded: request.query.to_lowercase(),
        })
    }

    fn spans(&self, value: &str) -> Vec<(usize, usize)> {
        if self.request.whole_cell {
            let matches = if self.request.match_case {
                value == self.request.query
            } else {
                value.to_lowercase() == self.folded
            };
            return if matches {
                vec![(0, value.len())]
            } else {
                vec![]
            };
        }
        if self.request.match_case {
            return value
                .match_indices(&self.request.query)
                .map(|(start, found)| (start, start + found.len()))
                .collect();
        }
        // Lowercase may expand one source character (e.g. U+0130). Only accept
        // matches at original scalar boundaries, preserving valid UTF-8 offsets.
        let mut folded = String::with_capacity(value.len());
        let mut boundaries = vec![(0_usize, 0_usize)];
        for (start, character) in value.char_indices() {
            folded.extend(character.to_lowercase());
            boundaries.push((folded.len(), start + character.len_utf8()));
        }
        folded
            .match_indices(&self.folded)
            .filter_map(|(start, found)| {
                let left = boundaries
                    .binary_search_by_key(&start, |pair| pair.0)
                    .ok()?;
                let right = boundaries
                    .binary_search_by_key(&(start + found.len()), |pair| pair.0)
                    .ok()?;
                Some((boundaries[left].1, boundaries[right].1))
            })
            .collect()
    }
}

impl Sheet {
    fn validate_search_scope(&self, scope: Option<CellRange>) -> Result<(), String> {
        if scope.is_some_and(|r| {
            r.start.row > r.end.row
                || r.start.col > r.end.col
                || r.end.row >= self.max_rows
                || r.end.col >= self.max_cols
        }) {
            return Err("查找区域超出当前工作表范围".into());
        }
        if self.cells.len() > MAX_STORED_CELLS {
            return Err("当前工作表超过本批 100000 个存储单元格的搜索预算".into());
        }
        Ok(())
    }

    pub fn search_cells(&self, request: &SearchRequest) -> Result<Vec<SearchHit>, String> {
        self.validate_search_scope(request.scope)?;
        let pattern = Pattern::new(request)?;
        let mut bytes = 0_usize;
        let mut hits = vec![];
        for (coord, data) in &self.cells {
            if request.scope.is_some_and(|r| !r.contains(*coord))
                || self.cell_anchor(*coord) != *coord
                || data.raw.is_empty()
            {
                continue;
            }
            if data.raw.len() > MAX_CELL_BYTES {
                return Err("单个单元格超出 256 KiB 搜索预算".into());
            }
            let value = match request.look_in {
                SearchLookIn::Contents => data.raw.clone(),
                SearchLookIn::DisplayedValues => self.get_formatted_display(*coord),
            };
            if value.len() > MAX_CELL_BYTES {
                return Err("单个单元格超出 256 KiB 搜索预算".into());
            }
            bytes = bytes.checked_add(value.len()).ok_or("搜索内容过大")?;
            if bytes > MAX_SCAN_BYTES {
                return Err("搜索内容超过 16 MiB 预算，请缩小选区".into());
            }
            let count = pattern.spans(&value).len();
            if count > 0 {
                hits.push(SearchHit {
                    coord: *coord,
                    occurrences: count,
                    preview: value.chars().take(120).collect(),
                });
            }
        }
        hits.sort_unstable_by_key(|hit| match request.order {
            SearchOrder::Rows => (hit.coord.row, hit.coord.col),
            SearchOrder::Columns => (hit.coord.col, hit.coord.row),
        });
        Ok(hits)
    }

    pub fn plan_replace(
        &self,
        request: &ReplaceRequest,
        target: ReplaceTarget,
    ) -> Result<ReplacePlan, String> {
        if request.search.look_in != SearchLookIn::Contents {
            return Err("显示值仅用于查找；替换请使用原始内容".into());
        }
        if request.replacement.len() > MAX_REPLACEMENT_BYTES {
            return Err("替换内容超过 128 KiB".into());
        }
        self.validate_search_scope(request.search.scope)?;
        let mut scan = request.search.clone();
        if let ReplaceTarget::Cell(coord) = target {
            if coord.row >= self.max_rows
                || coord.col >= self.max_cols
                || scan.scope.is_some_and(|range| !range.contains(coord))
            {
                return Err("当前单元格不在替换范围内".into());
            }
            scan.scope = Some(CellRange::new(coord, coord));
        }
        let hits = self.search_cells(&scan)?;
        let pattern = Pattern::new(&scan)?;
        let mut plan = ReplacePlan {
            request: request.clone(),
            target,
            edits: vec![],
            matched_cells: hits.len(),
            occurrences: 0,
            skipped_formulas: 0,
        };
        let mut total_bytes = 0_usize;
        for hit in hits {
            let before = self.get_raw(hit.coord);
            if !request.allow_formulas && before.trim_start().starts_with('=') {
                plan.skipped_formulas += 1;
                continue;
            }
            let mut spans = pattern.spans(before);
            if matches!(target, ReplaceTarget::Cell(_)) {
                spans.truncate(1);
            }
            let removed: usize = spans.iter().map(|(a, b)| b - a).sum();
            let size = request
                .replacement
                .len()
                .checked_mul(spans.len())
                .and_then(|n| (before.len() - removed).checked_add(n))
                .ok_or("替换结果过大")?;
            if size > MAX_CELL_BYTES {
                return Err("替换结果超过单元格 256 KiB 预算".into());
            }
            let mut after = String::with_capacity(size);
            let mut cursor = 0;
            for (start, end) in &spans {
                after.push_str(&before[cursor..*start]);
                after.push_str(&request.replacement);
                cursor = *end;
            }
            after.push_str(&before[cursor..]);
            if after == before {
                continue;
            }
            if !request.allow_formulas && after.trim_start().starts_with('=') {
                return Err("替换结果会生成公式；请先允许修改或生成公式".into());
            }
            if after.trim_start().starts_with('=') && self.hyperlink(hit.coord).is_some() {
                return Err(
                    "超链接单元格不能通过替换生成公式；请先移除超链接，避免导出时丢失链接".into(),
                );
            }
            if after.chars().count() > MAX_CELL_CHARACTERS {
                return Err("替换结果超过本批单元格 32767 字符限制".into());
            }
            total_bytes = total_bytes
                .checked_add(before.len())
                .and_then(|n| n.checked_add(after.len()))
                .ok_or("替换计划过大")?;
            if total_bytes > MAX_SCAN_BYTES {
                return Err("替换计划超过 16 MiB 预算，请缩小选区".into());
            }
            plan.occurrences += spans.len();
            plan.edits.push(ReplacementEdit {
                coord: hit.coord,
                before: before.into(),
                after,
            });
        }
        Ok(plan)
    }

    pub fn apply_replace(&mut self, plan: &ReplacePlan) -> Result<usize, String> {
        // Re-plan the full read set, catching new/lost matches and changed merges.
        // Formatting or hyperlink changes survive because only raw text is written.
        let fresh = self.plan_replace(&plan.request, plan.target)?;
        if fresh.edits != plan.edits
            || fresh.matched_cells != plan.matched_cells
            || fresh.skipped_formulas != plan.skipped_formulas
        {
            return Err("替换范围内容已变化，请重新预览".into());
        }
        if plan.edits.is_empty() {
            return Ok(0);
        }
        // All fallible validation is above; no per-cell partial failure path.
        for edit in &plan.edits {
            self.cells
                .entry(edit.coord)
                .or_default()
                .raw
                .clone_from(&edit.after);
        }
        self.recalculate();
        Ok(plan.edits.len())
    }
}
