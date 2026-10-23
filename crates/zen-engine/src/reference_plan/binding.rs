//! Source- and workbook-borrowed point bindings, not structural-edit admission.
use super::{
    ReferenceAnchor,
    syntax::{PointSpan, ScanError, scan_points},
};
use crate::workbook::{SheetId, SheetKind, Workbook};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BindError {
    #[error("sheet catalog exceeds caller budget or fixed 1024-sheet cap")]
    SheetBudget,
    #[error("sheet name exceeds the binding slice's 128-byte budget")]
    NameBudget,
    #[error("formula owner is not a worksheet in this workbook")]
    Owner,
    #[error("unknown or non-worksheet reference at byte {0}")]
    Target(usize),
    #[error("defined names are not admitted by this binding slice")]
    DefinedNames,
    #[error(transparent)]
    Scan(#[from] ScanError),
    #[error("cannot allocate bounded point bindings")]
    Allocation,
}
#[derive(Debug)]
pub struct BoundPoint {
    span: PointSpan,
    anchor: ReferenceAnchor,
}
impl BoundPoint {
    pub fn span(&self) -> &PointSpan {
        &self.span
    }
    pub fn anchor(&self) -> ReferenceAnchor {
        self.anchor
    }
    pub fn explicit_sheet(&self) -> bool {
        self.span.sheet.is_some()
    }
}
/// Immutable observations tied to these exact borrowed source/model values.
/// No public constructor, mutable access, serialization, revision or write grant.
/// Successful binding still requires formula grammar/function validation and
/// range/owner policies before any future structural operation can be admitted.
#[derive(Debug)]
pub struct BoundFormula<'f, 'w> {
    source: &'f str,
    workbook: &'w Workbook,
    owner: SheetId,
    points: Vec<BoundPoint>,
}
impl<'f, 'w> BoundFormula<'f, 'w> {
    pub fn source(&self) -> &'f str {
        self.source
    }
    pub fn workbook(&self) -> &'w Workbook {
        self.workbook
    }
    pub fn owner(&self) -> SheetId {
        self.owner
    }
    pub fn points(&self) -> &[BoundPoint] {
        &self.points
    }
    pub fn reference_text(&self, index: usize) -> Option<&'f str> {
        self.points
            .get(index)
            .map(|p| &self.source[p.span.source.clone()])
    }
    pub fn address_text(&self, index: usize) -> Option<&'f str> {
        self.points
            .get(index)
            .map(|p| &self.source[p.span.address.clone()])
    }
}
/// Workbook::new already guarantees unique IDs and lowercase sheet names.
/// Lookup deliberately follows its String::to_lowercase convention; that is
/// the existing model policy, not proof of complete Excel Unicode equivalence.
/// Fixed caps bound scanning cost; caller may impose smaller limits. Budgets
/// count names/references rather than all heap bytes. No I/O or workbook edit.
pub fn bind_points<'f, 'w>(
    source: &'f str,
    workbook: &'w Workbook,
    owner: SheetId,
    max_refs: usize,
    max_sheets: usize,
) -> Result<BoundFormula<'f, 'w>, BindError> {
    let sheets = workbook.sheets();
    if sheets.len() > max_sheets.min(1024) {
        return Err(BindError::SheetBudget);
    }
    if sheets.iter().any(|s| s.name().len() > 128) {
        return Err(BindError::NameBudget);
    }
    if !workbook.defined_names().is_empty() {
        return Err(BindError::DefinedNames);
    }
    if !workbook
        .sheet(owner)
        .is_some_and(|s| matches!(s.kind(), SheetKind::Worksheet))
    {
        return Err(BindError::Owner);
    }
    let spans = scan_points(source, max_refs)?;
    let mut points = Vec::new();
    points
        .try_reserve_exact(spans.len())
        .map_err(|_| BindError::Allocation)?;
    for span in spans {
        let sheet = if let Some(name) = &span.sheet {
            let key = name.to_lowercase();
            sheets
                .iter()
                .find(|s| {
                    s.name().to_lowercase() == key && matches!(s.kind(), SheetKind::Worksheet)
                })
                .map(|s| s.id())
                .ok_or(BindError::Target(span.source.start))?
        } else {
            owner
        };
        let anchor = ReferenceAnchor {
            sheet,
            cell: span.cell,
            column_fixed: span.column_fixed,
            row_fixed: span.row_fixed,
        };
        points.push(BoundPoint { span, anchor });
    }
    Ok(BoundFormula {
        source,
        workbook,
        owner,
        points,
    })
}
