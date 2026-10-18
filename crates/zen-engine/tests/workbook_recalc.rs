use std::collections::BTreeMap;
use zen_engine::{
    calc::{CellCoord, CellReference, DependencyGraph},
    recalc::{CalculationValue as V, RecalcError, WorkbookCalculator},
    workbook::*,
};

fn address(s: &str) -> CellAddress {
    let c = CellCoord::from_a1(s).unwrap();
    CellAddress::new(c.row as u32, c.col as u16).unwrap()
}
fn number(n: f64) -> TypedValue {
    TypedValue::Number {
        value: n,
        lexical: n.to_string(),
    }
}
fn sheet(id: u32, name: &str, cells: &[(&str, &str)]) -> Worksheet {
    let cells = cells
        .iter()
        .map(|(a, text)| {
            let formula = text.strip_prefix('=').map(|text| CellFormula {
                text: text.into(),
                kind: FormulaKind::Normal,
                shared_index: None,
                range: None,
                attributes: BTreeMap::new(),
            });
            let value = if formula.is_some() {
                Some(number(-99999.0))
            } else {
                Some(
                    text.parse::<f64>()
                        .map(number)
                        .unwrap_or(TypedValue::Text((*text).into())),
                )
            };
            let source_type = if matches!(value, Some(TypedValue::Text(_))) {
                CellType::InlineString
            } else {
                CellType::Number
            };
            (
                address(a),
                TypedCell {
                    source_type,
                    value,
                    formula,
                    style_index: None,
                },
            )
        })
        .collect();
    Worksheet::new(
        SheetId::new(id).unwrap(),
        name.into(),
        SheetVisibility::Visible,
        SheetKind::Worksheet,
        cells,
    )
    .unwrap()
}
fn book(sheets: Vec<Worksheet>) -> Workbook {
    Workbook::new(sheets, DateSystem::Excel1900).unwrap()
}
fn result(c: &WorkbookCalculator, sheet: &str, a: &str) -> V {
    c.value(sheet, address(a)).cloned().unwrap()
}

#[test]
fn every_sheet_has_its_own_context_and_caches_are_never_inputs() {
    let wb = book(vec![
        sheet(
            7,
            "Review",
            &[
                ("A1", "3"),
                ("B8", "=A1+2"),
                ("C9", "='O''Brien ! data'!J17*B8"),
                ("D4", "=IF(C9>30,SUM(1E2,ABS(-4)),0)"),
            ],
        ),
        sheet(
            2,
            "O'Brien ! data",
            &[("A1", "1000"), ("J17", "=K17*2"), ("K17", "4")],
        ),
    ]);
    let mut calc = WorkbookCalculator::new(&wb).unwrap();
    assert!(calc.value("Review", address("B8")).is_none());
    assert_eq!(calc.recalculate().unwrap().evaluated.len(), 4);
    assert_eq!(result(&calc, "review", "B8"), V::Number(5.0));
    assert_eq!(result(&calc, "Review", "C9"), V::Number(40.0));
    assert_eq!(result(&calc, "Review", "D4"), V::Number(104.0));
}

#[test]
fn names_resolve_scope_ranges_and_formula_dependencies() {
    let wb = book(vec![
        sheet(11, "Data", &[("A1", "2"), ("A2", "=A1*3")]),
        sheet(
            22,
            "Report",
            &[("B1", "=SUM(TotalRange)*Rate"), ("C1", "=Rate")],
        ),
    ])
    .with_defined_names(vec![
        DefinedName {
            name: "TotalRange".into(),
            local_sheet: None,
            formula: "Data!$A$1:$A$2".into(),
        },
        DefinedName {
            name: "Rate".into(),
            local_sheet: None,
            formula: "10".into(),
        },
        DefinedName {
            name: "Rate".into(),
            local_sheet: Some(SheetId::new(22).unwrap()),
            formula: "2".into(),
        },
    ])
    .unwrap();
    let mut calc = WorkbookCalculator::new(&wb).unwrap();
    calc.recalculate().unwrap();
    assert_eq!(result(&calc, "Report", "B1"), V::Number(16.0));
    assert_eq!(result(&calc, "Report", "C1"), V::Number(2.0));
    calc.set_input("data", address("A1"), Some(number(5.0)))
        .unwrap();
    let affected = calc
        .recalculate_dirty()
        .unwrap()
        .evaluated
        .iter()
        .map(CellReference::to_a1)
        .collect::<Vec<_>>();
    assert_eq!(affected, vec!["Data!A2", "Report!B1"]);
    assert_eq!(result(&calc, "Report", "B1"), V::Number(40.0));
}

