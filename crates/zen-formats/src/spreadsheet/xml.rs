use super::*;
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::BTreeSet;

pub(super) fn decode(part: &PackagePart, budget: &mut Budget) -> Result<String, ReadError> {
    budget.xml_bytes = budget
        .xml_bytes
        .checked_add(part.bytes().len() as u64)
        .ok_or(ReadError::Limit("total XML bytes"))?;
    if budget.xml_bytes > budget.limits.max_total_xml_bytes {
        return Err(ReadError::Limit("total XML bytes"));
    }
    let mut limits = budget.limits.package.clone();
    limits.max_metadata_xml_bytes = budget.limits.max_xml_bytes;
    Ok(crate::opc::xml::decode(part.bytes(), &limits)?)
}
pub const MC_NS: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

const KNOWN_NAMESPACES: &[&str] = &[
    NS,
    REL_NS,
    "http://schemas.openxmlformats.org/package/2006/relationships",
    MC_NS,
    "http://www.w3.org/XML/1998/namespace",
];

fn is_namespace_understood(uri: &str) -> bool {
    KNOWN_NAMESPACES.contains(&uri)
}

fn is_prefix_understood(node: Node<'_, '_>, prefix: &str) -> bool {
    if let Some(uri) = node.lookup_namespace_uri(Some(prefix)) {
        is_namespace_understood(uri)
    } else {
        false
    }
}

fn validate_element_mce(node: Node<'_, '_>) -> Result<(), ReadError> {
    for attr in node.attributes() {
        if attr.namespace() == Some(MC_NS) && attr.name() == "MustUnderstand" {
            for prefix in attr.value().split_whitespace() {
                if let Some(uri) = node.lookup_namespace_uri(Some(prefix)) {
                    if !is_namespace_understood(uri) {
                        return Err(ReadError::Unsupported(format!(
                            "mc:MustUnderstand requires unsupported namespace: prefix '{prefix}' ({uri})"
                        )));
                    }
                } else {
                    return Err(ReadError::Unsupported(format!(
                        "mc:MustUnderstand requires unbound namespace prefix '{prefix}'"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_mce_tree(node: Node<'_, '_>) -> Result<(), ReadError> {
    if !node.is_element() {
        return Ok(());
    }

    if node.has_tag_name((MC_NS, "AlternateContent")) {
        let mut selected_child = None;
        for choice in node.children().filter(|n| n.is_element() && n.has_tag_name((MC_NS, "Choice"))) {
            let requires = choice.attribute("Requires").unwrap_or("");
            let all_understood = !requires.is_empty()
                && requires.split_whitespace().all(|p| is_prefix_understood(choice, p));
            if all_understood {
                selected_child = Some(choice);
                break;
            }
        }
        if selected_child.is_none() {
            if let Some(fallback) = node.children().find(|n| n.is_element() && n.has_tag_name((MC_NS, "Fallback"))) {
                selected_child = Some(fallback);
            }
        }

        if let Some(selected) = selected_child {
            validate_element_mce(selected)?;
            for child in selected.children() {
                validate_mce_tree(child)?;
            }
        }
        return Ok(());
    }

    validate_element_mce(node)?;
    for child in node.children() {
        validate_mce_tree(child)?;
    }
    Ok(())
}

pub(super) fn parse<'a>(
    text: &'a str,
    root_name: &str,
    limits: &WorkbookReadLimits,
) -> Result<Document<'a>, ReadError> {
    let doc = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: limits.max_xml_nodes,
        },
    )?;
    if !doc.root_element().has_tag_name((NS, root_name)) {
        return Err(ReadError::Unsupported(format!(
            "expected Transitional {root_name} namespace"
        )));
    }
    validate_mce_tree(doc.root_element())?;
    Ok(doc)
}

fn collect_effective_children<'a, 'i>(
    node: Node<'a, 'i>,
    out: &mut Vec<Node<'a, 'i>>,
) {
    for child in node.children().filter(|n| n.is_element()) {
        if child.has_tag_name((MC_NS, "AlternateContent")) {
            let mut selected = None;
            for choice in child.children().filter(|n| n.is_element() && n.has_tag_name((MC_NS, "Choice"))) {
                let requires = choice.attribute("Requires").unwrap_or("");
                let all_understood = !requires.is_empty()
                    && requires.split_whitespace().all(|p| is_prefix_understood(choice, p));
                if all_understood {
                    selected = Some(choice);
                    break;
                }
            }
            if selected.is_none() {
                if let Some(fallback) = child.children().find(|n| n.is_element() && n.has_tag_name((MC_NS, "Fallback"))) {
                    selected = Some(fallback);
                }
            }
            if let Some(branch) = selected {
                collect_effective_children(branch, out);
            }
        } else {
            out.push(child);
        }
    }
}

pub(super) fn children<'a, 'i>(
    node: Node<'a, 'i>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    let mut items = Vec::new();
    collect_effective_children(node, &mut items);
    items.into_iter().filter(move |n| n.has_tag_name((NS, name)))
}
pub(super) fn one<'a, 'i>(
    node: Node<'a, 'i>,
    name: &'a str,
) -> Result<Option<Node<'a, 'i>>, ReadError> {
    let mut nodes = children(node, name);
    let first = nodes.next();
    if nodes.next().is_some() {
        return Err(invalid(&format!("duplicate {name} element")));
    }
    Ok(first)
}
pub(super) fn required<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Result<&'a str, ReadError> {
    node.attribute(name)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(&format!("missing {name} attribute")))
}
pub(super) fn leaf(node: Node<'_, '_>) -> Result<String, ReadError> {
    if node.children().any(|n| n.is_element()) {
        return Err(invalid("unexpected nested element in scalar value"));
    }
    Ok(node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect())
}
pub(super) fn warn_unprojected(
    node: Node<'_, '_>,
    known: &[&str],
    location: &str,
    budget: &mut Budget,
) {
    let names: BTreeSet<_> = node
        .children()
        .filter(|n| n.is_element())
        .filter(|n| n.tag_name().namespace() != Some(NS) || !known.contains(&n.tag_name().name()))
        .map(|n| {
            format!(
                "{{{}}}{}",
                n.tag_name().namespace().unwrap_or(""),
                n.tag_name().name()
            )
        })
        .collect();
    if !names.is_empty() {
        budget.warn(
            location,
            &format!(
                "Unprojected elements retained in source: {}",
                names.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }
}

/// Decode ST_Xstring escapes in one pass, including escaped literal underscores
/// and UTF-16 surrogate pairs. Do not recursively interpret decoded escapes.
pub(super) fn escaped(text: &str) -> Result<String, ReadError> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut words = Vec::new();
    while index < bytes.len() {
        if bytes.get(index..index + 2) == Some(b"_x") && bytes.get(index + 6) == Some(&b'_') {
            if let Some(hex) = bytes
                .get(index + 2..index + 6)
                .filter(|s| s.iter().all(u8::is_ascii_hexdigit))
            {
                let word = u16::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap();
                words.push(word);
                index += 7;
                continue;
            }
        }
        let character = text[index..].chars().next().unwrap();
        let mut buffer = [0u16; 2];
        words.extend_from_slice(character.encode_utf16(&mut buffer));
        index += character.len_utf8();
    }
    String::from_utf16(&words)
        .map_err(|_| ReadError::Unsupported("unpaired surrogate in escaped string".into()))
}
