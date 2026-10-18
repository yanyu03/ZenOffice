use super::*;
use roxmltree::Node;
use zen_engine::workbook::{
    CellAddress, CellFormula, CellType, FormulaKind, TypedCell, TypedValue,
};

pub(super) fn shared_strings(
    part: &PackagePart,
    budget: &mut Budget,
) -> Result<Vec<String>, ReadError> {
    let text = xml::decode(part, budget)?;
    let doc = xml::parse(&text, "sst", budget.limits)?;
    let mut strings = Vec::new();
    for item in xml::children(doc.root_element(), "si") {
        if strings.len() >= budget.limits.max_shared_strings {
            return Err(ReadError::Limit("shared strings"));
        }
        strings.push(rich_text(item, part.name().as_str(), budget)?);
    }
    xml::warn_unprojected(doc.root_element(), &["si"], part.name().as_str(), budget);
    Ok(strings)
}

fn rich_text(node: Node<'_, '_>, location: &str, budget: &mut Budget) -> Result<String, ReadError> {
    let plain = xml::one(node, "t")?;
    let runs: Vec<_> = xml::children(node, "r").collect();
    if plain.is_some() && !runs.is_empty() {
        return Err(invalid("mixed plain and rich text representation"));
    }
    let mut text = String::new();
    if let Some(plain) = plain {
        text = xml::escaped(&xml::leaf(plain)?)?;
    } else {
        for run in &runs {
            let value = xml::one(*run, "t")?.ok_or_else(|| invalid("rich text run missing t"))?;
            text.push_str(&xml::escaped(&xml::leaf(value)?)?);
        }
    }
    budget.text(&text)?;
    if !runs.is_empty() {
        budget.warn(
            location,
            "Rich text projected as plain text; run formatting remains in original part",
        );
    }
    xml::warn_unprojected(node, &["t", "r"], location, budget);
    Ok(text)
}

pub(super) fn worksheet(
    part: &PackagePart,
    strings: &[String],
    budget: &mut Budget,
) -> Result<BTreeMap<CellAddress, TypedCell>, ReadError> {
    let text = xml::decode(part, budget)?;
    let doc = xml::parse(&text, "worksheet", budget.limits)?;
    let root = doc.root_element();
    let data = xml::one(root, "sheetData")?.ok_or_else(|| invalid("sheetData missing"))?;
    let mut cells = BTreeMap::new();
    let mut next_row: u32 = 0;
    let mut complex_formulas = 0;
    let mut unknown_cell_metadata = 0;
    for row in xml::children(data, "row") {
        let row_index = if let Some(r) = row.attribute("r") {
            r.parse::<u32>()
                .ok()
                .and_then(|v| v.checked_sub(1))
                .ok_or_else(|| invalid("invalid row index"))?
        } else {
            next_row
        };
        if row_index < next_row || row_index >= 1_048_576 {
            return Err(invalid("duplicate/unordered/out-of-bounds row"));
        }
        next_row = row_index + 1;
        let mut next_col: u16 = 0;
        for node in xml::children(row, "c") {
            budget.cells += 1;
            if budget.cells > budget.limits.max_cells {
                return Err(ReadError::Limit("cells"));
            }
            let address = if let Some(reference) = node.attribute("r") {
                address(reference)?
            } else {
                CellAddress::new(row_index, next_col)?
            };
            if address.row() != row_index || address.col() < next_col {
                return Err(invalid("duplicate/unordered cell or row mismatch"));
            }
            next_col = address.col() + 1;
            let cell = cell(
                node,
                strings,
                budget,
                &format!("{}!{}", part.name().as_str(), address.to_a1()),
            )?;
            if cell
                .formula
                .as_ref()
                .is_some_and(|f| f.kind != FormulaKind::Normal)
            {
                complex_formulas += 1;
            }
            if node
                .attributes()
                .any(|a| a.namespace().is_some() || !["r", "s", "t"].contains(&a.name()))
                || node.children().any(|c| {
                    c.is_element()
                        && !["f", "v", "is"]
                            .iter()
                            .any(|name| c.has_tag_name((NS, *name)))
                })
            {
                unknown_cell_metadata += 1;
            }
            cells.insert(address, cell);
        }
        xml::warn_unprojected(row, &["c"], part.name().as_str(), budget);
    }
    if complex_formulas > 0 {
        budget.warn(part.name().as_str(), &format!("{complex_formulas} shared/array/data-table formula records retained without expansion or recalculation"));
    }
    if unknown_cell_metadata > 0 {
        budget.warn(part.name().as_str(), &format!("{unknown_cell_metadata} cells have unprojected attributes/extensions; retained in source only"));
    }
    xml::warn_unprojected(data, &["row"], part.name().as_str(), budget);
    xml::warn_unprojected(root, &["sheetData"], part.name().as_str(), budget);
    budget.warn(part.name().as_str(), "Cell style indices retained, not resolved; row/column presentation and page metadata are not projected");
    Ok(cells)
}