#[test]
fn repeated_changes_formula_edits_and_clears_invalidate_only_dependents() {
    let wb = book(vec![sheet(
        1,
        "Any",
        &[("A1", "2"), ("B1", "=A1*3"), ("C1", "=B1+4"), ("D1", "=99")],
    )]);
    let mut calc = WorkbookCalculator::new(&wb).unwrap();
    calc.recalculate().unwrap();
    for n in [3.0, 7.0, 1.0] {
        calc.set_input("Any", address("A1"), Some(number(n)))
            .unwrap();
        assert!(calc.value("Any", address("C1")).is_none());
        assert_eq!(calc.recalculate_dirty().unwrap().evaluated.len(), 2);
        assert_eq!(result(&calc, "Any", "C1"), V::Number(n * 3.0 + 4.0));
        assert!(calc.recalculate_dirty().unwrap().evaluated.is_empty());
    }
    calc.set_formula("Any", address("B1"), "=A1*10").unwrap();
    calc.recalculate_dirty().unwrap();
    assert_eq!(result(&calc, "Any", "C1"), V::Number(14.0));
    calc.set_input("Any", address("B1"), None).unwrap();
    calc.recalculate_dirty().unwrap();
    assert_eq!(result(&calc, "Any", "C1"), V::Number(4.0));
    calc.set_input("Any", address("B1"), Some(number(8.0)))
        .unwrap();
    calc.recalculate_dirty().unwrap();
    assert_eq!(result(&calc, "Any", "C1"), V::Number(12.0));
}

#[test]
fn errors_propagate_through_arithmetic_and_iferror_is_lazy() {
    let mut calc = WorkbookCalculator::new(&book(vec![sheet(
        1,
        "Errors",
        &[
            ("A1", "=1/0"),
            ("B1", "=A1+10"),
            ("C1", "=IFERROR(B1,42)"),
            ("D1", "=IF(1=1,7,1/0)"),
        ],
    )]))
    .unwrap();
    calc.recalculate().unwrap();
    assert_eq!(result(&calc, "Errors", "B1"), V::Error("#DIV/0!".into()));
    assert_eq!(result(&calc, "Errors", "C1"), V::Number(42.0));
    assert_eq!(result(&calc, "Errors", "D1"), V::Number(7.0));
    calc.set_formula("Errors", address("E1"), "=IF(F1=\"\",1,0)")
        .unwrap();
    calc.recalculate_dirty().unwrap();
    assert_eq!(result(&calc, "Errors", "E1"), V::Number(1.0));
}

#[test]
fn rejected_formula_edits_leave_prior_values_and_graph_intact() {
    let mut calc = WorkbookCalculator::new(&book(vec![sheet(
        1,
        "Data",
        &[("A1", "2"), ("B1", "=A1+1")],
    )]))
    .unwrap();
    calc.recalculate().unwrap();
    for formula in [
        "=SUM(A1:A100001)",
        "=INDIRECT(\"A1\")",
        "=Table1[Amount]",
        "=Sheet1:Sheet3!A1",
        "=[x.xlsx]Data!A1",
        "=XFE1",
        "=SUM(A1:A2)+A3:A4",
        "=UnknownName+1",
        "=SUM(A1,,2)",
    ] {
        assert!(
            calc.set_formula("Data", address("B1"), formula).is_err(),
            "{formula}"
        );
        assert_eq!(result(&calc, "Data", "B1"), V::Number(3.0));
    }
    calc.set_input("Data", address("A1"), Some(number(9.0)))
        .unwrap();
    calc.recalculate_dirty().unwrap();
    assert_eq!(result(&calc, "Data", "B1"), V::Number(10.0));
}

#[test]
fn cycles_are_reported_without_reusing_source_formula_caches() {
    let mut calc = WorkbookCalculator::new(&book(vec![sheet(
        1,
        "Loop",
        &[("A1", "=B1"), ("B1", "=A1"), ("C1", "=A1+2")],
    )]))
    .unwrap();
    let RecalcError::Circular(error) = calc.recalculate().unwrap_err() else {
        panic!("expected cycle");
    };
    assert_eq!(error.cycle_nodes.len(), 2);
    assert_eq!(error.blocked_downstream_nodes.len(), 1);
    assert!(calc.value("Loop", address("A1")).is_none());
}

#[test]
fn typed_references_strings_scientific_literals_and_aggregate_rules() {
    let wb = book(vec![sheet(
        1,
        "Types",
        &[
            ("A1", "text"),
            ("A2", "3"),
            ("B1", "=SUM(A1:A2,\"4\",TRUE)"),
            ("B2", "=COUNT(A1:A3,TRUE,\"4\",\"x\")"),
            ("C1", "=IF(1E2=100,\"A1 and \"\"quotes\"\"\",\"no\")"),
            ("C2", "=PRODUCT(A3:A4)"),
        ],
    )]);
    let mut calc = WorkbookCalculator::new(&wb).unwrap();
    calc.recalculate().unwrap();
    assert_eq!(result(&calc, "Types", "B1"), V::Number(8.0));
    assert_eq!(result(&calc, "Types", "B2"), V::Number(3.0));
    assert_eq!(
        result(&calc, "Types", "C1"),
        V::Text("A1 and \"quotes\"".into())
    );
    assert_eq!(result(&calc, "Types", "C2"), V::Number(0.0));
}

