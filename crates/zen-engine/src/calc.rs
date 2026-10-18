use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

/// Cell Coordinate (0-indexed col and row, e.g. A1 -> col=0, row=0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellCoord {
    pub col: usize,
    pub row: usize,
}

impl CellCoord {
    pub fn new(col: usize, row: usize) -> Self {
        Self { col, row }
    }

    /// Convert A1 notation to CellCoord (e.g., "B3" -> col: 1, row: 2)
    pub fn from_a1(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_uppercase();
        let s = s.strip_prefix('$').unwrap_or(&s);
        let split = s.bytes().position(|c| !c.is_ascii_alphabetic())?;
        let col_str = &s[..split];
        let row_str = s[split..].strip_prefix('$').unwrap_or(&s[split..]);
        if col_str.is_empty() || row_str.is_empty() || !row_str.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }

        let mut col: usize = 0;
        for c in col_str.chars() {
            col = col.checked_mul(26)?.checked_add(c as usize - 'A' as usize + 1)?;
        }
        col = col.checked_sub(1)?;

        let row: usize = row_str.parse::<usize>().ok()?.checked_sub(1)?;

        Some(Self { col, row })
    }

    /// Convert CellCoord to A1 notation (e.g., col: 1, row: 2 -> "B3")
    pub fn to_a1(&self) -> String {
        let mut col = self.col + 1;
        let mut col_name = String::new();
        while col > 0 {
            let rem = (col - 1) % 26;
            col_name.insert(0, (b'A' + rem as u8) as char);
            col = (col - 1) / 26;
        }
        format!("{}{}", col_name, self.row + 1)
    }
}

/// Evaluated Cell Value.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    Number(f64),
    Text(String),
    Error(String),
}

impl CellValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            CellValue::Text(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn display_string(&self) -> String {
        match self {
            CellValue::Empty => String::new(),
            CellValue::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{:.0}", n)
                } else {
                    format!("{:.2}", n)
                }
            }
            CellValue::Text(s) => s.clone(),
            CellValue::Error(e) if e == "REF!" => "#REF!".to_owned(),
            CellValue::Error(e) => format!("#ERR: {e}"),
        }
    }
}

/// Fully qualified cell reference with optional sheet name.
/// e.g. "Sheet1!A1", "'Fact_Transactions_30K'!E2", or "A1"
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellReference {
    pub sheet: Option<String>,
    pub col: usize,
    pub row: usize,
}

impl CellReference {
    pub fn new(sheet: Option<String>, col: usize, row: usize) -> Self {
        Self { sheet, col, row }
    }

    pub fn from_coord(sheet: Option<String>, coord: CellCoord) -> Self {
        Self {
            sheet,
            col: coord.col,
            row: coord.row,
        }
    }

    pub fn coord(&self) -> CellCoord {
        CellCoord::new(self.col, self.row)
    }

    pub fn to_a1(&self) -> String {
        let coord_a1 = self.coord().to_a1();
        match &self.sheet {
            Some(s) => {
                if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    format!("{}!{}", s, coord_a1)
                } else {
                    format!("'{}'!{}", s.replace('\'', "''"), coord_a1)
                }
            }
            None => coord_a1,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let (sheet, coord) = parse_cell_reference(s)?;
        Some(Self {
            sheet,
            col: coord.col,
            row: coord.row,
        })
    }
}

impl std::fmt::Display for CellReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

/// Error occurring during formula dependency extraction or validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyExtractionError {
    RangeQuotaExceeded {
        range: String,
        cell_count: u64,
        max_allowed: u64,
    },
    InvalidCellReference(String),
}

impl std::fmt::Display for DependencyExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RangeQuotaExceeded { range, cell_count, max_allowed } => {
                write!(
                    f,
                    "Rectangular range '{}' contains {} cells, which exceeds the dependency extraction quota limit of {} cells",
                    range, cell_count, max_allowed
                )
            }
            Self::InvalidCellReference(s) => {
                write!(f, "Invalid cell reference in formula: '{}'", s)
            }
        }
    }
}

impl std::error::Error for DependencyExtractionError {}

/// Helper function to parse and expand a single cell or a colon-delimited rectangular range.
fn process_cell_or_range_tokens(
    cell_token: &str,
    sheet: Option<&str>,
    chars: &[char],
    i: &mut usize,
    deps: &mut BTreeSet<CellReference>,
) -> Result<(), DependencyExtractionError> {
    if let Some(c1) = CellCoord::from_a1(cell_token) {
        let n = chars.len();
        let mut peek = *i;
        while peek < n && chars[peek].is_whitespace() {
            peek += 1;
        }
        if peek < n && chars[peek] == ':' {
            peek += 1;
            while peek < n && chars[peek].is_whitespace() {
                peek += 1;
            }
            let mut second_sheet = None;
            if peek < n && chars[peek] == '\'' {
                peek += 1;
                let s_start = peek;
                while peek < n && chars[peek] != '\'' {
                    peek += 1;
                }
                if peek < n {
                    second_sheet = Some(chars[s_start..peek].iter().collect::<String>());
                    peek += 1;
                    if peek < n && chars[peek] == '!' {
                        peek += 1;
                    }
                }
            }
            let c2_start = peek;
            while peek < n && (chars[peek].is_alphanumeric() || chars[peek] == '$' || chars[peek] == '_') {
                peek += 1;
            }
            let mut c2_id: String = chars[c2_start..peek].iter().collect();
            if second_sheet.is_none() && peek < n && chars[peek] == '!' {
                second_sheet = Some(c2_id);
                peek += 1;
                let c2_real_start = peek;
                while peek < n && (chars[peek].is_alphanumeric() || chars[peek] == '$') {
                    peek += 1;
                }
                c2_id = chars[c2_real_start..peek].iter().collect();
            }

            if let Some(c2) = CellCoord::from_a1(&c2_id) {
                *i = peek;
                let min_col = c1.col.min(c2.col);
                let max_col = c1.col.max(c2.col);
                let min_row = c1.row.min(c2.row);
                let max_row = c1.row.max(c2.row);

                let cell_count = (max_col - min_col + 1) as u64 * (max_row - min_row + 1) as u64;
                let max_allowed = 100_000u64;
                if cell_count > max_allowed {
                    return Err(DependencyExtractionError::RangeQuotaExceeded {
                        range: format!("{}:{}", cell_token, c2_id),
                        cell_count,
                        max_allowed,
                    });
                }

                let target_sheet = second_sheet.or_else(|| sheet.map(|s| s.to_string()));
                for r in min_row..=max_row {
                    for c in min_col..=max_col {
                        deps.insert(CellReference {
                            sheet: target_sheet.clone(),
                            col: c,
                            row: r,
                        });
                    }
                }
                return Ok(());
            } else {
                return Err(DependencyExtractionError::InvalidCellReference(format!("{}:{}", cell_token, c2_id)));
            }
        }

        deps.insert(CellReference {
            sheet: sheet.map(|s| s.to_string()),
            col: c1.col,
            row: c1.row,
        });
        Ok(())
    } else {
        Ok(())
    }
}

