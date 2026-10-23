//! Pure coordinate plans for decoded point references, not formula rewriting.
//! Callers must resolve sheet identity and retain syntax before using this.
//! This module never opens, mutates, calculates or serializes a workbook.
use crate::workbook::{CellAddress, SheetId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceAnchor {
    pub sheet: SheetId,
    pub cell: CellAddress,
    pub column_fixed: bool,
    pub row_fixed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}
impl Axis {
    fn limit(self) -> u32 {
        match self {
            Self::Row => 1_048_576,
            Self::Column => 16_384,
        }
    }
    fn coordinate(self, cell: CellAddress) -> u32 {
        match self {
            Self::Row => cell.row(),
            Self::Column => u32::from(cell.col()),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Insert,
    Delete,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlanError {
    #[error("axis edit requires a nonempty interval inside XLSX bounds")]
    InvalidInterval,
    #[error("reference coordinate would leave XLSX bounds")]
    CoordinateOverflow,
    #[error("anchor count exceeds the caller's explicit planning budget")]
    AnchorBudget,
    #[error("cannot allocate bounded reference plan")]
    Allocation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedAnchor {
    Kept(ReferenceAnchor),
    /// A point was deleted. Not authorization to write #REF! into a formula.
    Deleted,
}
/// One validated axis edit, applied to explicitly decoded point anchors.
/// Ranges, lexical formulas and package dependencies need their own admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisEditPlan {
    sheet: SheetId,
    axis: Axis,
    at: u32,
    count: u32,
    change: Change,
}
impl AxisEditPlan {
    pub fn insert(sheet: SheetId, axis: Axis, at: u32, count: u32) -> Result<Self, PlanError> {
        Self::new(sheet, axis, at, count, Change::Insert)
    }
    pub fn delete(sheet: SheetId, axis: Axis, at: u32, count: u32) -> Result<Self, PlanError> {
        Self::new(sheet, axis, at, count, Change::Delete)
    }
    fn new(
        sheet: SheetId,
        axis: Axis,
        at: u32,
        count: u32,
        change: Change,
    ) -> Result<Self, PlanError> {
        if at >= axis.limit() || count == 0 || count > axis.limit() - at {
            return Err(PlanError::InvalidInterval);
        }
        Ok(Self {
            sheet,
            axis,
            at,
            count,
            change,
        })
    }
    pub fn map_anchor(self, anchor: ReferenceAnchor) -> Result<PlannedAnchor, PlanError> {
        if anchor.sheet != self.sheet {
            return Ok(PlannedAnchor::Kept(anchor));
        }
        let value = self.axis.coordinate(anchor.cell);
        let next = match self.change {
            Change::Insert if value >= self.at => value
                .checked_add(self.count)
                .filter(|v| *v < self.axis.limit())
                .ok_or(PlanError::CoordinateOverflow)?,
            Change::Delete if (self.at..self.at + self.count).contains(&value) => {
                return Ok(PlannedAnchor::Deleted);
            }
            Change::Delete if value >= self.at + self.count => value - self.count,
            _ => value,
        };
        let (row, col) = match self.axis {
            Axis::Row => (next, anchor.cell.col()),
            Axis::Column => (
                anchor.cell.row(),
                u16::try_from(next).map_err(|_| PlanError::CoordinateOverflow)?,
            ),
        };
        Ok(PlannedAnchor::Kept(ReferenceAnchor {
            cell: CellAddress::new(row, col).map_err(|_| PlanError::CoordinateOverflow)?,
            ..anchor
        }))
    }
    /// No input mutation or partial returned vector if any anchor fails.
    /// This budget counts anchors, not total workbook or heap bytes.
    pub fn map_anchors(
        self,
        anchors: &[ReferenceAnchor],
        max_anchors: usize,
    ) -> Result<Vec<PlannedAnchor>, PlanError> {
        if anchors.len() > max_anchors {
            return Err(PlanError::AnchorBudget);
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(anchors.len())
            .map_err(|_| PlanError::Allocation)?;
        for anchor in anchors {
            result.push(self.map_anchor(*anchor)?);
        }
        Ok(result)
    }
}
/// Coordinate displacement for a copied decoded anchor. Structural edits above
/// move fixed coordinates too; copying moves only relative dimensions. This
/// preserves explicit sheet identity, not an implicit formula-owner binding.
pub fn copy_anchor(
    anchor: ReferenceAnchor,
    rows: i64,
    columns: i64,
) -> Result<ReferenceAnchor, PlanError> {
    let row = i64::from(anchor.cell.row())
        .checked_add(if anchor.row_fixed { 0 } else { rows })
        .filter(|r| (0..i64::from(Axis::Row.limit())).contains(r))
        .ok_or(PlanError::CoordinateOverflow)?;
    let col = i64::from(anchor.cell.col())
        .checked_add(if anchor.column_fixed { 0 } else { columns })
        .filter(|c| (0..i64::from(Axis::Column.limit())).contains(c))
        .ok_or(PlanError::CoordinateOverflow)?;
    Ok(ReferenceAnchor {
        cell: CellAddress::new(row as u32, col as u16)
            .map_err(|_| PlanError::CoordinateOverflow)?,
        ..anchor
    })
}

/// Lexical observations only; not formula or edit admission.
pub mod syntax;

/// Source/model-bound point observations; not structural-edit admission.
pub mod binding;

/// Bounded arithmetic sequencing only, not evaluation or edit permission.
pub mod arithmetic;

/// Pure normalized rectangle interval policies, not formula rewriting.
pub mod range;