fn address(reference: &str) -> Result<CellAddress, ReadError> {
    let split = reference
        .bytes()
        .position(|b| !b.is_ascii_uppercase())
        .ok_or_else(|| invalid("invalid cell reference"))?;
    if split == 0 || !reference[split..].bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("noncanonical cell reference"));
    }
    let mut column = 0u32;
    for byte in reference[..split].bytes() {
        column = column
            .checked_mul(26)
            .and_then(|n| n.checked_add(u32::from(byte - b'A' + 1)))
            .ok_or_else(|| invalid("column overflow"))?;
    }
    let row = reference[split..]
        .parse::<u32>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or_else(|| invalid("invalid cell row"))?;
    if column == 0 || column > 16_384 {
        return Err(invalid("column out of bounds"));
    }
    Ok(CellAddress::new(row, (column - 1) as u16)?)
}

fn cell(
    node: Node<'_, '_>,
    strings: &[String],
    budget: &mut Budget,
    location: &str,
) -> Result<TypedCell, ReadError> {
    let source_type = match node.attribute("t").unwrap_or("n") {
        "n" => CellType::Number,
        "s" => CellType::SharedString,
        "inlineStr" => CellType::InlineString,
        "str" => CellType::FormulaString,
        "b" => CellType::Boolean,
        "e" => CellType::Error,
        other => return Err(ReadError::Unsupported(format!("cell type {other}"))),
    };
    let style_index = node
        .attribute("s")
        .map(|s| s.parse::<u32>().map_err(|_| invalid("invalid style index")))
        .transpose()?;
    let formula = xml::one(node, "f")?
        .map(|f| formula(f, budget))
        .transpose()?;
    let raw = xml::one(node, "v")?.map(xml::leaf).transpose()?;
    let inline = xml::one(node, "is")?;
    if source_type == CellType::InlineString && (raw.is_some() || formula.is_some())
        || source_type != CellType::InlineString && inline.is_some()
    {
        return Err(invalid("cell type/value representation mismatch"));
    }
    let value = if source_type == CellType::InlineString {
        Some(TypedValue::Text(rich_text(
            inline.ok_or_else(|| invalid("inline string missing is"))?,
            location,
            budget,
        )?))
    } else if source_type == CellType::Number && raw.as_deref().is_some_and(|s| s.trim().is_empty())
    {
        // Writers commonly use <v/> for a formula whose result is not cached.
        None
    } else if let Some(raw) = raw.as_deref() {
        Some(match source_type {
            CellType::Number => {
                let value = raw
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| invalid("invalid numeric cell"))?;
                if !value.is_finite() {
                    return Err(invalid("non-finite numeric cell"));
                }
                let lexical = raw.trim().to_owned();
                budget.text(&lexical)?;
                TypedValue::Number { value, lexical }
            }
            CellType::SharedString => {
                let index = raw
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| invalid("invalid shared string index"))?;
                let value = strings
                    .get(index)
                    .ok_or_else(|| invalid("shared string index out of bounds/missing table"))?;
                budget.text(value)?;
                TypedValue::Text(value.clone())
            }
            CellType::Boolean => TypedValue::Boolean(match raw.trim() {
                "0" | "false" => false,
                "1" | "true" => true,
                _ => return Err(invalid("invalid boolean cell")),
            }),
            CellType::Error => {
                let value = raw.to_owned();
                budget.text(&value)?;
                TypedValue::Error(value)
            }
            CellType::FormulaString => {
                let value = xml::escaped(raw)?;
                budget.text(&value)?;
                TypedValue::Text(value)
            }
            CellType::InlineString => unreachable!(),
        })
    } else {
        None
    };
    Ok(TypedCell {
        source_type,
        value,
        formula,
        style_index,
    })
}

fn formula(node: Node<'_, '_>, budget: &mut Budget) -> Result<CellFormula, ReadError> {
    let kind = match node.attribute("t").unwrap_or("normal") {
        "normal" => FormulaKind::Normal,
        "shared" => FormulaKind::Shared,
        "array" => FormulaKind::Array,
        "dataTable" => FormulaKind::DataTable,
        _ => return Err(ReadError::Unsupported("formula type".into())),
    };
    let text = xml::leaf(node)?;
    budget.text(&text)?;
    if kind == FormulaKind::Normal && text.is_empty() {
        return Err(invalid("empty normal formula"));
    }
    let shared_index = node
        .attribute("si")
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| invalid("invalid shared formula index"))
        })
        .transpose()?;
    if kind == FormulaKind::Shared && shared_index.is_none() {
        return Err(invalid("shared formula missing index"));
    }
    let range = node.attribute("ref").map(str::to_owned);
    let mut attributes = BTreeMap::new();
    for attribute in node.attributes() {
        if attribute.namespace().is_some() {
            return Err(ReadError::Unsupported(
                "namespaced formula attribute".into(),
            ));
        }
        budget.text(attribute.value())?;
        attributes.insert(attribute.name().into(), attribute.value().into());
    }
    Ok(CellFormula {
        text,
        kind,
        shared_index,
        range,
        attributes,
    })
}