/// Extract all cell references and expanded rectangular ranges from a formula expression string.
pub fn extract_formula_dependencies(
    formula: &str,
    default_sheet: Option<&str>,
) -> Result<BTreeSet<CellReference>, DependencyExtractionError> {
    let mut deps = BTreeSet::new();
    let expr = formula.strip_prefix('=').unwrap_or(formula).trim();
    if expr.is_empty() {
        return Ok(deps);
    }

    let chars: Vec<char> = expr.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        if chars[i] == '"' {
            i += 1;
            while i < n && chars[i] != '"' {
                i += 1;
            }
            if i < n {
                i += 1;
            }
            continue;
        }

        if chars[i] == '\'' || chars[i].is_alphabetic() || chars[i] == '$' || chars[i] == '_' {
            let mut sheet_name: Option<String> = None;

            if chars[i] == '\'' {
                i += 1;
                let sheet_start = i;
                while i < n && chars[i] != '\'' {
                    i += 1;
                }
                if i < n {
                    let s_str: String = chars[sheet_start..i].iter().collect();
                    i += 1;
                    if i < n && chars[i] == '!' {
                        i += 1;
                        sheet_name = Some(s_str);
                    } else {
                        continue;
                    }
                } else {
                    break;
                }
            }

            let id_start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '$' || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            let first_id: String = chars[id_start..i].iter().collect();

            if sheet_name.is_none() && i < n && chars[i] == '!' {
                sheet_name = Some(first_id);
                i += 1;
                let cell_start = i;
                while i < n && (chars[i].is_alphanumeric() || chars[i] == '$') {
                    i += 1;
                }
                let cell_part: String = chars[cell_start..i].iter().collect();
                process_cell_or_range_tokens(
                    &cell_part,
                    sheet_name.as_deref().or(default_sheet),
                    &chars,
                    &mut i,
                    &mut deps,
                )?;
            } else if sheet_name.is_some() {
                process_cell_or_range_tokens(
                    &first_id,
                    sheet_name.as_deref().or(default_sheet),
                    &chars,
                    &mut i,
                    &mut deps,
                )?;
            } else {
                let mut peek = i;
                while peek < n && chars[peek].is_whitespace() {
                    peek += 1;
                }
                if peek < n && chars[peek] == '(' {
                    continue;
                }

                process_cell_or_range_tokens(
                    &first_id,
                    default_sheet,
                    &chars,
                    &mut i,
                    &mut deps,
                )?;
            }
        } else {
            i += 1;
        }
    }

    Ok(deps)
}

/// Topological sort error with precise distinction between confirmed cycle members and downstream blocked nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircularDependencyError {
    /// Nodes confirmed to be part of directed cycle(s).
    pub cycle_nodes: Vec<CellReference>,
    /// Nodes that are not in a cycle themselves, but cannot be evaluated because they depend on cycle nodes.
    pub blocked_downstream_nodes: Vec<CellReference>,
    /// All remaining unresolved nodes (cycles + blocked downstream).
    pub unresolved_nodes: Vec<CellReference>,
}

impl std::fmt::Display for CircularDependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cycle_str = self
            .cycle_nodes
            .iter()
            .map(|c| c.to_a1())
            .collect::<Vec<_>>()
            .join(", ");
        let blocked_str = self
            .blocked_downstream_nodes
            .iter()
            .map(|c| c.to_a1())
            .collect::<Vec<_>>()
            .join(", ");
        let unres_str = self
            .unresolved_nodes
            .iter()
            .map(|c| c.to_a1())
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "Topological recalculation blocked: confirmed cycle nodes: [{}]; blocked downstream nodes: [{}]; total unresolved: [{}]",
            cycle_str, blocked_str, unres_str
        )
    }
}

impl std::error::Error for CircularDependencyError {}

/// Iterative SCC partitioning; large cycles must not recurse on the thread stack.
fn partition_unresolved_nodes(
    unresolved: &[CellReference],
    dependencies: &HashMap<CellReference, BTreeSet<CellReference>>,
) -> (Vec<CellReference>, Vec<CellReference>) {
    let indices: HashMap<_,_> = unresolved.iter().enumerate().map(|(i,cell)| (cell,i)).collect();
    let mut edges = vec![Vec::new();unresolved.len()];
    let mut reverse = edges.clone();
    for (i,cell) in unresolved.iter().enumerate() {
        if let Some(deps) = dependencies.get(cell) {
            for dep in deps {
                if let Some(&j) = indices.get(dep) {edges[i].push(j);reverse[j].push(i);}
            }
        }
    }
    let mut visited=vec![false;unresolved.len()];
    let mut finished=Vec::new();
    for start in 0..unresolved.len() {
        if visited[start] {continue;}
        visited[start]=true;
        let mut stack=vec![(start,0usize)];
        while let Some((node,next))=stack.last_mut() {
            if *next < edges[*node].len() {
                let neighbor=edges[*node][*next]; *next+=1;
                if !visited[neighbor] {visited[neighbor]=true;stack.push((neighbor,0));}
            } else {let (node,_)=stack.pop().unwrap();finished.push(node);}
        }
    }
    visited.fill(false);
    let mut cycle_nodes = Vec::new();
    let mut blocked_downstream = Vec::new();
    for start in finished.into_iter().rev() {
        if visited[start] {continue;}
        let mut component=Vec::new();let mut stack=vec![start];visited[start]=true;
        while let Some(node)=stack.pop() {
            component.push(node);
            for &neighbor in &reverse[node] {
                if !visited[neighbor] {visited[neighbor]=true;stack.push(neighbor);}
            }
        }
        let cyclic=component.len()>1 || edges[start].contains(&start);
        for node in component {
            if cyclic {cycle_nodes.push(unresolved[node].clone());}
            else {blocked_downstream.push(unresolved[node].clone());}
        }
    }

    cycle_nodes.sort();
    blocked_downstream.sort();
    (cycle_nodes, blocked_downstream)
}