#[test]
fn graph_rejected_edit_and_deleted_then_recreated_input_preserve_edges() {
    let a = CellReference::parse("A1").unwrap();
    let b = CellReference::parse("B1").unwrap();
    let c = CellReference::parse("C1").unwrap();
    let mut graph = DependencyGraph::new();
    graph.register_cell(b.clone(), Some("=A1+1"), None).unwrap();
    graph.register_cell(c.clone(), Some("=B1+1"), None).unwrap();
    assert!(
        graph
            .register_cell(b.clone(), Some("=SUM(A1:A100001)"), None)
            .is_err()
    );
    assert_eq!(graph.get_formula(&b), Some("=A1+1"));
    graph.remove_cell(&a);
    graph.clear_all_dirty();
    assert!(graph.mark_dirty(&a).contains(&c));
}

#[test]
fn shared_formulas_and_recursive_defined_names_are_rejected() {
    let mut s = sheet(1, "Test", &[("A1", "=1")]);
    let mut cells = s.cells().clone();
    cells
        .get_mut(&address("A1"))
        .unwrap()
        .formula
        .as_mut()
        .unwrap()
        .kind = FormulaKind::Shared;
    s = Worksheet::new(
        s.id(),
        s.name().into(),
        s.visibility(),
        s.kind().clone(),
        cells,
    )
    .unwrap();
    assert!(WorkbookCalculator::new(&book(vec![s])).is_err());
    let wb = book(vec![sheet(1, "Test", &[("A1", "=Alias")])])
        .with_defined_names(vec![DefinedName {
            name: "Alias".into(),
            local_sheet: None,
            formula: "Alias".into(),
        }])
        .unwrap();
    assert!(WorkbookCalculator::new(&wb).is_err());
}

#[test]
fn sumif_matches_text_wildcards_and_numeric_criteria_with_real_dependencies() {
    let wb = book(vec![sheet(
        1,
        "Regions",
        &[
            ("A1", "North"),
            ("A2", "South"),
            ("A3", "N*"),
            ("B1", "2"),
            ("B2", "3"),
            ("B3", "7"),
            ("C1", "=SUMIF(A1:A3,\"north\",B1:B3)"),
            ("C2", "=SUMIF(A1:A3,\"n*\",B1:B3)"),
            ("C3", "=SUMIF(A1:A3,\"N~*\",B1:B3)"),
            ("C4", "=SUMIF(B1:B3,\">=3\")"),
        ],
    )]);
    let mut calc = WorkbookCalculator::new(&wb).unwrap();
    calc.recalculate().unwrap();
    for (a, n) in [("C1", 2.0), ("C2", 9.0), ("C3", 7.0), ("C4", 10.0)] {
        assert_eq!(result(&calc, "Regions", a), V::Number(n));
    }
    assert!(
        calc.set_formula("Regions", address("C1"), "=SUMIF(A1:A3,\"North\",B1:B2)")
            .is_err()
    );
    calc.set_input("Regions", address("B1"), Some(number(5.0)))
        .unwrap();
    assert_eq!(calc.recalculate_dirty().unwrap().evaluated.len(), 4);
    assert_eq!(result(&calc, "Regions", "C2"), V::Number(12.0));
}

#[test]
fn deep_formula_trees_and_nonfinite_edits_are_rejected_without_losing_state() {
    let mut calc = WorkbookCalculator::new(&book(vec![sheet(
        1,
        "Data",
        &[("A1", "2"), ("B1", "=A1+1")],
    )]))
    .unwrap();
    calc.recalculate().unwrap();
    assert!(
        calc.set_formula(
            "Data",
            address("B1"),
            &format!("={}", vec!["1"; 150].join("+"))
        )
        .is_err()
    );
    assert!(
        calc.set_input("Data", address("A1"), Some(number(f64::INFINITY)))
            .is_err()
    );
    assert_eq!(result(&calc, "Data", "B1"), V::Number(3.0));
}

#[test]
fn expanding_defined_names_have_a_total_node_budget() {
    let mut names = vec![DefinedName {
        name: "Alias_0".into(),
        local_sheet: None,
        formula: "1".into(),
    }];
    for i in 1..=14 {
        names.push(DefinedName {
            name: format!("Alias_{i}"),
            local_sheet: None,
            formula: format!("SUM(Alias_{},Alias_{})", i - 1, i - 1),
        });
    }
    let wb = book(vec![sheet(1, "Test", &[("A1", "=Alias_14")])])
        .with_defined_names(names)
        .unwrap();
    let error = WorkbookCalculator::new(&wb)
        .err()
        .expect("expansion must be bounded");
    assert!(error.to_string().contains("quota"));
}

#[test]
fn large_cycle_partition_does_not_use_recursive_call_stack() {
    let mut graph = DependencyGraph::new();
    let count = 12000;
    for row in 0..count {
        let dependency = if row == 0 { count - 1 } else { row - 1 };
        graph
            .register_cell(
                CellReference::new(None, 0, row),
                Some(&format!("=A{}", dependency + 1)),
                None,
            )
            .unwrap();
    }
    graph
        .register_cell(CellReference::new(None, 1, 0), Some("=A1+1"), None)
        .unwrap();
    let error = graph.topological_sort().unwrap_err();
    assert_eq!(error.cycle_nodes.len(), count);
    assert_eq!(
        error.blocked_downstream_nodes,
        vec![CellReference::new(None, 1, 0)]
    );
}
