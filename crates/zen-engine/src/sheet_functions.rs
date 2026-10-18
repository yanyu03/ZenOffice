//! Bounded input contract for the first function wizard, using the existing evaluator.
use super::{CellCoord, CellRange, CellValue, Sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicFunction {
    Sum,
    Average,
    Count,
    Max,
    Min,
}

impl BasicFunction {
    pub const ALL: [Self; 5] = [Self::Sum, Self::Average, Self::Count, Self::Max, Self::Min];
    pub fn name(self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Average => "AVERAGE",
            Self::Count => "COUNT",
            Self::Max => "MAX",
            Self::Min => "MIN",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Sum => "求和",
            Self::Average => "算术平均值",
            Self::Count => "统计数值个数",
            Self::Max => "最大值",
            Self::Min => "最小值",
        }
    }
    pub fn category(self) -> &'static str {
        if self == Self::Sum {
            "数学"
        } else {
            "统计"
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("AVG") {
            return Some(Self::Average);
        }
        Self::ALL
            .into_iter()
            .find(|f| name.eq_ignore_ascii_case(f.name()))
    }
}

fn reference(sheet: &Sheet, input: &str) -> Result<CellCoord, String> {
    let value = input.trim();
    let split = value
        .bytes()
        .position(|b| b.is_ascii_digit())
        .ok_or("请输入 A1 单元格地址")?;
    if !(1..=3).contains(&split)
        || value.len() - split > 7
        || !value[..split].bytes().all(|b| b.is_ascii_alphabetic())
        || !value[split..].bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!("无效单元格地址：{value}"));
    }
    let coord = CellCoord::from_a1(value).ok_or_else(|| format!("无效单元格地址：{value}"))?;
    if coord.col >= sheet.max_cols || coord.row >= sheet.max_rows {
        return Err(format!("{value} 超出当前工作表范围"));
    }
    Ok(coord)
}

impl Sheet {
    /// Accept finite numbers, A1 references and rectangular ranges only.
    /// Limit aggregate work before passing the formula to the existing evaluator.
    pub fn build_function_formula(
        &self,
        target: CellCoord,
        function: BasicFunction,
        arguments: &str,
    ) -> Result<String, String> {
        if target.col >= self.max_cols
            || target.row >= self.max_rows
            || self.cell_anchor(target) != target
        {
            return Err("目标单元格无效".into());
        }
        if arguments.len() > 4096 {
            return Err("参数内容过长".into());
        }
        let inputs: Vec<_> = arguments.split([',', ';', '，', '；']).collect();
        if inputs.len() > 30 {
            return Err("本批最多支持 30 个参数".into());
        }
        let mut normalized = Vec::new();
        let mut visited = 0usize;
        for input in inputs {
            let input = input.trim();
            if input.is_empty() {
                return Err("参数不能为空；可输入 B2:B4 或 B2:B4,10".into());
            }
            if let Some((left, right)) = input.split_once(':') {
                let range = CellRange::new(reference(self, left)?, reference(self, right)?);
                let area = (range.end.col - range.start.col + 1)
                    .checked_mul(range.end.row - range.start.row + 1)
                    .ok_or("区域过大")?;
                visited = visited.checked_add(area).ok_or("区域过大")?;
                if visited > 100_000 {
                    return Err("单次函数引用最多 100000 个单元格".into());
                }
                if range.contains(target) {
                    return Err("参数区域包含目标单元格，会产生循环引用".into());
                }
                normalized.push(format!("{}:{}", range.start.to_a1(), range.end.to_a1()));
            } else if let Ok(number) = input.parse::<f64>() {
                if !number.is_finite() {
                    return Err("数字必须是有限值".into());
                }
                normalized.push(number.to_string());
            } else {
                let coord = reference(self, input)?;
                if coord == target {
                    return Err("目标单元格不能引用自身".into());
                }
                visited += 1;
                if visited > 100_000 {
                    return Err("单次函数引用最多 100000 个单元格".into());
                }
                normalized.push(coord.to_a1());
            }
        }
        Ok(format!("={}({})", function.name(), normalized.join(",")))
    }

    /// Preview or commit the same candidate; never mutate the source sheet on failure.
    pub fn function_candidate(
        &self,
        target: CellCoord,
        function: BasicFunction,
        arguments: &str,
    ) -> Result<(Self, String, f64), String> {
        let formula = self.build_function_formula(target, function, arguments)?;
        let mut candidate = self.clone();
        candidate.set_cell(target, &formula);
        match candidate.get_value(target) {
            CellValue::Number(value) if value.is_finite() => Ok((candidate, formula, value)),
            CellValue::Error(error) => Err(format!("无法计算：{error}；请检查引用与源单元格")),
            _ => Err("函数未返回有效数值".into()),
        }
    }
}
