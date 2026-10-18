//! Read-only physical-cell statistics over the stored sparse cells.
use super::{CellRange, CellValue, Sheet};

#[derive(Debug, Clone)]
pub struct RangeStatistics {
    pub range: CellRange,
    pub total: u64,
    pub nonempty: u64,
    pub numbers: u64,
    pub texts: u64,
    pub formulas: u64,
    pub errors: u64,
    pub text_characters: u64,
    pub sum: Option<f64>,
    pub average: Option<f64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub aggregate_overflow: bool,
}

impl Sheet {
    pub fn range_statistics(&self, range: CellRange) -> Result<RangeStatistics, &'static str> {
        if range.start.row > range.end.row
            || range.start.col > range.end.col
            || range.end.row >= self.max_rows
            || range.end.col >= self.max_cols
        {
            return Err("统计区域超出工作表范围");
        }
        let height =
            u64::try_from(range.end.row - range.start.row + 1).map_err(|_| "统计区域过大")?;
        let width =
            u64::try_from(range.end.col - range.start.col + 1).map_err(|_| "统计区域过大")?;
        let mut out = RangeStatistics {
            range,
            total: height.checked_mul(width).ok_or("统计区域过大")?,
            nonempty: 0,
            numbers: 0,
            texts: 0,
            formulas: 0,
            errors: 0,
            text_characters: 0,
            sum: None,
            average: None,
            minimum: None,
            maximum: None,
            aggregate_overflow: false,
        };
        let mut sum = 0.0_f64;
        let mut correction = 0.0_f64;
        // Coordinate order makes floating point accumulation reproducible.
        let mut cells: Vec<_> = self
            .cells
            .iter()
            .filter(|(coord, data)| range.contains(**coord) && !data.raw.is_empty())
            .collect();
        cells.sort_unstable_by_key(|(coord, _)| (coord.row, coord.col));
        for (_, data) in cells {
            out.nonempty += 1;
            if data.raw.trim_start().starts_with('=') {
                out.formulas += 1;
            }
            match &data.evaluated {
                CellValue::Number(value) if value.is_finite() => {
                    out.numbers += 1;
                    out.minimum = Some(out.minimum.map_or(*value, |previous| previous.min(*value)));
                    out.maximum = Some(out.maximum.map_or(*value, |previous| previous.max(*value)));
                    let adjusted = value - correction;
                    let next = sum + adjusted;
                    correction = (next - sum) - adjusted;
                    sum = next;
                    if !sum.is_finite() || !correction.is_finite() {
                        out.aggregate_overflow = true;
                    }
                }
                CellValue::Number(_) | CellValue::Error(_) => out.errors += 1,
                CellValue::Text(text) => {
                    out.texts += 1;
                    out.text_characters = out
                        .text_characters
                        .checked_add(text.chars().count() as u64)
                        .ok_or("文本字符数量超出统计范围")?;
                }
                CellValue::Empty => {}
            }
        }
        if out.numbers > 0 && !out.aggregate_overflow {
            out.sum = Some(sum);
            out.average = Some(sum / out.numbers as f64);
        }
        Ok(out)
    }
}