/// Directed Acyclic Graph (DAG) dependency tracker and Kahn topological recalculator for Sheets.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// Formula expressions registered for cells
    formulas: HashMap<CellReference, String>,
    /// node -> predecessors (the cells this node reads from to evaluate)
    dependencies: HashMap<CellReference, BTreeSet<CellReference>>,
    /// node -> successors (the cells that depend on this node and need recomputation when this node changes)
    dependents: HashMap<CellReference, BTreeSet<CellReference>>,
    /// Set of cells marked dirty
    dirty_nodes: HashSet<CellReference>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.formulas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.formulas.is_empty()
    }

    pub fn get_formula(&self, cell: &CellReference) -> Option<&str> {
        self.formulas.get(cell).map(|s| s.as_str())
    }

    pub fn get_dependencies(&self, cell: &CellReference) -> Option<&BTreeSet<CellReference>> {
        self.dependencies.get(cell)
    }

    pub fn get_dependents(&self, cell: &CellReference) -> Option<&BTreeSet<CellReference>> {
        self.dependents.get(cell)
    }

    pub fn is_dirty(&self, cell: &CellReference) -> bool {
        self.dirty_nodes.contains(cell)
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty_nodes.len()
    }

    pub fn clear_dirty(&mut self, cell: &CellReference) {
        self.dirty_nodes.remove(cell);
    }

    pub fn clear_all_dirty(&mut self) {
        self.dirty_nodes.clear();
    }

    /// Register or update a cell. If formula is Some, dependencies are automatically extracted.
    /// Modifying or deleting a formula cell cascades dirty state to all downstream dependents.
    pub fn register_cell(
        &mut self,
        cell: CellReference,
        formula: Option<&str>,
        default_sheet: Option<&str>,
    ) -> Result<HashSet<CellReference>, DependencyExtractionError> {
        // Validate before removing the old edges: rejected edits must be atomic.
        let deps = formula.map(|expr| {
            extract_formula_dependencies(expr.trim(), cell.sheet.as_deref().or(default_sheet))
        }).transpose()?;
        Ok(self.register_cell_with_dependencies(cell, formula, deps.unwrap_or_default()))
    }

    /// Register a formula whose references have already been resolved by a parser.
    /// This allows named ranges to participate in exactly the same dependency graph.
    pub fn register_cell_with_dependencies(
        &mut self,
        cell: CellReference,
        formula: Option<&str>,
        deps: BTreeSet<CellReference>,
    ) -> HashSet<CellReference> {
        // Clean up previous dependency edges for this cell
        if let Some(old_deps) = self.dependencies.remove(&cell) {
            for dep in old_deps {
                if let Some(succs) = self.dependents.get_mut(&dep) {
                    succs.remove(&cell);
                }
            }
        }

        if let Some(expr) = formula {
            let formula_str = expr.trim().to_string();

            for dep in &deps {
                self.dependents
                    .entry(dep.clone())
                    .or_default()
                    .insert(cell.clone());
            }

            self.dependencies.insert(cell.clone(), deps);
            self.formulas.insert(cell.clone(), formula_str);
        } else {
            self.formulas.remove(&cell);
        }

        // When a cell is registered, updated, or turned into a raw value,
        // it and all downstream dependents must consistently be marked dirty.
        let affected = self.mark_dirty(&cell);
        affected
    }

    /// Remove a cell from the graph entirely, cascading dirty marks to its downstream dependents.
    pub fn remove_cell(&mut self, cell: &CellReference) -> HashSet<CellReference> {
        let affected = self.mark_dirty(cell);

        if let Some(old_deps) = self.dependencies.remove(cell) {
            for dep in old_deps {
                if let Some(succs) = self.dependents.get_mut(&dep) {
                    succs.remove(cell);
                }
            }
        }
        // Other formulas still refer to this address even when it becomes blank.
        // Retain incoming references so recreating the value invalidates them.
        self.formulas.remove(cell);
        self.dirty_nodes.remove(cell);

        affected
    }

    /// Mark a single cell dirty and propagate dirtiness downstream along the dependency graph.
    /// Visited set prevents infinite loops in cycles; does NOT use already-dirty as a termination condition.
    pub fn mark_dirty(&mut self, cell: &CellReference) -> HashSet<CellReference> {
        let mut affected = HashSet::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        self.dirty_nodes.insert(cell.clone());
        affected.insert(cell.clone());
        visited.insert(cell.clone());
        queue.push_back(cell.clone());

        while let Some(current) = queue.pop_front() {
            if let Some(succs) = self.dependents.get(&current) {
                for succ in succs {
                    self.dirty_nodes.insert(succ.clone());
                    affected.insert(succ.clone());
                    if visited.insert(succ.clone()) {
                        queue.push_back(succ.clone());
                    }
                }
            }
        }

        affected
    }

    /// Propagate dirtiness from a batch of modified cells to all downstream dependent cells.
    pub fn propagate_dirty(&mut self, changed_cells: &[CellReference]) -> HashSet<CellReference> {
        let mut total_affected = HashSet::new();
        for cell in changed_cells {
            let affected = self.mark_dirty(cell);
            total_affected.extend(affected);
        }
        total_affected
    }

    /// Full topological sort using Kahn's algorithm over all registered formula cells.
    /// If cycles or downstream blockages exist, returns Tarjan-partitioned CircularDependencyError.
    pub fn topological_sort(&self) -> Result<Vec<CellReference>, CircularDependencyError> {
        let all_formula_cells: HashSet<&CellReference> = self.formulas.keys().collect();
        let mut in_degree: HashMap<&CellReference, usize> = HashMap::new();

        for &cell in &all_formula_cells {
            let deg = self
                .dependencies
                .get(cell)
                .map(|deps| deps.iter().filter(|dep| all_formula_cells.contains(dep)).count())
                .unwrap_or(0);
            in_degree.insert(cell, deg);
        }

        let mut ready: Vec<&CellReference> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&cell, _)| cell)
            .collect();
        ready.sort();
        let mut queue = VecDeque::from(ready);

        let mut order = Vec::with_capacity(all_formula_cells.len());

        while let Some(cell) = queue.pop_front() {
            order.push(cell.clone());

            if let Some(succs) = self.dependents.get(cell) {
                for succ in succs {
                    if all_formula_cells.contains(succ) {
                        if let Some(deg) = in_degree.get_mut(succ) {
                            *deg = deg.saturating_sub(1);
                            if *deg == 0 {
                                queue.push_back(succ);
                            }
                        }
                    }
                }
            }
        }

        if order.len() < all_formula_cells.len() {
            let mut unresolved = Vec::new();
            for (&cell, &deg) in &in_degree {
                if deg > 0 {
                    unresolved.push(cell.clone());
                }
            }
            unresolved.sort();
            let (cycle_nodes, blocked_downstream_nodes) =
                partition_unresolved_nodes(&unresolved, &self.dependencies);
            return Err(CircularDependencyError {
                cycle_nodes,
                blocked_downstream_nodes,
                unresolved_nodes: unresolved,
            });
        }

        Ok(order)
    }

    /// Incremental topological sort restricted to dirty formula cells.
    /// Only dirty cells are ordered for recalculation, maintaining correct dependency ordering.
    pub fn get_evaluation_order_for_dirty(&self) -> Result<Vec<CellReference>, CircularDependencyError> {
        let dirty_formula_cells: HashSet<&CellReference> = self
            .dirty_nodes
            .iter()
            .filter(|c| self.formulas.contains_key(c))
            .collect();

        if dirty_formula_cells.is_empty() {
            return Ok(Vec::new());
        }

        let mut in_degree: HashMap<&CellReference, usize> = HashMap::new();

        for &cell in &dirty_formula_cells {
            let deg = self
                .dependencies
                .get(cell)
                .map(|deps| deps.iter().filter(|dep| dirty_formula_cells.contains(dep)).count())
                .unwrap_or(0);
            in_degree.insert(cell, deg);
        }

        let mut ready: Vec<&CellReference> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&cell, _)| cell)
            .collect();
        ready.sort();
        let mut queue = VecDeque::from(ready);

        let mut order = Vec::with_capacity(dirty_formula_cells.len());

        while let Some(cell) = queue.pop_front() {
            order.push(cell.clone());

            if let Some(succs) = self.dependents.get(cell) {
                for succ in succs {
                    if dirty_formula_cells.contains(succ) {
                        if let Some(deg) = in_degree.get_mut(succ) {
                            *deg = deg.saturating_sub(1);
                            if *deg == 0 {
                                queue.push_back(succ);
                            }
                        }
                    }
                }
            }
        }

        if order.len() < dirty_formula_cells.len() {
            let mut unresolved = Vec::new();
            for (&cell, &deg) in &in_degree {
                if deg > 0 {
                    unresolved.push(cell.clone());
                }
            }
            unresolved.sort();
            let (cycle_nodes, blocked_downstream_nodes) =
                partition_unresolved_nodes(&unresolved, &self.dependencies);
            return Err(CircularDependencyError {
                cycle_nodes,
                blocked_downstream_nodes,
                unresolved_nodes: unresolved,
            });
        }

        Ok(order)
    }
}

/// Parse an Excel cell reference with optional sheet name prefix,
/// e.g. "B3", "$B$3", "Sheet1!B3", or "'Fact_Transactions_30K'!E5001".
pub fn parse_cell_reference(s: &str) -> Option<(Option<String>, CellCoord)> {
    let s = s.trim();
    if let Some((sheet_part, cell_part)) = s.split_once('!') {
        let sheet_part = sheet_part.trim();
        let sheet_name = if sheet_part.starts_with('\'') && sheet_part.ends_with('\'') && sheet_part.len() >= 2 {
            sheet_part[1..sheet_part.len() - 1].to_string()
        } else if !sheet_part.is_empty() && sheet_part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            sheet_part.to_string()
        } else {
            return None;
        };
        let coord = CellCoord::from_a1(cell_part.trim())?;
        Some((Some(sheet_name), coord))
    } else {
        let coord = CellCoord::from_a1(s)?;
        Some((None, coord))
    }
}

/// Lightweight formula evaluator directly supporting standard Excel formulas.
pub struct FormulaEngine<'a> {
    resolver: Box<dyn Fn(Option<&str>, CellCoord) -> CellValue + 'a>,
    name_resolver: Option<Box<dyn Fn(&str) -> Option<CellValue> + 'a>>,
}

