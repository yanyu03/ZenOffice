use zen_engine::{
    reference_plan::{
        Axis, AxisEditPlan, PlannedAnchor, ReferenceAnchor,
        range::{PlannedRange, RangeError, ReferenceRange, map_range},
    },
    workbook::{CellAddress, SheetId},
};
fn anchor(axis: Axis, v: u32, end: bool) -> ReferenceAnchor {
    ReferenceAnchor {
        sheet: SheetId::new(7).unwrap(),
        cell: match axis {
            Axis::Row => CellAddress::new(v, 2),
            Axis::Column => CellAddress::new(2, v as u16),
        }
        .unwrap(),
        column_fixed: !end,
        row_fixed: end,
    }
}
fn rect(axis: Axis, lo: u32, hi: u32) -> ReferenceRange {
    ReferenceRange::new(anchor(axis, lo, false), anchor(axis, hi, true)).unwrap()
}
fn bounds(axis: Axis, p: PlannedRange) -> (u32, u32) {
    let PlannedRange::Kept(r) = p else {
        panic!("expected surviving rectangle")
    };
    let (a, b) = r.corners();
    match axis {
        Axis::Row => (a.cell.row(), b.cell.row()),
        Axis::Column => (a.cell.col() as u32, b.cell.col() as u32),
    }
}
#[test]
fn normalized_same_sheet_construction_only() {
    assert!(matches!(
        ReferenceRange::new(anchor(Axis::Row, 20, false), anchor(Axis::Row, 10, true)),
        Err(RangeError::InvalidRectangle)
    ));
    let a = anchor(Axis::Row, 10, false);
    let mut b = anchor(Axis::Row, 20, true);
    b.sheet = SheetId::new(9).unwrap();
    assert!(ReferenceRange::new(a, b).is_err());
    assert!(
        ReferenceRange::new(
            anchor(Axis::Column, 20, false),
            anchor(Axis::Column, 10, true)
        )
        .is_err()
    );
}
#[test]
fn insertion_literal_boundary_matrix_on_both_axes() {
    for axis in [Axis::Row, Axis::Column] {
        for (at, want) in [
            (5, (12, 22)),
            (10, (12, 22)),
            (15, (10, 22)),
            (20, (10, 22)),
            (21, (10, 20)),
        ] {
            let plan = AxisEditPlan::insert(SheetId::new(7).unwrap(), axis, at, 2).unwrap();
            assert_eq!(
                bounds(axis, map_range(plan, rect(axis, 10, 20)).unwrap()),
                want
            );
        }
    }
}
#[test]
fn deletion_literal_clip_matrix_on_both_axes() {
    for axis in [Axis::Row, Axis::Column] {
        for (at, want) in [
            (5, (7, 17)),
            (8, (8, 17)),
            (10, (10, 17)),
            (15, (10, 17)),
            (19, (10, 18)),
            (20, (10, 19)),
            (21, (10, 20)),
        ] {
            let plan = AxisEditPlan::delete(SheetId::new(7).unwrap(), axis, at, 3).unwrap();
            assert_eq!(
                bounds(axis, map_range(plan, rect(axis, 10, 20)).unwrap()),
                want
            );
        }
    }
}
#[test]
fn exact_containing_and_full_axis_deletion_are_deleted_not_serialized() {
    for axis in [Axis::Row, Axis::Column] {
        for (at, count) in [
            (10, 11),
            (8, 20),
            (0, if axis == Axis::Row { 1_048_576 } else { 16_384 }),
        ] {
            assert_eq!(
                map_range(
                    AxisEditPlan::delete(SheetId::new(7).unwrap(), axis, at, count).unwrap(),
                    rect(axis, 10, 20)
                ),
                Ok(PlannedRange::Deleted)
            );
        }
    }
}
#[test]
fn other_sheet_and_mixed_flags_are_preserved() {
    let r = rect(Axis::Row, 10, 20);
    let other = AxisEditPlan::delete(SheetId::new(9).unwrap(), Axis::Row, 0, 30).unwrap();
    assert_eq!(map_range(other, r), Ok(PlannedRange::Kept(r)));
    let plan = AxisEditPlan::delete(SheetId::new(7).unwrap(), Axis::Row, 8, 3).unwrap();
    let PlannedRange::Kept(mapped) = map_range(plan, r).unwrap() else {
        panic!()
    };
    let (a, b) = mapped.corners();
    assert_eq!(
        (a.column_fixed, a.row_fixed, b.column_fixed, b.row_fixed),
        (true, false, false, true)
    );
    assert_eq!((a.cell.col(), b.cell.col()), (2, 2));
}
#[test]
fn insertion_overflow_refuses_without_changing_input() {
    for (axis, last) in [(Axis::Row, 1_048_575), (Axis::Column, 16_383)] {
        let r = rect(axis, last - 1, last);
        let original = r.corners();
        let plan = AxisEditPlan::insert(SheetId::new(7).unwrap(), axis, last, 1).unwrap();
        assert!(map_range(plan, r).is_err());
        assert_eq!(r.corners(), original);
    }
}
#[test]
fn singleton_outcomes_match_independent_point_mapping() {
    for axis in [Axis::Row, Axis::Column] {
        for at in [5, 10, 15] {
            for delete in [false, true] {
                let p = if delete {
                    AxisEditPlan::delete(SheetId::new(7).unwrap(), axis, at, 3)
                } else {
                    AxisEditPlan::insert(SheetId::new(7).unwrap(), axis, at, 3)
                }
                .unwrap();
                let a = anchor(axis, 10, false);
                let r = ReferenceRange::new(a, a).unwrap();
                match (p.map_anchor(a).unwrap(), map_range(p, r).unwrap()) {
                    (PlannedAnchor::Deleted, PlannedRange::Deleted) => (),
                    (PlannedAnchor::Kept(a), PlannedRange::Kept(r)) => {
                        assert_eq!(r.corners(), (a, a))
                    }
                    other => panic!("singleton disagreement: {other:?}"),
                }
            }
        }
    }
}
