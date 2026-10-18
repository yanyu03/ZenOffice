//! Typed workbook read model, independent of XML and the legacy single-sheet UI.
use std::{
    collections::{BTreeMap, HashSet},
    num::NonZeroU32,
};

#[derive(Debug, thiserror::Error)]
#[error("Invalid workbook model: {0}")]
pub struct ModelError(pub String);

/// Document-local identity; sheet order and names can change independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SheetId(NonZeroU32);
impl SheetId {
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellAddress {
    row: u32,
    col: u16,
}
impl CellAddress {
    pub fn new(row: u32, col: u16) -> Result<Self, ModelError> {
        if row >= 1_048_576 || col >= 16_384 {
            return Err(ModelError("cell exceeds XLSX bounds".into()));
        }
        Ok(Self { row, col })
    }
    pub fn row(self) -> u32 {
        self.row
    }
    pub fn col(self) -> u16 {
        self.col
    }
    pub fn to_a1(self) -> String {
        crate::calc::CellCoord::new(self.col as usize, self.row as usize).to_a1()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateSystem {
    Excel1900,
    Excel1904,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetVisibility {
    Visible,
    Hidden,
    VeryHidden,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetKind {
    Worksheet,
    Chartsheet,
    Opaque(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType {
    Number,
    SharedString,
    InlineString,
    FormulaString,
    Boolean,
    Error,
}

/// Numbers keep their lexical representation for precision/fidelity decisions.
/// Numeric date serials stay numeric until styles/date policy is implemented.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    Number { value: f64, lexical: String },
    Text(String),
    Boolean(bool),
    Error(String),
}
impl TypedValue {
    pub fn text(&self) -> &str {
        match self {
            Self::Number { lexical, .. } => lexical,
            Self::Text(text) | Self::Error(text) => text,
            Self::Boolean(true) => "TRUE",
            Self::Boolean(false) => "FALSE",
        }
    }
    pub fn display(&self) -> String {
        match self {
            Self::Number { lexical, .. } => lexical.clone(),
            Self::Text(text) | Self::Error(text) => text.clone(),
            Self::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaKind {
    Normal,
    Shared,
    Array,
    DataTable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellFormula {
    /// Source text without a synthetic '='; shared followers may be empty.
    pub text: String,
    pub kind: FormulaKind,
    pub shared_index: Option<u32>,
    pub range: Option<String>,
    pub attributes: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TypedCell {
    pub source_type: CellType,
    /// None means no stored value/cache, distinct from an empty text value.
    pub value: Option<TypedValue>,
    pub formula: Option<CellFormula>,
    pub style_index: Option<u32>,
}
impl TypedCell {
    pub fn type_label(&self) -> &'static str {
        match self.source_type {
            CellType::Number => "数字",
            CellType::SharedString => "共享文本",
            CellType::InlineString => "内联文本",
            CellType::FormulaString => "文本结果",
            CellType::Boolean => "布尔",
            CellType::Error => "错误",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Worksheet {
    id: SheetId,
    name: String,
    visibility: SheetVisibility,
    kind: SheetKind,
    cells: BTreeMap<CellAddress, TypedCell>,
}
impl Worksheet {
    pub fn new(
        id: SheetId,
        name: String,
        visibility: SheetVisibility,
        kind: SheetKind,
        cells: BTreeMap<CellAddress, TypedCell>,
    ) -> Result<Self, ModelError> {
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(ModelError("empty/control-character sheet name".into()));
        }
        if cells.values().any(
            |c| matches!(&c.value, Some(TypedValue::Number { value, .. }) if !value.is_finite()),
        ) {
            return Err(ModelError("non-finite numeric cell".into()));
        }
        for cell in cells.values() {
            if let Some(value) = &cell.value {
                let compatible = matches!(
                    (cell.source_type, value),
                    (CellType::Number, TypedValue::Number { .. })
                        | (
                            CellType::SharedString
                                | CellType::InlineString
                                | CellType::FormulaString,
                            TypedValue::Text(_)
                        )
                        | (CellType::Boolean, TypedValue::Boolean(_))
                        | (CellType::Error, TypedValue::Error(_))
                );
                if !compatible {
                    return Err(ModelError("cell value/type mismatch".into()));
                }
            }
        }
        Ok(Self {
            id,
            name,
            visibility,
            kind,
            cells,
        })
    }
    pub fn id(&self) -> SheetId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn visibility(&self) -> SheetVisibility {
        self.visibility
    }
    pub fn kind(&self) -> &SheetKind {
        &self.kind
    }
    pub fn cells(&self) -> &BTreeMap<CellAddress, TypedCell> {
        &self.cells
    }
    pub fn cell(&self, address: CellAddress) -> Option<&TypedCell> {
        self.cells.get(&address)
    }
}

#[derive(Debug, Clone)]
pub struct DefinedName {
    pub name: String,
    /// Document-local sheet identity, not the sheet's position or display name.
    pub local_sheet: Option<SheetId>,
    pub formula: String,
}

#[derive(Debug, Clone)]
pub struct Workbook {
    sheets: Vec<Worksheet>,
    date_system: DateSystem,
    defined_names: Vec<DefinedName>,
}
impl Workbook {
    pub fn new(sheets: Vec<Worksheet>, date_system: DateSystem) -> Result<Self, ModelError> {
        if sheets.is_empty() {
            return Err(ModelError("workbook has no sheets".into()));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for sheet in &sheets {
            if !ids.insert(sheet.id) || !names.insert(sheet.name.to_lowercase()) {
                return Err(ModelError("duplicate sheet id/name".into()));
            }
        }
        Ok(Self {
            sheets,
            date_system,
            defined_names: Vec::new(),
        })
    }
    pub fn with_defined_names(mut self, names: Vec<DefinedName>) -> Result<Self, ModelError> {
        let mut seen = HashSet::new();
        for name in &names {
            if name.name.is_empty()
                || name.local_sheet.is_some_and(|id| self.sheet(id).is_none())
                || !seen.insert((name.local_sheet, name.name.to_lowercase()))
            {
                return Err(ModelError("invalid or duplicate scoped defined name".into()));
            }
        }
        self.defined_names = names;
        Ok(self)
    }
    pub fn defined_names(&self) -> &[DefinedName] {
        &self.defined_names
    }
    pub fn sheets(&self) -> &[Worksheet] {
        &self.sheets
    }
    pub fn sheet(&self, id: SheetId) -> Option<&Worksheet> {
        self.sheets.iter().find(|s| s.id == id)
    }
    pub fn date_system(&self) -> DateSystem {
        self.date_system
    }
    pub fn cell_count(&self) -> usize {
        self.sheets.iter().map(|s| s.cells.len()).sum()
    }
}
