use std::collections::BTreeMap;
use zen_engine::{
    reference_plan::{
        Axis, AxisEditPlan, PlannedAnchor,
        binding::{BindError, bind_points},
    },
    workbook::{DateSystem, DefinedName, SheetId, SheetKind, SheetVisibility, Workbook, Worksheet},
};
fn id(n: u32) -> SheetId {
    SheetId::new(n).unwrap()
}
fn sheet(n: u32, name: &str, kind: SheetKind) -> Worksheet {
    Worksheet::new(
        id(n),
        name.into(),
        SheetVisibility::Visible,
        kind,
        BTreeMap::new(),
    )
    .unwrap()
}
fn book() -> Workbook {
    Workbook::new(
        vec![
            sheet(70, "O'Brien 表", SheetKind::Worksheet),
            sheet(9, "Owner", SheetKind::Worksheet),
            sheet(42, "DATA", SheetKind::Worksheet),
        ],
        DateSystem::Excel1900,
    )
    .unwrap()
}
#[test]
fn binds_explicit_and_implicit_references_to_ids_with_original_text() {
    let w = book();
    let f = "= $a1 + data!B$2 + 'O''Brien 表'!$C$3";
    let b = bind_points(f, &w, id(9), 3, 3).unwrap();
    assert!(std::ptr::eq(b.workbook(), &w));
    assert_eq!(b.source(), f);
    assert_eq!(b.owner(), id(9));
    assert_eq!(
        b.points()
            .iter()
            .map(|p| p.anchor().sheet)
            .collect::<Vec<_>>(),
        [id(9), id(42), id(70)]
    );
    assert_eq!(
        b.points()
            .iter()
            .map(|p| p.explicit_sheet())
            .collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(b.reference_text(2), Some("'O''Brien 表'!$C$3"));
    assert_eq!(b.address_text(2), Some("$C$3"));
    assert_eq!(b.reference_text(3), None);
    assert_eq!(b.address_text(3), None);
    assert!(b.points()[0].anchor().column_fixed);
    assert!(b.points()[1].anchor().row_fixed);
}
#[test]
fn order_is_not_identity_and_implicit_owner_is_explicit_input() {
    let w = book();
    let f = "=A1+DATA!B2";
    let a = bind_points(f, &w, id(9), 2, 3).unwrap();
    let b = bind_points(f, &w, id(70), 2, 3).unwrap();
    assert_eq!(a.points()[0].anchor().sheet, id(9));
    assert_eq!(b.points()[0].anchor().sheet, id(70));
    assert_eq!(a.points()[1].anchor(), b.points()[1].anchor());
    let w2 = Workbook::new(
        vec![
            sheet(42, "data", SheetKind::Worksheet),
            sheet(70, "O'Brien 表", SheetKind::Worksheet),
            sheet(9, "Owner", SheetKind::Worksheet),
        ],
        DateSystem::Excel1900,
    )
    .unwrap();
    let c = bind_points(f, &w2, id(9), 2, 3).unwrap();
    assert_eq!(a.points()[0].anchor(), c.points()[0].anchor());
    assert_eq!(a.points()[1].anchor(), c.points()[1].anchor());
}
#[test]
fn unknown_owner_target_and_non_worksheet_refuse() {
    let w = Workbook::new(
        vec![
            sheet(9, "Owner", SheetKind::Worksheet),
            sheet(42, "Chart", SheetKind::Chartsheet),
        ],
        DateSystem::Excel1900,
    )
    .unwrap();
    assert!(matches!(
        bind_points("=A1", &w, id(70), 1, 2),
        Err(BindError::Owner)
    ));
    assert!(matches!(
        bind_points("=A1", &w, id(42), 1, 2),
        Err(BindError::Owner)
    ));
    for f in ["=A1+Missing!B2", "=Chart!B2"] {
        assert!(matches!(
            bind_points(f, &w, id(9), 2, 2),
            Err(BindError::Target(_))
        ));
    }
}
#[test]
fn refuses_names_and_lexical_failures_without_publishing_partial_bindings() {
    let w = book()
        .with_defined_names(vec![DefinedName {
            name: "Rate".into(),
            local_sheet: None,
            formula: "1".into(),
        }])
        .unwrap();
    assert!(matches!(
        bind_points("=A1", &w, id(9), 1, 3),
        Err(BindError::DefinedNames)
    ));
    let w = book();
    for f in ["=A1+B2:C3", "=A1+Name", "=[book]DATA!A1", "=A1+{B2}"] {
        assert!(matches!(
            bind_points(f, &w, id(9), 8, 3),
            Err(BindError::Scan(_))
        ));
    }
    assert_eq!(w.sheets()[0].name(), "O'Brien 表");
}
#[test]
fn rejects_catalog_and_reference_budgets() {
    let w = book();
    assert!(matches!(
        bind_points("=A1", &w, id(9), 1, 2),
        Err(BindError::SheetBudget)
    ));
    assert!(matches!(
        bind_points("=A1+B2", &w, id(9), 1, 3),
        Err(BindError::Scan(_))
    ));
    let w = Workbook::new(
        vec![sheet(9, &"a".repeat(129), SheetKind::Worksheet)],
        DateSystem::Excel1900,
    )
    .unwrap();
    assert!(matches!(
        bind_points("=1", &w, id(9), 0, 1),
        Err(BindError::NameBudget)
    ));
    let many = (1..=1025)
        .map(|n| sheet(n, &format!("S{n}"), SheetKind::Worksheet))
        .collect();
    let w = Workbook::new(many, DateSystem::Excel1900).unwrap();
    assert!(matches!(
        bind_points("=1", &w, id(1), 0, usize::MAX),
        Err(BindError::SheetBudget)
    ));
}
#[test]
fn bound_anchors_feed_point_math_without_rewriting_source_or_model() {
    let w = book();
    let f = "=$A$1+DATA!$B$2";
    let b = bind_points(f, &w, id(9), 2, 3).unwrap();
    let p = AxisEditPlan::insert(id(42), Axis::Row, 0, 2).unwrap();
    let PlannedAnchor::Kept(moved) = p.map_anchor(b.points()[1].anchor()).unwrap() else {
        panic!()
    };
    assert_eq!(moved.cell.row(), 3);
    assert_eq!(moved.sheet, id(42));
    assert_eq!(b.points()[1].anchor().cell.row(), 1);
    assert_eq!(b.reference_text(1), Some("DATA!$B$2"));
    assert_eq!(b.source(), f);
    assert!(w.sheets().iter().all(|s| s.cells().is_empty()));
}
#[test]
fn model_duplicate_rejection_and_binding_are_not_excel_or_grammar_admission() {
    assert!(
        Workbook::new(
            vec![
                sheet(9, "DATA", SheetKind::Worksheet),
                sheet(42, "data", SheetKind::Worksheet)
            ],
            DateSystem::Excel1900
        )
        .is_err()
    );
    assert!(
        Workbook::new(
            vec![
                sheet(9, "A", SheetKind::Worksheet),
                sheet(9, "B", SheetKind::Worksheet)
            ],
            DateSystem::Excel1900
        )
        .is_err()
    );
    let w = book();
    // Keep the scope visible: binding alone never authorizes writing these.
    assert!(bind_points("A1 B2", &w, id(9), 2, 3).is_ok());
    assert!(bind_points("=UNKNOWN(A1)", &w, id(9), 1, 3).is_ok());
}
