//! Pure normalized rectangle policy, not formula serialization or edit admission.
use super::{Axis, AxisEditPlan, Change, PlanError, ReferenceAnchor};
use crate::workbook::CellAddress;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RangeError {
    #[error("range requires same-sheet normalized corners")]
    InvalidRectangle,
    #[error(transparent)]
    Plan(#[from] PlanError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceRange {
    start: ReferenceAnchor,
    end: ReferenceAnchor,
}
impl ReferenceRange {
    pub fn new(start: ReferenceAnchor, end: ReferenceAnchor) -> Result<Self, RangeError> {
        if start.sheet != end.sheet
            || start.cell.row() > end.cell.row()
            || start.cell.col() > end.cell.col()
        {
            return Err(RangeError::InvalidRectangle);
        }
        Ok(Self { start, end })
    }
    pub fn corners(self) -> (ReferenceAnchor, ReferenceAnchor) {
        (self.start, self.end)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedRange {
    Kept(ReferenceRange),
    /// Not authorization to serialize #REF! or remove a formula.
    Deleted,
}
fn moved(anchor: ReferenceAnchor, axis: Axis, value: u32) -> Result<ReferenceAnchor, PlanError> {
    let (row, col) = match axis {
        Axis::Row => (value, anchor.cell.col()),
        Axis::Column => (
            anchor.cell.row(),
            u16::try_from(value).map_err(|_| PlanError::CoordinateOverflow)?,
        ),
    };
    Ok(ReferenceAnchor {
        cell: CellAddress::new(row, col).map_err(|_| PlanError::CoordinateOverflow)?,
        ..anchor
    })
}
/// Insert at/before start moves the whole range; strictly inside through the
/// last coordinate expands the end; after end does nothing. Delete clips away
/// intersecting coordinates and shifts surviving suffix coordinates; no
/// surviving coordinate returns Deleted. Fixed flags follow surviving corners
/// unchanged: they do not prevent structural displacement. These are declared
/// internal policies, not native Excel behavior evidence. No package I/O,
/// text parsing, frame authority, recursive scan or allocation occurs here.
pub fn map_range(plan: AxisEditPlan, range: ReferenceRange) -> Result<PlannedRange, RangeError> {
    if range.start.sheet != plan.sheet {
        return Ok(PlannedRange::Kept(range));
    }
    let lo = plan.axis.coordinate(range.start.cell);
    let hi = plan.axis.coordinate(range.end.cell);
    let (lo, hi) = match plan.change {
        Change::Insert => {
            let next_hi = if plan.at <= hi {
                hi.checked_add(plan.count)
                    .ok_or(PlanError::CoordinateOverflow)?
            } else {
                hi
            };
            let next_lo = if plan.at <= lo {
                lo.checked_add(plan.count)
                    .ok_or(PlanError::CoordinateOverflow)?
            } else {
                lo
            };
            (next_lo, next_hi)
        }
        Change::Delete => {
            let end = plan.at + plan.count; // Validated non-overflowing interval.
            if hi < plan.at {
                (lo, hi)
            } else if lo >= end {
                (lo - plan.count, hi - plan.count)
            } else if lo >= plan.at && hi < end {
                return Ok(PlannedRange::Deleted);
            } else {
                let next_lo = if lo < plan.at { lo } else { plan.at };
                let next_hi = if hi >= end {
                    hi - plan.count
                } else {
                    plan.at
                        .checked_sub(1)
                        .ok_or(PlanError::CoordinateOverflow)?
                };
                (next_lo, next_hi)
            }
        }
    };
    if hi >= plan.axis.limit() {
        return Err(PlanError::CoordinateOverflow.into());
    }
    Ok(PlannedRange::Kept(ReferenceRange::new(
        moved(range.start, plan.axis, lo)?,
        moved(range.end, plan.axis, hi)?,
    )?))
}