impl<'a> FormulaEngine<'a> {
    pub fn new(get_cell_fn: &'a dyn Fn(CellCoord) -> CellValue) -> Self {
        Self {
            resolver: Box::new(move |_sheet, coord| get_cell_fn(coord)),
            name_resolver: None,
        }
    }

    pub fn with_resolver(resolver: &'a dyn Fn(Option<&str>, CellCoord) -> CellValue) -> Self {
        Self {
            resolver: Box::new(move |sheet, coord| resolver(sheet, coord)),
            name_resolver: None,
        }
    }

    pub fn with_named_resolver(
        resolver: &'a dyn Fn(Option<&str>, CellCoord) -> CellValue,
        name_resolver: &'a dyn Fn(&str) -> Option<CellValue>,
    ) -> Self {
        Self {
            resolver: Box::new(move |sheet, coord| resolver(sheet, coord)),
            name_resolver: Some(Box::new(move |name| name_resolver(name))),
        }
    }

    pub const MAX_EVAL_DEPTH: usize = 32;

    /// Evaluate formula expression string, e.g. "=SUM(A1:A3)" or "=A1 * 2 + B1"
    pub fn eval(&self, formula: &str) -> CellValue {
        self.eval_with_depth(formula, 0)
    }

    pub fn eval_with_depth(&self, formula: &str, depth: usize) -> CellValue {
        if depth > Self::MAX_EVAL_DEPTH {
            return CellValue::Error("#VALUE!".into());
        }
        let expr = formula.trim();
        if expr.len() > 2048 {
            return CellValue::Error("#VALUE!".into());
        }
        let expr = if let Some(stripped) = expr.strip_prefix('=') {
            stripped.trim()
        } else {
            expr
        };

        if expr.is_empty() {
            return CellValue::Empty;
        }
        if expr.contains("#REF!") {
            return CellValue::Error("REF!".to_owned());
        }

        // String literal support, e.g. "Fallback"
        if expr.starts_with('"') && expr.ends_with('"') && expr.len() >= 2 {
            return CellValue::Text(expr[1..expr.len() - 1].to_string());
        }

        // Direct reference (local or cross-sheet) preserves typed value.
        if let Some((sheet_opt, coord)) = parse_cell_reference(expr) {
            return match (self.resolver)(sheet_opt.as_deref(), coord) {
                CellValue::Empty => CellValue::Number(0.0),
                value => value,
            };
        }

        // Direct named range identifier reference
        if let Some(nr) = &self.name_resolver {
            if let Some(val) = nr(expr) {
                return val;
            }
        }

        if let Some((func_name, args_str)) = Self::parse_function(expr) {
            return self.eval_func(&func_name, args_str, depth);
        }

        match self.eval_arithmetic(expr, depth) {
            Ok(val) => CellValue::Number(val),
            Err(err_msg) => {
                if let Some((sheet_opt, coord)) = parse_cell_reference(expr) {
                    (self.resolver)(sheet_opt.as_deref(), coord)
                } else {
                    CellValue::Error(err_msg.into())
                }
            }
        }
    }

