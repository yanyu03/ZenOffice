//! Bounded workbook calculation. Formula caches from the source file are never inputs.
//! One parsed expression supplies both dependency discovery and evaluation.
//! Unsupported syntax and formula kinds are rejected before the graph is modified.
//! Supports arithmetic, comparisons, nested SUM/AVERAGE/MIN/MAX/COUNT/ABS/IF/
//! IFERROR/PRODUCT/SUMIF and the internal AVG alias. SUMIF requires equal-shaped
//! ranges. Shared/array formulas, dynamic references and iterative cycles are
//! rejected. Limits include 64 expression levels, 8192 resolved nodes per formula,
//! 100,000 reference visits per formula and 1,000,000 workbook dependency edges.
use crate::{
    calc::{CellCoord, CellReference, CircularDependencyError, DependencyGraph},
    workbook::{CellAddress, FormulaKind, SheetKind, TypedValue, Workbook},
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RANGE_CELLS: usize = 100_000;
const MAX_DEPTH: usize = 64;
const MAX_RESOLVED_NODES: usize = 8192;
const MAX_TOTAL_DEPENDENCIES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum CalculationValue {
    Empty,
    Number(f64),
    Text(String),
    Boolean(bool),
    Error(String),
}
impl CalculationValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }
    fn from_source(value: Option<&TypedValue>) -> Self {
        match value {
            None => Self::Empty,
            Some(TypedValue::Number { value, .. }) => Self::Number(*value),
            Some(TypedValue::Text(s)) => Self::Text(s.clone()),
            Some(TypedValue::Boolean(b)) => Self::Boolean(*b),
            Some(TypedValue::Error(e)) => Self::Error(e.clone()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecalcError {
    #[error("Cannot calculate {cell}: {reason}")]
    Unsupported { cell: String, reason: String },
    #[error("{0}")]
    Circular(#[from] CircularDependencyError),
}

#[derive(Debug, Clone)]
enum Expr {
    Literal(CalculationValue),
    Reference(CellReference),
    Range(CellReference, CellReference),
    Name(Option<String>, String),
    Unary(char, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Word(String),
    QuotedSheet(String),
    Symbol(char),
    Compare(String),
}

fn lex(formula: &str) -> Result<Vec<Token>, String> {
    if formula.len() > 8192 {
        return Err("formula exceeds 8192 bytes".into());
    }
    let formula = formula.trim().strip_prefix('=').unwrap_or(formula.trim());
    let chars: Vec<char> = formula.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let mut s = String::new();
            i += 1;
            loop {
                let Some(&ch) = chars.get(i) else {
                    return Err("unterminated quoted text".into());
                };
                i += 1;
                if ch == quote {
                    if chars.get(i) == Some(&quote) {
                        s.push(quote);
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    s.push(ch);
                }
            }
            tokens.push(if quote == '"' {
                Token::Text(s)
            } else {
                Token::QuotedSheet(s)
            });
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            while chars.get(i).is_some_and(char::is_ascii_digit) {
                i += 1;
            }
            if chars.get(i) == Some(&'.') {
                i += 1;
                while chars.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
            }
            if matches!(chars.get(i), Some('e' | 'E')) {
                i += 1;
                if matches!(chars.get(i), Some('+' | '-')) {
                    i += 1;
                }
                let digits = i;
                while chars.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
                if digits == i {
                    return Err("invalid numeric exponent".into());
                }
            }
            let n: f64 = chars[start..i]
                .iter()
                .collect::<String>()
                .parse()
                .map_err(|_| "invalid number")?;
            if !n.is_finite() {
                return Err("non-finite numeric literal".into());
            }
            tokens.push(Token::Number(n));
        } else if c.is_alphabetic() || matches!(c, '_' | '$' | '\\') {
            let start = i;
            i += 1;
            while chars
                .get(i)
                .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '\\'))
            {
                i += 1;
            }
            tokens.push(Token::Word(chars[start..i].iter().collect()));
        } else if matches!(c, '=' | '<' | '>') {
            i += 1;
            let mut op = c.to_string();
            if matches!((c, chars.get(i)), ('<', Some('=' | '>')) | ('>', Some('='))) {
                op.push(chars[i]);
                i += 1;
            }
            tokens.push(Token::Compare(op));
        } else if "+-*/(),:!".contains(c) {
            tokens.push(Token::Symbol(c));
            i += 1;
        } else {
            return Err(format!("unsupported formula character '{c}'"));
        }
        if tokens.len() > 4096 {
            return Err("formula token quota exceeded".into());
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}
impl Parser {
    fn parse(formula: &str) -> Result<Expr, String> {
        let mut parser = Self {
            tokens: lex(formula)?,
            pos: 0,
        };
        let expr = parser.expression(0, 0)?;
        if parser.pos != parser.tokens.len() {
            return Err("unsupported or trailing formula syntax".into());
        }
        // Left-associative chains do not increase parser recursion, but still
        // create deep expression trees. Bound them before recursive resolution.
        let mut pending = vec![(&expr, 0usize)];
        while let Some((node, depth)) = pending.pop() {
            if depth > MAX_DEPTH {
                return Err("formula tree depth quota exceeded".into());
            }
            match node {
                Expr::Unary(_, x) => pending.push((x, depth + 1)),
                Expr::Binary(_, a, b) => {
                    pending.push((a, depth + 1));
                    pending.push((b, depth + 1));
                }
                Expr::Call(_, args) => pending.extend(args.iter().map(|arg| (arg, depth + 1))),
                _ => {}
            }
        }
        Ok(expr)
    }
    fn take_symbol(&mut self, symbol: char) -> bool {
        if self.tokens.get(self.pos) == Some(&Token::Symbol(symbol)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expression(&mut self, min_precedence: u8, depth: usize) -> Result<Expr, String> {
        if depth > MAX_DEPTH {
            return Err("formula nesting quota exceeded".into());
        }
        let mut left = self.primary(depth + 1)?;
        loop {
            let (op, precedence) = match self.tokens.get(self.pos) {
                Some(Token::Compare(s)) => (s.clone(), 1),
                Some(Token::Symbol(c @ ('+' | '-'))) => (c.to_string(), 2),
                Some(Token::Symbol(c @ ('*' | '/'))) => (c.to_string(), 3),
                _ => break,
            };
            if precedence < min_precedence {
                break;
            }
            self.pos += 1;
            let right = self.expression(precedence + 1, depth + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn primary(&mut self, depth: usize) -> Result<Expr, String> {
        if depth > MAX_DEPTH {
            return Err("formula nesting quota exceeded".into());
        }
        let token = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or("missing expression")?;
        self.pos += 1;
        let mut expr = match token {
            Token::Number(n) => Expr::Literal(CalculationValue::Number(n)),
            Token::Text(s) => Expr::Literal(CalculationValue::Text(s)),
            Token::Symbol(c @ ('+' | '-')) => Expr::Unary(c, Box::new(self.primary(depth + 1)?)),
            Token::Symbol('(') => {
                let expr = self.expression(0, depth + 1)?;
                if !self.take_symbol(')') {
                    return Err("unbalanced parentheses or unsupported union".into());
                }
                expr
            }
            Token::Word(word) if self.take_symbol('(') => {
                let name = word.to_ascii_uppercase();
                if ![
                    "SUM", "AVERAGE", "AVG", "MAX", "MIN", "COUNT", "ABS", "IF", "IFERROR",
                    "PRODUCT", "SUMIF",
                ]
                .contains(&name.as_str())
                {
                    return Err(format!("unsupported function {name}"));
                }
                let mut args = Vec::new();
                if !self.take_symbol(')') {
                    loop {
                        args.push(self.expression(0, depth + 1)?);
                        if self.take_symbol(')') {
                            break;
                        }
                        if !self.take_symbol(',') {
                            return Err("invalid function argument separator".into());
                        }
                    }
                }
                let valid = match name.as_str() {
                    "ABS" => args.len() == 1,
                    "IF" => (2..=3).contains(&args.len()),
                    "IFERROR" => args.len() == 2,
                    "SUMIF" => (2..=3).contains(&args.len()),
                    _ => !args.is_empty() && args.len() <= 255,
                };
                if !valid {
                    return Err(format!("invalid argument count for {name}"));
                }
                Expr::Call(name, args)
            }
            Token::Word(word) => self.reference_or_name(None, word)?,
            Token::QuotedSheet(sheet) => {
                if !self.take_symbol('!') {
                    return Err("quoted sheet must precede !".into());
                }
                let Some(Token::Word(word)) = self.tokens.get(self.pos).cloned() else {
                    return Err("missing sheet reference".into());
                };
                self.pos += 1;
                self.reference_or_name(Some(sheet), word)?
            }
            _ => return Err("unsupported expression".into()),
        };
        if self.take_symbol(':') {
            let Expr::Reference(first) = expr else {
                return Err("range must start with a cell".into());
            };
            let Expr::Reference(last) = self.primary(depth + 1)? else {
                return Err("range must end with a cell".into());
            };
            expr = Expr::Range(first, last);
        }
        Ok(expr)
    }
    fn reference_or_name(
        &mut self,
        mut sheet: Option<String>,
        mut word: String,
    ) -> Result<Expr, String> {
        if sheet.is_none() && self.take_symbol('!') {
            sheet = Some(word);
            let Some(Token::Word(next)) = self.tokens.get(self.pos).cloned() else {
                return Err("missing qualified reference".into());
            };
            self.pos += 1;
            word = next;
        }
        if let Some(coord) = CellCoord::from_a1(&word) {
            let row = u32::try_from(coord.row).map_err(|_| "row exceeds XLSX bounds")?;
            let col = u16::try_from(coord.col).map_err(|_| "column exceeds XLSX bounds")?;
            CellAddress::new(row, col).map_err(|_| "cell exceeds XLSX bounds")?;
            Ok(Expr::Reference(CellReference::from_coord(sheet, coord)))
        } else if word.contains('$') {
            Err("invalid absolute reference".into())
        } else if sheet.is_none() && word.eq_ignore_ascii_case("TRUE") {
            Ok(Expr::Literal(CalculationValue::Boolean(true)))
        } else if sheet.is_none() && word.eq_ignore_ascii_case("FALSE") {
            Ok(Expr::Literal(CalculationValue::Boolean(false)))
        } else {
            Ok(Expr::Name(sheet, word))
        }
    }
}

fn canonical_sheet(workbook: &Workbook, name: &str) -> Result<String, String> {
    workbook
        .sheets()
        .iter()
        .find(|s| s.name().to_lowercase() == name.to_lowercase())
        .filter(|s| matches!(s.kind(), SheetKind::Worksheet))
        .map(|s| s.name().to_owned())
        .ok_or_else(|| format!("unknown or non-worksheet sheet '{name}'"))
}

fn resolve(
    expr: Expr,
    workbook: &Workbook,
    sheet: &str,
    names: &mut Vec<String>,
    explicit_sheet: bool,
    depth: usize,
    budget: &mut usize,
) -> Result<Expr, String> {
    *budget = budget
        .checked_sub(1)
        .ok_or("defined-name expansion quota exceeded")?;
    if depth > MAX_DEPTH {
        return Err("resolved formula depth quota exceeded".into());
    }
    let resolve_ref = |mut r: CellReference, default: &str| -> Result<CellReference, String> {
        if explicit_sheet && r.sheet.is_none() {
            return Err("relative global defined-name references are unsupported".into());
        }
        r.sheet = Some(canonical_sheet(
            workbook,
            r.sheet.as_deref().unwrap_or(default),
        )?);
        Ok(r)
    };
    Ok(match expr {
        Expr::Reference(r) => Expr::Reference(resolve_ref(r, sheet)?),
        Expr::Range(a, mut b) => {
            let a = resolve_ref(a, sheet)?;
            // A qualified range start supplies the default sheet for its endpoint.
            if b.sheet.is_none() {
                b.sheet = a.sheet.clone();
            }
            let b = resolve_ref(b, a.sheet.as_deref().unwrap())?;
            if a.sheet != b.sheet {
                return Err("3D or cross-sheet range endpoints are unsupported".into());
            }
            let count = (a.col.abs_diff(b.col) + 1) as u64 * (a.row.abs_diff(b.row) + 1) as u64;
            if count > MAX_RANGE_CELLS as u64 {
                return Err(format!("range quota exceeded: {count} > {MAX_RANGE_CELLS}"));
            }
            Expr::Range(a, b)
        }
        Expr::Name(scope, name) => {
            let scope = canonical_sheet(workbook, scope.as_deref().unwrap_or(sheet))?;
            let id = workbook
                .sheets()
                .iter()
                .find(|s| s.name() == scope)
                .unwrap()
                .id();
            let key = format!("{}!{}", scope.to_lowercase(), name.to_lowercase());
            if names.len() >= MAX_DEPTH || names.contains(&key) {
                return Err(format!("recursive defined name {name}"));
            }
            let definition = workbook
                .defined_names()
                .iter()
                .find(|n| n.local_sheet == Some(id) && n.name.eq_ignore_ascii_case(&name))
                .or_else(|| {
                    workbook
                        .defined_names()
                        .iter()
                        .find(|n| n.local_sheet.is_none() && n.name.eq_ignore_ascii_case(&name))
                })
                .ok_or_else(|| format!("unresolved defined name {name}"))?;
            names.push(key);
            let parsed = Parser::parse(&definition.formula)?;
            let result = resolve(
                parsed,
                workbook,
                &scope,
                names,
                definition.local_sheet.is_none(),
                depth + 1,
                budget,
            )?;
            names.pop();
            result
        }
        Expr::Unary(op, x) => Expr::Unary(
            op,
            Box::new(resolve(
                *x,
                workbook,
                sheet,
                names,
                explicit_sheet,
                depth + 1,
                budget,
            )?),
        ),
        Expr::Binary(op, a, b) => Expr::Binary(
            op,
            Box::new(resolve(
                *a,
                workbook,
                sheet,
                names,
                explicit_sheet,
                depth + 1,
                budget,
            )?),
            Box::new(resolve(
                *b,
                workbook,
                sheet,
                names,
                explicit_sheet,
                depth + 1,
                budget,
            )?),
        ),
        Expr::Call(f, args) => Expr::Call(
            f,
            args.into_iter()
                .map(|x| resolve(x, workbook, sheet, names, explicit_sheet, depth + 1, budget))
                .collect::<Result<_, _>>()?,
        ),
        literal => literal,
    })
}

fn dependencies(
    expr: &Expr,
    deps: &mut BTreeSet<CellReference>,
    budget: &mut usize,
    range_allowed: bool,
) -> Result<(), String> {
    match expr {
        Expr::Reference(r) => {
            *budget += 1;
            deps.insert(r.clone());
        }
        Expr::Range(a, b) => {
            if !range_allowed {
                return Err(
                    "range in scalar expression requires unsupported array/intersection semantics"
                        .into(),
                );
            }
            for row in a.row.min(b.row)..=a.row.max(b.row) {
                for col in a.col.min(b.col)..=a.col.max(b.col) {
                    *budget += 1;
                    if *budget > MAX_RANGE_CELLS {
                        return Err("formula dependency quota exceeded".into());
                    }
                    deps.insert(CellReference::new(a.sheet.clone(), col, row));
                }
            }
        }
        Expr::Unary(_, x) => dependencies(x, deps, budget, false)?,
        Expr::Binary(_, a, b) => {
            dependencies(a, deps, budget, false)?;
            dependencies(b, deps, budget, false)?;
        }
        Expr::Call(f, args) => {
            if f == "SUMIF" {
                let first = range_cells(&args[0])
                    .ok_or("SUMIF requires a cell or rectangular criteria range")?;
                if let Some(sum) = args.get(2) {
                    let sum =
                        range_cells(sum).ok_or("SUMIF requires a cell or rectangular sum range")?;
                    let shape = |cells: &[CellReference]| {
                        let rows = cells.iter().map(|r| r.row).collect::<BTreeSet<_>>().len();
                        (rows, cells.len() / rows)
                    };
                    if shape(&first) != shape(&sum) {
                        return Err(
                            "SUMIF implicit range resizing is unsupported; use equal-shaped ranges"
                                .into(),
                        );
                    }
                }
                for (i, arg) in args.iter().enumerate() {
                    dependencies(arg, deps, budget, i != 1)?;
                }
                return Ok(());
            }
            let aggregate = !matches!(f.as_str(), "ABS" | "IF" | "IFERROR");
            for arg in args {
                dependencies(arg, deps, budget, aggregate)?;
            }
        }
        _ => {}
    }
    if *budget > MAX_RANGE_CELLS {
        return Err("formula dependency quota exceeded".into());
    }
    Ok(())
}

fn number(value: CalculationValue) -> Result<f64, String> {
    match value {
        CalculationValue::Number(n) => Ok(n),
        CalculationValue::Empty => Ok(0.0),
        CalculationValue::Boolean(b) => Ok(if b { 1.0 } else { 0.0 }),
        CalculationValue::Text(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| "#VALUE!".into()),
        CalculationValue::Error(e) => Err(e),
    }
}
fn numeric_result(value: Result<f64, String>) -> CalculationValue {
    match value {
        Ok(n) if n.is_finite() => CalculationValue::Number(n),
        Ok(_) => CalculationValue::Error("#NUM!".into()),
        Err(e) => CalculationValue::Error(e),
    }
}
fn raw_value(
    values: &BTreeMap<CellReference, CalculationValue>,
    cell: &CellReference,
) -> CalculationValue {
    values.get(cell).cloned().unwrap_or(CalculationValue::Empty)
}

fn range_cells(expr: &Expr) -> Option<Vec<CellReference>> {
    match expr {
        Expr::Reference(r) => Some(vec![r.clone()]),
        Expr::Range(a, b) => Some(
            (a.row.min(b.row)..=a.row.max(b.row))
                .flat_map(|row| {
                    (a.col.min(b.col)..=a.col.max(b.col))
                        .map(move |col| CellReference::new(a.sheet.clone(), col, row))
                })
                .collect(),
        ),
        _ => None,
    }
}

fn criterion_matches(
    value: &CalculationValue,
    criterion: &CalculationValue,
    budget: &mut usize,
) -> Result<bool, String> {
    use CalculationValue as V;
    let text = match criterion {
        V::Error(e) => return Err(e.clone()),
        V::Number(n) => {
            return Ok(
                matches!(value,V::Number(v) if v == n) || matches!(value, V::Empty) && *n == 0.0
            );
        }
        V::Boolean(b) => return Ok(matches!(value,V::Boolean(v) if v == b)),
        V::Empty => return Ok(matches!(value, V::Empty | V::Number(0.0))),
        V::Text(s) => s,
    };
    if text.chars().count() > 255 {
        return Err("#ZEN_LIMIT!".into());
    }
    let (op, operand) = ["<=", ">=", "<>", "<", ">", "="]
        .into_iter()
        .find_map(|op| text.strip_prefix(op).map(|s| (op, s)))
        .unwrap_or(("=", text.as_str()));
    let compare = if let Ok(n) = operand.parse::<f64>() {
        if !n.is_finite() {
            return Err("#NUM!".into());
        }
        let v = match value {
            V::Number(n) => *n,
            V::Empty => 0.0,
            _ => return Ok(op == "<>"),
        };
        v.partial_cmp(&n).unwrap()
    } else {
        let target = match value {
            V::Text(s) => s.as_str(),
            V::Empty => "",
            _ => return Ok(op == "<>"),
        };
        if matches!(op, "=" | "<>") && operand.contains(['*', '?', '~']) {
            let pattern: Vec<char> = operand.to_lowercase().chars().collect();
            let target: Vec<char> = target.to_lowercase().chars().collect();
            let cost = (pattern.len() + 1).saturating_mul(target.len() + 1);
            *budget = budget.checked_sub(cost).ok_or("#ZEN_LIMIT!")?;
            let mut matched = vec![false; target.len() + 1];
            matched[0] = true;
            let mut i = 0;
            while i < pattern.len() {
                let mut c = pattern[i];
                let mut literal = false;
                if c == '~' && i + 1 < pattern.len() {
                    i += 1;
                    c = pattern[i];
                    literal = true;
                }
                let mut next = vec![false; target.len() + 1];
                if c == '*' && !literal {
                    next[0] = matched[0];
                }
                for j in 1..=target.len() {
                    next[j] = if c == '*' && !literal {
                        matched[j] || next[j - 1]
                    } else {
                        matched[j - 1] && (c == target[j - 1] || c == '?' && !literal)
                    };
                }
                matched = next;
                i += 1;
            }
            return Ok(if op == "=" {
                matched[target.len()]
            } else {
                !matched[target.len()]
            });
        }
        target.to_lowercase().cmp(&operand.to_lowercase())
    };
    Ok(match op {
        "=" => compare.is_eq(),
        "<>" => !compare.is_eq(),
        "<" => compare.is_lt(),
        ">" => compare.is_gt(),
        "<=" => !compare.is_gt(),
        _ => !compare.is_lt(),
    })
}

fn eval(expr: &Expr, values: &BTreeMap<CellReference, CalculationValue>) -> CalculationValue {
    use CalculationValue as V;
    match expr {
        Expr::Literal(v) => v.clone(),
        Expr::Reference(r) => match raw_value(values, r) {
            V::Empty => V::Number(0.0),
            v => v,
        },
        Expr::Unary(op, x) => {
            numeric_result(number(eval(x, values)).map(|n| if *op == '-' { -n } else { n }))
        }
        Expr::Binary(op, a, b) => {
            let comparison = matches!(op.as_str(), "=" | "<>" | "<" | ">" | "<=" | ">=");
            // A blank reference compares as 0, empty text, or FALSE depending on
            // the other operand; ordinary scalar evaluation would erase this.
            let operand = |expr: &Expr| match expr {
                Expr::Reference(r) if comparison => raw_value(values, r),
                _ => eval(expr, values),
            };
            let mut av = operand(a);
            let mut bv = operand(b);
            if let V::Error(_) = av {
                return av;
            }
            if let V::Error(_) = bv {
                return bv;
            }
            if comparison {
                let blank_like = |other: &V| match other {
                    V::Text(_) => V::Text(String::new()),
                    V::Boolean(_) => V::Boolean(false),
                    _ => V::Number(0.0),
                };
                if av == V::Empty {
                    av = blank_like(&bv);
                }
                if bv == V::Empty {
                    bv = blank_like(&av);
                }
                let rank = |v: &V| match v {
                    V::Number(_) | V::Empty => 0,
                    V::Text(_) => 1,
                    V::Boolean(_) => 2,
                    V::Error(_) => 3,
                };
                let cmp = match (&av, &bv) {
                    (V::Text(a), V::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
                    (V::Boolean(a), V::Boolean(b)) => a.cmp(b),
                    (V::Number(a), V::Number(b)) => a.partial_cmp(b).expect("finite numbers"),
                    _ => rank(&av).cmp(&rank(&bv)),
                };
                return V::Boolean(match op.as_str() {
                    "=" => cmp.is_eq(),
                    "<>" => !cmp.is_eq(),
                    "<" => cmp.is_lt(),
                    ">" => cmp.is_gt(),
                    "<=" => !cmp.is_gt(),
                    _ => !cmp.is_lt(),
                });
            }
            numeric_result(number(av).and_then(|a| {
                number(bv).and_then(|b| match op.as_str() {
                    "+" => Ok(a + b),
                    "-" => Ok(a - b),
                    "*" => Ok(a * b),
                    "/" if b == 0.0 => Err("#DIV/0!".into()),
                    "/" => Ok(a / b),
                    _ => Err("#VALUE!".into()),
                })
            }))
        }
        Expr::Call(f, args) => {
            if f == "SUMIF" {
                let criteria = range_cells(&args[0]).expect("validated criteria range");
                let sum = args
                    .get(2)
                    .map(|x| range_cells(x).expect("validated sum range"))
                    .unwrap_or_else(|| criteria.clone());
                let criterion = eval(&args[1], values);
                let mut total = 0.0;
                let mut budget = 1_000_000usize;
                for (test, sum) in criteria.iter().zip(sum) {
                    match criterion_matches(&raw_value(values, test), &criterion, &mut budget) {
                        Err(e) => return V::Error(e),
                        Ok(false) => {}
                        Ok(true) => match raw_value(values, &sum) {
                            V::Number(n) => total += n,
                            V::Error(e) => return V::Error(e),
                            _ => {}
                        },
                    }
                }
                return numeric_result(Ok(total));
            }
            if f == "IFERROR" {
                let value = eval(&args[0], values);
                return if matches!(value, V::Error(_)) {
                    eval(&args[1], values)
                } else {
                    value
                };
            }
            if f == "IF" {
                let condition = match eval(&args[0], values) {
                    V::Boolean(b) => Ok(b),
                    V::Text(t)
                        if t.eq_ignore_ascii_case("TRUE") || t.eq_ignore_ascii_case("FALSE") =>
                    {
                        Ok(t.eq_ignore_ascii_case("TRUE"))
                    }
                    v => number(v).map(|n| n != 0.0),
                };
                return match condition {
                    Err(e) => V::Error(e),
                    Ok(true) => eval(&args[1], values),
                    Ok(false) => args
                        .get(2)
                        .map(|x| eval(x, values))
                        .unwrap_or(V::Boolean(false)),
                };
            }
            if f == "ABS" {
                return numeric_result(number(eval(&args[0], values)).map(f64::abs));
            }
            let mut numbers = Vec::new();
            for arg in args {
                let mut incoming = Vec::new();
                match arg {
                    Expr::Range(a, b) => {
                        for row in a.row.min(b.row)..=a.row.max(b.row) {
                            for col in a.col.min(b.col)..=a.col.max(b.col) {
                                incoming.push((
                                    raw_value(
                                        values,
                                        &CellReference::new(a.sheet.clone(), col, row),
                                    ),
                                    true,
                                ));
                            }
                        }
                    }
                    Expr::Reference(r) => incoming.push((raw_value(values, r), true)),
                    x => incoming.push((eval(x, values), false)),
                }
                for (v, from_reference) in incoming {
                    if f == "COUNT" {
                        if let V::Number(n) = v {
                            numbers.push(n);
                        } else if !from_reference {
                            if let Ok(n) = number(v) {
                                numbers.push(n);
                            }
                        }
                    } else {
                        match v {
                            V::Number(n) => numbers.push(n),
                            V::Error(e) => return V::Error(e),
                            V::Empty => {}
                            v if from_reference => {
                                let _ = v;
                            }
                            v => match number(v) {
                                Ok(n) => numbers.push(n),
                                Err(e) => return V::Error(e),
                            },
                        }
                    }
                }
            }
            numeric_result(match f.as_str() {
                "COUNT" => Ok(numbers.len() as f64),
                "SUM" => Ok(numbers.iter().sum()),
                "PRODUCT" => Ok(if numbers.is_empty() {
                    0.0
                } else {
                    numbers.iter().product()
                }),
                "AVERAGE" | "AVG" if numbers.is_empty() => Err("#DIV/0!".into()),
                "AVERAGE" | "AVG" => Ok(numbers.iter().sum::<f64>() / numbers.len() as f64),
                "MIN" => Ok(numbers.into_iter().reduce(f64::min).unwrap_or(0.0)),
                "MAX" => Ok(numbers.into_iter().reduce(f64::max).unwrap_or(0.0)),
                _ => Err("#NAME?".into()),
            })
        }
        _ => V::Error("#VALUE!".into()),
    }
}

/// In-memory calculation session; it never mutates or saves the source workbook.
pub struct WorkbookCalculator {
    workbook: Workbook,
    graph: DependencyGraph,
    formulas: BTreeMap<CellReference, Expr>,
    values: BTreeMap<CellReference, CalculationValue>,
    dependency_count: usize,
}
#[derive(Debug, Clone)]
pub struct Recalculation {
    pub evaluated: Vec<CellReference>,
}
impl WorkbookCalculator {
    pub fn new(workbook: &Workbook) -> Result<Self, RecalcError> {
        let mut calc = Self {
            workbook: workbook.clone(),
            graph: DependencyGraph::new(),
            formulas: BTreeMap::new(),
            values: BTreeMap::new(),
            dependency_count: 0,
        };
        for sheet in workbook.sheets() {
            if !matches!(sheet.kind(), SheetKind::Worksheet) {
                continue;
            }
            for (&address, cell) in sheet.cells() {
                let reference = CellReference::new(
                    Some(sheet.name().to_owned()),
                    address.col() as usize,
                    address.row() as usize,
                );
                if let Some(formula) = &cell.formula {
                    if formula.kind != FormulaKind::Normal {
                        return Err(RecalcError::Unsupported {
                            cell: reference.to_a1(),
                            reason: "shared, array and data-table formulas are unsupported".into(),
                        });
                    }
                    calc.set_formula(sheet.name(), address, &formula.text)?;
                } else {
                    calc.values.insert(
                        reference,
                        CalculationValue::from_source(cell.value.as_ref()),
                    );
                }
            }
        }
        Ok(calc)
    }
    fn address(&self, sheet: &str, address: CellAddress) -> Result<CellReference, RecalcError> {
        let sheet =
            canonical_sheet(&self.workbook, sheet).map_err(|reason| RecalcError::Unsupported {
                cell: address.to_a1(),
                reason,
            })?;
        Ok(CellReference::new(
            Some(sheet),
            address.col() as usize,
            address.row() as usize,
        ))
    }
    pub fn formula_count(&self) -> usize {
        self.formulas.len()
    }
    /// Returns no stale value for a formula awaiting recalculation.
    pub fn value(&self, sheet: &str, address: CellAddress) -> Option<&CalculationValue> {
        let cell = self.address(sheet, address).ok()?;
        if self.formulas.contains_key(&cell) && self.graph.is_dirty(&cell) {
            None
        } else {
            self.values.get(&cell)
        }
    }
    /// Replaces any existing formula with a typed input (None clears the cell).
    pub fn set_input(
        &mut self,
        sheet: &str,
        address: CellAddress,
        value: Option<TypedValue>,
    ) -> Result<(), RecalcError> {
        let cell = self.address(sheet, address)?;
        if matches!(&value, Some(TypedValue::Number{value, ..}) if !value.is_finite()) {
            return Err(RecalcError::Unsupported {
                cell: cell.to_a1(),
                reason: "non-finite numeric input".into(),
            });
        }
        self.dependency_count -= self.graph.get_dependencies(&cell).map_or(0, BTreeSet::len);
        self.graph
            .register_cell_with_dependencies(cell.clone(), None, BTreeSet::new());
        self.formulas.remove(&cell);
        self.values
            .insert(cell, CalculationValue::from_source(value.as_ref()));
        Ok(())
    }
    /// Parse and resolve the complete candidate before touching existing graph/value state.
    pub fn set_formula(
        &mut self,
        sheet: &str,
        address: CellAddress,
        formula: &str,
    ) -> Result<(), RecalcError> {
        let cell = self.address(sheet, address)?;
        let build = || -> Result<(Expr, BTreeSet<CellReference>), String> {
            let mut node_budget = MAX_RESOLVED_NODES;
            let expr = resolve(
                Parser::parse(formula)?,
                &self.workbook,
                cell.sheet.as_deref().unwrap(),
                &mut Vec::new(),
                false,
                0,
                &mut node_budget,
            )?;
            let mut deps = BTreeSet::new();
            dependencies(&expr, &mut deps, &mut 0, false)?;
            Ok((expr, deps))
        };
        let (expr, deps) = build().map_err(|reason| RecalcError::Unsupported {
            cell: cell.to_a1(),
            reason,
        })?;
        let dependency_count = self.dependency_count
            - self.graph.get_dependencies(&cell).map_or(0, BTreeSet::len)
            + deps.len();
        if dependency_count > MAX_TOTAL_DEPENDENCIES {
            return Err(RecalcError::Unsupported {
                cell: cell.to_a1(),
                reason: "workbook dependency quota exceeded".into(),
            });
        }
        self.dependency_count = dependency_count;
        self.graph
            .register_cell_with_dependencies(cell.clone(), Some(formula), deps);
        self.formulas.insert(cell.clone(), expr);
        self.values.remove(&cell);
        Ok(())
    }
    pub fn recalculate(&mut self) -> Result<Recalculation, RecalcError> {
        let order = self.graph.topological_sort()?;
        self.evaluate(order)
    }
    pub fn recalculate_dirty(&mut self) -> Result<Recalculation, RecalcError> {
        let order = self.graph.get_evaluation_order_for_dirty()?;
        self.evaluate(order)
    }
    fn evaluate(&mut self, order: Vec<CellReference>) -> Result<Recalculation, RecalcError> {
        // Topology/cycle validation has completed before this point. Evaluation
        // returns error *values*, not fallible operations, so no full value-map
        // clone is needed to protect a rejected calculation request.
        for cell in &order {
            let value = eval(&self.formulas[cell], &self.values);
            self.values.insert(cell.clone(), value);
        }
        self.graph.clear_all_dirty();
        Ok(Recalculation { evaluated: order })
    }
}
