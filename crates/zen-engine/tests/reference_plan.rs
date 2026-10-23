//! Literal expectations for the declared point-coordinate policy, not Excel evidence.
use zen_engine::{
    reference_plan::{Axis, AxisEditPlan, PlanError, PlannedAnchor, ReferenceAnchor, copy_anchor},
    workbook::{CellAddress, SheetId},
};
fn sheet(n: u32) -> SheetId {
    SheetId::new(n).unwrap()
}
fn anchor(s: u32, row: u32, col: u16, rf: bool, cf: bool) -> ReferenceAnchor {
    ReferenceAnchor {
        sheet: sheet(s),
        cell: CellAddress::new(row, col).unwrap(),
        row_fixed: rf,
        column_fixed: cf,
    }
}
#[test]
fn insertion_moves_fixed_and_relative_points_on_referenced_identity_only() {
    for rf in [false, true] {
        for cf in [false, true] {
            let original = anchor(7, 4, 2, rf, cf);
            let row = AxisEditPlan::insert(sheet(7), Axis::Row, 4, 2).unwrap();
            assert_eq!(
                row.map_anchor(original).unwrap(),
                PlannedAnchor::Kept(anchor(7, 6, 2, rf, cf))
            );
            let col = AxisEditPlan::insert(sheet(7), Axis::Column, 1, 3).unwrap();
            assert_eq!(
                col.map_anchor(original).unwrap(),
                PlannedAnchor::Kept(anchor(7, 4, 5, rf, cf))
            );
            assert_eq!(
                row.map_anchor(anchor(9, 4, 2, rf, cf)).unwrap(),
                PlannedAnchor::Kept(anchor(9, 4, 2, rf, cf))
            );
        }
    }
}
#[test]
fn deletion_marks_inside_and_shifts_below_without_moving_before_interval() {
    let p = AxisEditPlan::delete(sheet(7), Axis::Row, 4, 2).unwrap();
    assert_eq!(
        p.map_anchor(anchor(7, 3, 2, true, true)).unwrap(),
        PlannedAnchor::Kept(anchor(7, 3, 2, true, true))
    );
    for r in [4, 5] {
        assert_eq!(
            p.map_anchor(anchor(7, r, 2, true, true)).unwrap(),
            PlannedAnchor::Deleted
        );
    }
    assert_eq!(
        p.map_anchor(anchor(7, 6, 2, true, true)).unwrap(),
        PlannedAnchor::Kept(anchor(7, 4, 2, true, true))
    );
    let p = AxisEditPlan::delete(sheet(7), Axis::Column, 1, 2).unwrap();
    assert_eq!(
        p.map_anchor(anchor(7, 8, 2, false, false)).unwrap(),
        PlannedAnchor::Deleted
    );
    assert_eq!(
        p.map_anchor(anchor(7, 8, 3, false, false)).unwrap(),
        PlannedAnchor::Kept(anchor(7, 8, 1, false, false))
    );
}
#[test]
fn copy_offset_uses_fixed_flags_independently_with_literal_expected_coordinates() {
    for (rf, cf, row, col) in [
        (false, false, 7, 1),
        (false, true, 7, 3),
        (true, false, 4, 1),
        (true, true, 4, 3),
    ] {
        let original = anchor(7, 4, 3, rf, cf);
        assert_eq!(
            copy_anchor(original, 3, -2).unwrap(),
            anchor(7, row, col, rf, cf)
        );
        assert_eq!(original, anchor(7, 4, 3, rf, cf));
    }
}
#[test]
fn invalid_intervals_refuse_without_clamping_or_wrapping() {
    for axis in [Axis::Row, Axis::Column] {
        let limit = if axis == Axis::Row { 1_048_576 } else { 16_384 };
        for (at, n) in [
            (0, 0),
            (limit, 1),
            (limit - 1, 2),
            (u32::MAX, 1),
            (1, u32::MAX),
        ] {
            assert_eq!(
                AxisEditPlan::insert(sheet(7), axis, at, n),
                Err(PlanError::InvalidInterval)
            );
            assert_eq!(
                AxisEditPlan::delete(sheet(7), axis, at, n),
                Err(PlanError::InvalidInterval)
            );
        }
    }
}
#[test]
fn coordinate_overflow_and_extreme_copy_offsets_refuse() {
    assert_eq!(
        AxisEditPlan::insert(sheet(7), Axis::Row, 0, 1)
            .unwrap()
            .map_anchor(anchor(7, 1_048_575, 0, false, false)),
        Err(PlanError::CoordinateOverflow)
    );
    assert_eq!(
        AxisEditPlan::insert(sheet(7), Axis::Column, 0, 1)
            .unwrap()
            .map_anchor(anchor(7, 0, 16_383, false, false)),
        Err(PlanError::CoordinateOverflow)
    );
    for (r, c) in [
        (-1, 0),
        (0, -1),
        (i64::MAX, 0),
        (0, i64::MAX),
        (i64::MIN, 0),
    ] {
        assert_eq!(
            copy_anchor(anchor(7, 0, 0, false, false), r, c),
            Err(PlanError::CoordinateOverflow)
        );
    }
    assert_eq!(
        copy_anchor(anchor(7, 1, 1, true, true), i64::MAX, i64::MIN).unwrap(),
        anchor(7, 1, 1, true, true)
    );
}
#[test]
fn bounded_batch_failure_returns_no_partial_plan_and_input_is_unchanged() {
    let input = [
        anchor(7, 3, 2, false, false),
        anchor(7, 1_048_575, 2, true, true),
    ];
    let before = input;
    let p = AxisEditPlan::insert(sheet(7), Axis::Row, 0, 1).unwrap();
    assert_eq!(p.map_anchors(&input, 1), Err(PlanError::AnchorBudget));
    assert_eq!(input, before);
    assert_eq!(p.map_anchors(&input, 2), Err(PlanError::CoordinateOverflow));
    assert_eq!(input, before);
    assert_eq!(p.map_anchors(&[], 0).unwrap(), Vec::<PlannedAnchor>::new());
}
#[test]
fn full_axis_deletion_can_describe_deleted_points_but_is_not_a_workbook_edit() {
    let p = AxisEditPlan::delete(sheet(7), Axis::Row, 0, 1_048_576).unwrap();
    assert_eq!(
        p.map_anchor(anchor(7, 0, 0, false, false)).unwrap(),
        PlannedAnchor::Deleted
    );
    assert_eq!(
        p.map_anchor(anchor(7, 1_048_575, 0, true, true)).unwrap(),
        PlannedAnchor::Deleted
    );
    assert_eq!(
        p.map_anchor(anchor(9, 0, 0, false, false)).unwrap(),
        PlannedAnchor::Kept(anchor(9, 0, 0, false, false))
    );
}