    fn parse_function(expr: &str) -> Option<(String, &str)> {
        let upper = expr.to_ascii_uppercase();
        // Place IFERROR before IF to prevent prefix shadowing
        for func in &["IFERROR", "IF", "PRODUCT", "SUM", "AVERAGE", "AVG", "MAX", "MIN", "COUNT", "ABS"] {
            if upper.starts_with(func) && expr[func.len()..].trim_start().starts_with('(') {
                let open_pos = expr.find('(')?;
                let mut depth = 0usize;
                let mut close_pos = None;
                for (offset, character) in expr[open_pos..].char_indices() {
                    match character {
                        '(' => depth += 1,
                        ')' => {
                            depth = depth.checked_sub(1)?;
                            if depth == 0 {
                                close_pos = Some(open_pos + offset);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let close_pos = close_pos?;
                // Only a complete function call may bypass arithmetic parsing.
                if !expr[close_pos + 1..].trim().is_empty() {
                    return None;
                }
                if close_pos > open_pos {
                    let args = &expr[open_pos + 1..close_pos];
                    return Some((func.to_string(), args));
                }
            }
        }
        None
    }

    fn eval_func(&self, func: &str, args_str: &str, depth: usize) -> CellValue {
        match func {
            "ABS" => {
                let val = self.eval_with_depth(args_str.trim(), depth + 1);
                match val.as_f64() {
                    Some(n) => CellValue::Number(n.abs()),
                    None => {
                        if matches!(val, CellValue::Error(_)) {
                            val
                        } else {
                            CellValue::Error("#VALUE!".into())
                        }
                    }
                }
            }
            "IF" => self.eval_if(args_str, depth + 1),
            "IFERROR" => {
                let args = Self::split_function_args(args_str);
                if args.len() < 2 {
                    return CellValue::Error("#VALUE!".into());
                }
                let val = self.eval_with_depth(args[0].trim(), depth + 1);
                if matches!(val, CellValue::Error(_)) {
                    self.eval_with_depth(args[1].trim(), depth + 1)
                } else {
                    val
                }
            }
            "PRODUCT" => {
                let numbers = match self.collect_range_numbers(args_str, depth + 1) {
                    Ok(n) => n,
                    Err(err_val) => return err_val,
                };
                if numbers.is_empty() {
                    CellValue::Number(0.0)
                } else {
                    let mut prod = 1.0;
                    for n in numbers {
                        prod *= n;
                        if !prod.is_finite() {
                            return CellValue::Error("#NUM!".into());
                        }
                    }
                    CellValue::Number(prod)
                }
            }
            "SUM" | "AVERAGE" | "AVG" | "MAX" | "MIN" | "COUNT" => {
                let numbers = match self.collect_range_numbers(args_str, depth + 1) {
                    Ok(n) => n,
                    Err(err_val) => return err_val,
                };
                if numbers.is_empty() && (func == "AVERAGE" || func == "AVG") {
                    return CellValue::Error("#DIV/0!".into());
                }
                match func {
                    "SUM" => {
                        let sum: f64 = numbers.iter().sum();
                        if !sum.is_finite() {
                            CellValue::Error("#NUM!".into())
                        } else {
                            CellValue::Number(sum)
                        }
                    }
                    "AVERAGE" | "AVG" => {
                        let sum: f64 = numbers.iter().sum();
                        let avg = sum / numbers.len() as f64;
                        if !avg.is_finite() {
                            CellValue::Error("#NUM!".into())
                        } else {
                            CellValue::Number(avg)
                        }
                    }
                    "MAX" => {
                        let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        CellValue::Number(if max.is_infinite() { 0.0 } else { max })
                    }
                    "MIN" => {
                        let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
                        CellValue::Number(if min.is_infinite() { 0.0 } else { min })
                    }
                    "COUNT" => CellValue::Number(numbers.len() as f64),
                    _ => unreachable!(),
                }
            }
            _ => CellValue::Error("NAME?".into()),
        }
    }

    fn split_function_args(args_str: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut current = String::new();
        let mut depth = 0usize;
        let mut in_quote = false;
        for c in args_str.chars() {
            match c {
                '"' => {
                    in_quote = !in_quote;
                    current.push(c);
                }
                '(' if !in_quote => {
                    depth += 1;
                    current.push(c);
                }
                ')' if !in_quote => {
                    depth = depth.saturating_sub(1);
                    current.push(c);
                }
                ',' if !in_quote && depth == 0 => {
                    args.push(current.trim().to_string());
                    current.clear();
                }
                _ => current.push(c),
            }
        }
        if !current.trim().is_empty() {
            args.push(current.trim().to_string());
        }
        args
    }

    fn eval_if(&self, args_str: &str, depth: usize) -> CellValue {
        let args = Self::split_function_args(args_str);
        if args.len() < 2 {
            return CellValue::Error("SYNTAX".into());
        }
        let cond_result = self.eval_condition(&args[0], depth);
        let branch = if cond_result {
            &args[1]
        } else if args.len() >= 3 {
            &args[2]
        } else {
            return CellValue::Number(0.0);
        };
        let branch = branch.trim();
        if branch.starts_with('"') && branch.ends_with('"') && branch.len() >= 2 {
            CellValue::Text(branch[1..branch.len() - 1].to_string())
        } else {
            self.eval_with_depth(branch, depth + 1)
        }
    }

    fn eval_condition(&self, cond_str: &str, depth: usize) -> bool {
        let cond = cond_str.trim();
        for op in &["<=", ">=", "!=", "<>", "==", "=", "<", ">"] {
            if let Some(pos) = Self::find_operator_at_depth_0(cond, op) {
                let left_str = cond[..pos].trim();
                let right_str = cond[pos + op.len()..].trim();
                let left_val = self.eval_with_depth(left_str, depth + 1);
                let right_val = self.eval_with_depth(right_str, depth + 1);
                if let (Some(l), Some(r)) = (left_val.as_f64(), right_val.as_f64()) {
                    return match *op {
                        "<=" => l <= r,
                        ">=" => l >= r,
                        "!=" | "<>" => (l - r).abs() > 1e-9,
                        "=" | "==" => (l - r).abs() <= 1e-9,
                        "<" => l < r,
                        ">" => l > r,
                        _ => false,
                    };
                } else {
                    let l_str = left_val.display_string();
                    let r_str = right_val.display_string();
                    return match *op {
                        "=" | "==" => l_str == r_str,
                        "!=" | "<>" => l_str != r_str,
                        _ => false,
                    };
                }
            }
        }
        match self.eval_with_depth(cond, depth + 1) {
            CellValue::Number(n) => n != 0.0,
            CellValue::Text(t) => !t.is_empty(),
            _ => false,
        }
    }

    fn find_operator_at_depth_0(s: &str, op: &str) -> Option<usize> {
        let mut depth = 0usize;
        let mut in_quote = false;
        let s_chars: Vec<char> = s.chars().collect();
        let op_chars: Vec<char> = op.chars().collect();
        let mut i = 0;
        while i < s_chars.len() {
            match s_chars[i] {
                '"' => in_quote = !in_quote,
                '(' if !in_quote => depth += 1,
                ')' if !in_quote => depth = depth.saturating_sub(1),
                _ if !in_quote && depth == 0 => {
                    if i + op_chars.len() <= s_chars.len()
                        && s_chars[i..i + op_chars.len()] == op_chars[..]
                    {
                        let byte_pos: usize = s_chars[..i].iter().map(|c| c.len_utf8()).sum();
                        return Some(byte_pos);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn collect_range_numbers(&self, args_str: &str, depth: usize) -> Result<Vec<f64>, CellValue> {
        if depth > Self::MAX_EVAL_DEPTH {
            return Err(CellValue::Error("#VALUE!".into()));
        }
        let mut results = Vec::new();
        for arg in Self::split_function_args(args_str) {
            let arg = arg.trim();
            if let Some((start_s, end_s)) = arg.split_once(':') {
                let (sheet1, c1_opt) = match parse_cell_reference(start_s) {
                    Some((s, c)) => (s, Some(c)),
                    None => (None, None),
                };
                let (_sheet2, c2_opt) = match parse_cell_reference(end_s) {
                    Some((s, c)) => (s, Some(c)),
                    None => (None, None),
                };
                if let (Some(c1), Some(c2)) = (c1_opt, c2_opt) {
                    let min_col = c1.col.min(c2.col);
                    let max_col = c1.col.max(c2.col);
                    let min_row = c1.row.min(c2.row);
                    let max_row = c1.row.max(c2.row);

                    // 保护配额：单区域最大枚举 1,000,000 个单元格，超大范围防止 CPU / 内存 DoS
                    let total_cells = (max_col - min_col + 1) as u64 * (max_row - min_row + 1) as u64;
                    if total_cells > 1_000_000 {
                        // 显式拒绝超限计算，向上返回 #NUM! 公式错误，严禁伪装为 0
                        return Err(CellValue::Error("#NUM!".into()));
                    }

                    for r in min_row..=max_row {
                        for c in min_col..=max_col {
                            let val = (self.resolver)(sheet1.as_deref(), CellCoord::new(c, r));
                            match val {
                                CellValue::Number(n) => {
                                    if !n.is_finite() {
                                        return Err(CellValue::Error("#NUM!".into()));
                                    }
                                    results.push(n);
                                }
                                CellValue::Error(e) => {
                                    return Err(CellValue::Error(e));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            } else if let Some((sheet_opt, coord)) = parse_cell_reference(arg) {
                let val = (self.resolver)(sheet_opt.as_deref(), coord);
                match val {
                    CellValue::Number(n) => results.push(n),
                    CellValue::Error(e) => return Err(CellValue::Error(e)),
                    _ => {}
                }
            } else if let Ok(n) = arg.parse::<f64>() {
                if !n.is_finite() {
                    return Err(CellValue::Error("#NUM!".into()));
                }
                results.push(n);
            } else {
                let sub_val = self.eval_with_depth(arg, depth + 1);
                match sub_val {
                    CellValue::Number(n) => {
                        if !n.is_finite() {
                            return Err(CellValue::Error("#NUM!".into()));
                        }
                        results.push(n);
                    }
                    CellValue::Error(e) => return Err(CellValue::Error(e)),
                    _ => return Err(CellValue::Error("#VALUE!".into())),
                }
            }
        }
        Ok(results)
    }

    fn eval_arithmetic(&self, expr: &str, depth: usize) -> Result<f64, &'static str> {
        if depth > Self::MAX_EVAL_DEPTH {
            return Err("#VALUE!");
        }
        let chars: Vec<char> = expr.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if (c == '+' || c == '-' || c == '*' || c == '/')
                || (c == '(' && tokens.last().map_or(true, |t| matches!(t, Token::Op('+') | Token::Op('-') | Token::Op('*') | Token::Op('/') | Token::Op('('))))
                || c == ')'
            {
                tokens.push(Token::Op(c));
                i += 1;
            } else {
                let mut s = String::new();
                let mut p_depth = 0usize;
                let mut in_quote = false;
                while i < chars.len() {
                    let ch = chars[i];
                    if ch == '\'' {
                        in_quote = !in_quote;
                    }
                    if !in_quote && p_depth == 0 && (ch.is_whitespace() || "+-*/".find(ch).is_some() || (ch == ')' && !s.is_empty())) {
                        break;
                    }
                    match ch {
                        '(' if !in_quote => p_depth += 1,
                        ')' if !in_quote => p_depth = p_depth.checked_sub(1).ok_or("SYNTAX")?,
                        _ => {}
                    }
                    s.push(ch);
                    i += 1;
                }
                if p_depth != 0 || in_quote {
                    return Err("SYNTAX");
                }
                if let Ok(num) = s.parse::<f64>() {
                    tokens.push(Token::Num(num));
                } else if let Some((function, arguments)) = Self::parse_function(&s) {
                    let value = self.eval_func(&function, arguments, depth + 1);
                    match value {
                        CellValue::Number(n) => tokens.push(Token::Num(n)),
                        CellValue::Error(e) => {
                            if e == "#DIV/0!" {
                                return Err("#DIV/0!");
                            } else if e == "#NUM!" {
                                return Err("#NUM!");
                            } else {
                                return Err("#VALUE!");
                            }
                        }
                        _ => return Err("#VALUE!"),
                    }
                } else if let Some((sheet_opt, coord)) = parse_cell_reference(&s) {
                    let val = (self.resolver)(sheet_opt.as_deref(), coord);
                    tokens.push(Token::Num(val.as_f64().unwrap_or(0.0)));
                } else if let Some(nr) = &self.name_resolver {
                    if let Some(val) = nr(&s) {
                        tokens.push(Token::Num(val.as_f64().unwrap_or(0.0)));
                    } else {
                        return Err("SYNTAX");
                    }
                } else {
                    return Err("SYNTAX");
                }
            }
        }

        if tokens.is_empty() {
            return Err("SYNTAX");
        }

        let mut parser = ArithmeticParser {
            tokens: &tokens,
            pos: 0,
            depth,
        };
        let val = parser.parse_expr()?;
        if parser.pos < parser.tokens.len() {
            return Err("SYNTAX");
        }
        if val.is_nan() || val.is_infinite() {
            return Err("#NUM!");
        }
        Ok(val)
    }
}

enum Token {
    Num(f64),
    Op(char),
}

struct ArithmeticParser<'a> {
    tokens: &'a [Token],
    pos: usize,
    depth: usize,
}

impl<'a> ArithmeticParser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        self.pos += 1;
        tok
    }

    fn parse_expr(&mut self) -> Result<f64, &'static str> {
        self.depth += 1;
        if self.depth > FormulaEngine::MAX_EVAL_DEPTH {
            return Err("#VALUE!");
        }
        let res = self.parse_add_sub();
        self.depth -= 1;
        res
    }

    fn parse_add_sub(&mut self) -> Result<f64, &'static str> {
        let mut lhs = self.parse_mul_div()?;
        while let Some(Token::Op(op @ ('+' | '-'))) = self.peek() {
            let op = *op;
            self.advance();
            let rhs = self.parse_mul_div()?;
            if op == '+' {
                lhs += rhs;
            } else {
                lhs -= rhs;
            }
            if lhs.is_nan() || lhs.is_infinite() {
                return Err("#NUM!");
            }
        }
        Ok(lhs)
    }

    fn parse_mul_div(&mut self) -> Result<f64, &'static str> {
        let mut lhs = self.parse_unary()?;
        while let Some(Token::Op(op @ ('*' | '/'))) = self.peek() {
            let op = *op;
            self.advance();
            let rhs = self.parse_unary()?;
            if op == '*' {
                lhs *= rhs;
            } else {
                if rhs == 0.0 {
                    return Err("#DIV/0!");
                }
                lhs /= rhs;
            }
            if lhs.is_nan() || lhs.is_infinite() {
                return Err("#NUM!");
            }
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<f64, &'static str> {
        match self.peek() {
            Some(Token::Op('+')) => {
                self.depth += 1;
                if self.depth > FormulaEngine::MAX_EVAL_DEPTH {
                    return Err("#VALUE!");
                }
                self.advance();
                let res = self.parse_unary();
                self.depth -= 1;
                res
            }
            Some(Token::Op('-')) => {
                self.depth += 1;
                if self.depth > FormulaEngine::MAX_EVAL_DEPTH {
                    return Err("#VALUE!");
                }
                self.advance();
                let val = self.parse_unary()?;
                self.depth -= 1;
                Ok(-val)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<f64, &'static str> {
        match self.advance() {
            Some(Token::Num(n)) => Ok(*n),
            Some(Token::Op('(')) => {
                let val = self.parse_expr()?;
                match self.advance() {
                    Some(Token::Op(')')) => Ok(val),
                    _ => Err("SYNTAX"),
                }
            }
            _ => Err("SYNTAX"),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_arithmetic_does_not_discard_trailing_expression() {
        let engine = FormulaEngine::new(&|c| match (c.col, c.row) {
            (0, 0) => CellValue::Number(10.0),
            (0, 1) => CellValue::Number(20.0),
            _ => CellValue::Empty,
        });
        assert_eq!(engine.eval("=SUM(A1:A2)+1"), CellValue::Number(31.0));
        assert_eq!(engine.eval("=2*SUM(A1:A2)-MIN(A1:A2)"), CellValue::Number(50.0));
        assert_eq!(engine.eval("=SUM(A1:A2)/COUNT(A1:A2)"), CellValue::Number(15.0));
        assert_eq!(engine.eval("=SUM(1,-2)+3"), CellValue::Number(2.0));
        assert_eq!(engine.eval("=SUM(A1:A2)garbage"), CellValue::Error("SYNTAX".into()));
        assert_eq!(engine.eval("=SUM(A1:A2"), CellValue::Error("SYNTAX".into()));
    }

    #[test]
    fn test_coord_a1() {
        assert!(CellCoord::from_a1("SUM(A1:A2)garbage").is_none());
        assert!(CellCoord::from_a1("A1+B2").is_none());
        assert_eq!(CellCoord::from_a1("$a$1"), Some(CellCoord::new(0, 0)));
        let coord = CellCoord::from_a1("B3").unwrap();
        assert_eq!(coord.col, 1);
        assert_eq!(coord.row, 2);
        assert_eq!(coord.to_a1(), "B3");

        let coord2 = CellCoord::from_a1("AA1").unwrap();
        assert_eq!(coord2.col, 26);
        assert_eq!(coord2.row, 0);
        assert_eq!(coord2.to_a1(), "AA1");
    }

    #[test]
    fn test_arithmetic() {
        let engine = FormulaEngine::new(&|_| CellValue::Empty);
        let res = engine.eval("=10 + 20 * 3");
        assert_eq!(res, CellValue::Number(70.0));
    }

    #[test]
    fn test_cell_reference_arithmetic() {
        let engine = FormulaEngine::new(&|c| {
            if c == CellCoord::new(0, 0) {
                CellValue::Number(100.0)
            } else if c == CellCoord::new(1, 0) {
                CellValue::Number(5.0)
            } else {
                CellValue::Empty
            }
        });
        let res = engine.eval("=A1 * B1");
        assert_eq!(res, CellValue::Number(500.0));
    }

    #[test]
    fn test_sum_and_average() {
        let engine = FormulaEngine::new(&|c| match (c.col, c.row) {
            (0, 0) => CellValue::Number(10.0),
            (0, 1) => CellValue::Number(20.0),
            (0, 2) => CellValue::Number(30.0),
            _ => CellValue::Empty,
        });
        assert_eq!(engine.eval("=SUM(A1:A3)"), CellValue::Number(60.0));
        assert_eq!(engine.eval("=AVERAGE(A1:A3)"), CellValue::Number(20.0));
        assert_eq!(engine.eval("=MAX(A1:A3)"), CellValue::Number(30.0));
    }

    #[test]
    fn test_parentheses_and_discount() {
        let engine = FormulaEngine::new(&|c| match (c.col, c.row) {
            (4, 5000) => CellValue::Number(255.0),
            (5, 5000) => CellValue::Number(0.05),
            _ => CellValue::Empty,
        });
        assert_eq!(engine.eval("=E5001*(1-F5001)"), CellValue::Number(242.25));
        assert_eq!(engine.eval("=(10 + 20) * (3 - 1)"), CellValue::Number(60.0));
    }

    #[test]
    fn test_cross_sheet_and_if_abs() {
        let engine = FormulaEngine::with_resolver(&|sheet, c| {
            if sheet == Some("Fact_Transactions_30K") && c == CellCoord::new(4, 1) {
                CellValue::Number(255.0)
            } else if sheet == Some("Balance_Sheet_Consolidation") && c == CellCoord::new(1, 14) {
                CellValue::Number(0.0)
            } else {
                CellValue::Empty
            }
        });
        assert_eq!(
            engine.eval("='Fact_Transactions_30K'!E2"),
            CellValue::Number(255.0)
        );
        assert_eq!(
            engine.eval("=IF(ABS(Balance_Sheet_Consolidation!B15)<0.01,\"STRICT_EQUILIBRIUM_PASS\",\"FAIL\")"),
            CellValue::Text("STRICT_EQUILIBRIUM_PASS".into())
        );
    }

    #[test]
    fn test_red_team_rt02_rt03_arithmetic_and_limits() {
        let engine = FormulaEngine::new(&|_| CellValue::Empty);
        // RT-03: 一元负号与二元运算符复合
        assert_eq!(engine.eval("=1/-2"), CellValue::Number(-0.5));
        assert_eq!(engine.eval("=2*-3"), CellValue::Number(-6.0));
        assert_eq!(engine.eval("=1 - -2"), CellValue::Number(3.0));
        assert_eq!(engine.eval("=-5 + 3"), CellValue::Number(-2.0));
        assert_eq!(engine.eval("=-(2+3)*4"), CellValue::Number(-20.0));
        assert_eq!(engine.eval("=10/2/2"), CellValue::Number(2.5));

        // RT-03: 除零必须明确返回 #DIV/0! 而非 SYNTAX
        assert_eq!(engine.eval("=1/0"), CellValue::Error("#DIV/0!".into()));
        assert_eq!(engine.eval("=10/(5-5)"), CellValue::Error("#DIV/0!".into()));

        // RT-03: 浮点极大溢出必须明确返回 #NUM! 而非 inf
        assert_eq!(engine.eval("=1e308*1e308"), CellValue::Error("#NUM!".into()));

        // RT-02: 超大范围 (=SUM(A1:XFD1048576)) 显式返回 #NUM! 错误，绝不伪装为 0
        let t0 = std::time::Instant::now();
        let huge_sum = engine.eval("=SUM(A1:XFD1048576)");
        let elapsed = t0.elapsed();
        assert_eq!(huge_sum, CellValue::Error("#NUM!".into()));
        assert!(elapsed.as_millis() < 50, "Huge range evaluation took {} ms, exceeded 50 ms budget", elapsed.as_millis());

        // 递归深度与公式长度配额测试：测试一元符号链、括号链和函数嵌套达到上限与超过上限时行为
        // 1. 括号链：31 层括号（总深度 32）成功计算，32 层及以上（总深度 >= 33）返回 #VALUE!
        let at_limit_parens = "(".repeat(31) + "1" + &")".repeat(31);
        assert_eq!(engine.eval(&at_limit_parens), CellValue::Number(1.0));
        let over_limit_parens = "(".repeat(32) + "1" + &")".repeat(32);
        assert_eq!(engine.eval(&over_limit_parens), CellValue::Error("#VALUE!".into()));

        // 2. 一元负号链：31 个负号（总深度 32）成功计算，32 个及以上返回 #VALUE!
        let at_limit_unary = "-".repeat(31) + "1";
        assert_eq!(engine.eval(&at_limit_unary), CellValue::Number(-1.0));
        let over_limit_unary = "-".repeat(32) + "1";
        assert_eq!(engine.eval(&over_limit_unary), CellValue::Error("#VALUE!".into()));

        // 3. 函数嵌套 (ABS)：31 层嵌套（总深度 32）成功计算，32 层及以上（总深度 >= 33）返回 #VALUE!
        let at_limit_func = "ABS(".repeat(31) + "1" + &")".repeat(31);
        assert_eq!(engine.eval(&at_limit_func), CellValue::Number(1.0));
        let over_limit_func = "ABS(".repeat(32) + "1" + &")".repeat(32);
        assert_eq!(engine.eval(&over_limit_func), CellValue::Error("#VALUE!".into()));

        // 4. 混合嵌套深度超限返回 #VALUE! 且不崩溃
        let mixed_deep = "-ABS(".repeat(17) + "1" + &")".repeat(17);
        assert_eq!(engine.eval(&mixed_deep), CellValue::Error("#VALUE!".into()));

        // 5. 字符超长防御
        let long_expr = "1+".repeat(1100) + "1";
        assert_eq!(engine.eval(&long_expr), CellValue::Error("#VALUE!".into()));
    }

    #[test]
    fn test_named_resolver_and_dependency_evaluation() {
        let cell_resolver = |sheet: Option<&str>, coord: CellCoord| -> CellValue {
            match (sheet, coord.col, coord.row) {
                (Some("Transactions"), 3, 3) => CellValue::Number(12.0), // D4
                (Some("Transactions"), 4, 3) => CellValue::Number(19.95), // E4
                _ => CellValue::Empty,
            }
        };

        let name_resolver = |name: &str| -> Option<CellValue> {
            match name {
                "SalesTaxRate" => Some(CellValue::Number(0.13)),
                _ => None,
            }
        };

        let engine = FormulaEngine::with_named_resolver(&cell_resolver, &name_resolver);

        // Direct named reference
        assert_eq!(engine.eval("=SalesTaxRate"), CellValue::Number(0.13));

        // Direct arithmetic with named range
        let val = engine.eval("=100 * SalesTaxRate");
        assert_eq!(val, CellValue::Number(13.0));

        // Mixed cross-sheet cell reference and named range
        let f4_eval = engine.eval("=Transactions!D4 * Transactions!E4");
        if let CellValue::Number(n) = f4_eval {
            assert!((n - 239.4).abs() < 1e-9);
        } else {
            panic!("Expected number");
        }

        let g4_eval = engine.eval("=239.4 * SalesTaxRate");
        if let CellValue::Number(n) = g4_eval {
            assert!((n - 31.122).abs() < 1e-9);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_product_and_iferror() {
        let resolver = |sheet: Option<&str>, coord: CellCoord| -> CellValue {
            match (sheet, coord.col, coord.row) {
                (None, 0, 0) => CellValue::Number(2.0), // A1
                (None, 0, 1) => CellValue::Number(3.0), // A2
                (None, 0, 2) => CellValue::Number(4.0), // A3
                (None, 1, 0) => CellValue::Number(0.0), // B1
                (None, 1, 1) => CellValue::Error("#DIV/0!".into()), // B2
                _ => CellValue::Empty,
            }
        };

        let engine = FormulaEngine::with_resolver(&resolver);

        // PRODUCT basic
        assert_eq!(engine.eval("=PRODUCT(A1:A3)"), CellValue::Number(24.0));
        assert_eq!(engine.eval("=PRODUCT(2, 3, 4)"), CellValue::Number(24.0));
        assert_eq!(engine.eval("=PRODUCT(A1, B1)"), CellValue::Number(0.0));

        // IFERROR basic
        assert_eq!(engine.eval("=IFERROR(A1 / A2, 999)"), CellValue::Number(2.0 / 3.0));
        assert_eq!(engine.eval("=IFERROR(A1 / B1, 999)"), CellValue::Number(999.0));
        assert_eq!(engine.eval("=IFERROR(B2, \"Fallback\")"), CellValue::Text("Fallback".into()));
    }

    #[test]
    fn test_extract_formula_dependencies() {
        // Single sheet cells
        let deps = extract_formula_dependencies("=A1 + B2 * $C$3", Some("Sheet1")).unwrap();
        let expected: BTreeSet<CellReference> = [
            CellReference::new(Some("Sheet1".into()), 0, 0),
            CellReference::new(Some("Sheet1".into()), 1, 1),
            CellReference::new(Some("Sheet1".into()), 2, 2),
        ].into_iter().collect();
        assert_eq!(deps, expected);

        // Rectangular range expansion
        let deps_range = extract_formula_dependencies("=SUM(A1:B2)", Some("Sheet1")).unwrap();
        assert_eq!(deps_range.len(), 4);
        assert!(deps_range.contains(&CellReference::new(Some("Sheet1".into()), 0, 0)));
        assert!(deps_range.contains(&CellReference::new(Some("Sheet1".into()), 0, 1)));
        assert!(deps_range.contains(&CellReference::new(Some("Sheet1".into()), 1, 0)));
        assert!(deps_range.contains(&CellReference::new(Some("Sheet1".into()), 1, 1)));

        // Cross sheet references with quoted sheet
        let deps_cross = extract_formula_dependencies("=Transactions!D4 * 'Lookups_2026'!B2 + SUM(Data!A1:A3)", Some("Main")).unwrap();
        assert!(deps_cross.contains(&CellReference::new(Some("Transactions".into()), 3, 3)));
        assert!(deps_cross.contains(&CellReference::new(Some("Lookups_2026".into()), 1, 1)));
        assert!(deps_cross.contains(&CellReference::new(Some("Data".into()), 0, 0)));
        assert!(deps_cross.contains(&CellReference::new(Some("Data".into()), 0, 1)));
        assert!(deps_cross.contains(&CellReference::new(Some("Data".into()), 0, 2)));

        // Literals and function names should NOT become dependencies
        let deps_func = extract_formula_dependencies("=IF(A1=\"B2\", PRODUCT(C1, D1), 0)", None).unwrap();
        assert_eq!(deps_func.len(), 3);
        assert!(deps_func.contains(&CellReference::new(None, 0, 0))); // A1
        assert!(deps_func.contains(&CellReference::new(None, 2, 0))); // C1
        assert!(deps_func.contains(&CellReference::new(None, 3, 0))); // D1
        assert!(!deps_func.contains(&CellReference::new(None, 1, 1))); // B2 is in string literal

        // Oversized range dependency quota rejection (must NOT be silently discarded)
        let oversized = extract_formula_dependencies("=SUM(A1:XFD1048576)", None);
        assert!(matches!(oversized, Err(DependencyExtractionError::RangeQuotaExceeded { range, cell_count, max_allowed: 100_000 }) if range == "A1:XFD1048576" && cell_count > 100_000));
    }

    #[test]
    fn test_dependency_graph_dag_topological_sort() {
        let mut graph = DependencyGraph::new();

        let a1 = CellReference::parse("A1").unwrap();
        let b1 = CellReference::parse("B1").unwrap();
        let c1 = CellReference::parse("C1").unwrap();
        let d1 = CellReference::parse("D1").unwrap();

        // A1 is raw input
        graph.register_cell(a1.clone(), None, None).unwrap();
        // B1 = A1 * 2
        graph.register_cell(b1.clone(), Some("=A1 * 2"), None).unwrap();
        // C1 = B1 + 10
        graph.register_cell(c1.clone(), Some("=B1 + 10"), None).unwrap();
        // D1 = PRODUCT(B1, C1)
        graph.register_cell(d1.clone(), Some("=PRODUCT(B1, C1)"), None).unwrap();

        let order = graph.topological_sort().expect("Should be a valid DAG");
        assert_eq!(order.len(), 3); // B1, C1, D1 (A1 is raw input without formula)

        let pos_b1 = order.iter().position(|c| c == &b1).unwrap();
        let pos_c1 = order.iter().position(|c| c == &c1).unwrap();
        let pos_d1 = order.iter().position(|c| c == &d1).unwrap();

        assert!(pos_b1 < pos_c1, "B1 must be evaluated before C1");
        assert!(pos_c1 < pos_d1, "C1 must be evaluated before D1");
        assert!(pos_b1 < pos_d1, "B1 must be evaluated before D1");
    }

    #[test]
    fn test_dependency_graph_cycle_detection_and_downstream_partition() {
        let mut graph = DependencyGraph::new();

        let a1 = CellReference::parse("A1").unwrap();
        let b1 = CellReference::parse("B1").unwrap();
        let c1 = CellReference::parse("C1").unwrap();
        let downstream_d1 = CellReference::parse("D1").unwrap();

        // Cycle between A1 and B1: A1 = B1, B1 = A1
        // Downstream node D1 = A1 + 10 depends on cycle, but is NOT part of the cycle
        graph.register_cell(a1.clone(), Some("=B1"), None).unwrap();
        graph.register_cell(b1.clone(), Some("=A1"), None).unwrap();
        graph.register_cell(downstream_d1.clone(), Some("=A1 + 10"), None).unwrap();

        let err = graph.topological_sort().unwrap_err();
        // Cycle nodes must strictly only include A1 and B1
        assert!(err.cycle_nodes.contains(&a1));
        assert!(err.cycle_nodes.contains(&b1));
        assert!(!err.cycle_nodes.contains(&downstream_d1), "D1 is downstream and must NOT be misdiagnosed as cycle member");
        // D1 must be diagnosed as blocked downstream
        assert!(err.blocked_downstream_nodes.contains(&downstream_d1));
        assert_eq!(err.unresolved_nodes.len(), 3);

        // 3-node cycle: A1 = B1, B1 = C1, C1 = A1
        let mut graph3 = DependencyGraph::new();
        graph3.register_cell(a1.clone(), Some("=B1"), None).unwrap();
        graph3.register_cell(b1.clone(), Some("=C1"), None).unwrap();
        graph3.register_cell(c1.clone(), Some("=A1"), None).unwrap();
        let err3 = graph3.topological_sort().unwrap_err();
        assert_eq!(err3.cycle_nodes.len(), 3);
        assert_eq!(err3.blocked_downstream_nodes.len(), 0);
    }

    #[test]
    fn test_dependency_graph_dirty_propagation_and_incremental_eval() {
        let mut graph = DependencyGraph::new();

        let a1 = CellReference::parse("A1").unwrap();
        let b1 = CellReference::parse("B1").unwrap();
        let c1 = CellReference::parse("C1").unwrap();
        let d1 = CellReference::parse("D1").unwrap();
        let x1 = CellReference::parse("X1").unwrap();
        let y1 = CellReference::parse("Y1").unwrap();

        // Chain 1: A1 -> B1 -> C1 -> D1
        graph.register_cell(a1.clone(), None, None).unwrap();
        graph.register_cell(b1.clone(), Some("=A1 * 10"), None).unwrap();
        graph.register_cell(c1.clone(), Some("=B1 + 5"), None).unwrap();
        graph.register_cell(d1.clone(), Some("=C1 * 2"), None).unwrap();

        // Chain 2: X1 -> Y1 (independent)
        graph.register_cell(x1.clone(), None, None).unwrap();
        graph.register_cell(y1.clone(), Some("=X1 + 100"), None).unwrap();

        // Clear initial dirtiness
        graph.clear_all_dirty();
        assert_eq!(graph.dirty_count(), 0);

        // Pre-mark intermediate node C1 as dirty
        graph.dirty_nodes.insert(c1.clone());

        // Modify root A1: dirty propagation MUST continue through already-dirty C1 down to D1
        let affected = graph.propagate_dirty(&[a1.clone()]);
        assert!(affected.contains(&a1));
        assert!(affected.contains(&b1));
        assert!(affected.contains(&c1));
        assert!(affected.contains(&d1), "D1 must be affected even though intermediate C1 was already dirty");
        assert!(!affected.contains(&x1));
        assert!(!affected.contains(&y1));

        // Evaluation order for dirty cells must include B1, C1, D1 in strict topological order
        let dirty_order = graph.get_evaluation_order_for_dirty().unwrap();
        assert_eq!(dirty_order.len(), 3);
        assert_eq!(dirty_order[0], b1);
        assert_eq!(dirty_order[1], c1);
        assert_eq!(dirty_order[2], d1);

        // Re-registering C1 formula must consistently cascade dirty state downstream to D1
        graph.clear_all_dirty();
        let re_affected = graph.register_cell(c1.clone(), Some("=B1 + 99"), None).unwrap();
        assert!(re_affected.contains(&c1));
        assert!(re_affected.contains(&d1), "Modifying C1 must cascade dirty state to D1");
    }
}
