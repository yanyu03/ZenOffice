use super::uri::{check_reference, internal_target};
use super::{
    OpcError, PartName, RELATIONSHIPS_MEDIA_TYPE, ReadLimits, Relationship, RelationshipTarget,
};
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::{BTreeMap, BTreeSet};

const TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

pub(crate) fn decode(bytes: &[u8], limits: &ReadLimits) -> Result<String, OpcError> {
    if bytes.len() as u64 > limits.max_metadata_xml_bytes {
        return Err(OpcError::Limit("metadata XML bytes"));
    }
    let (text, expected) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if (bytes.len() - 2) % 2 != 0 {
            return Err(OpcError::Invalid("Odd UTF-16 XML byte count".into()));
        }
        let little = bytes[0] == 0xff;
        let words: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|p| {
                if little {
                    u16::from_le_bytes([p[0], p[1]])
                } else {
                    u16::from_be_bytes([p[0], p[1]])
                }
            })
            .collect();
        (
            String::from_utf16(&words)
                .map_err(|_| OpcError::Invalid("Invalid UTF-16 XML".into()))?,
            "UTF-16",
        )
    } else {
        let data = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        (
            std::str::from_utf8(data)
                .map_err(|_| {
                    OpcError::Unsupported("metadata XML requires UTF-8 or BOM-marked UTF-16".into())
                })?
                .to_owned(),
            "UTF-8",
        )
    };
    if let Some(declaration) = text
        .strip_prefix("<?xml")
        .and_then(|s| s.split_once("?>").map(|(d, _)| d))
    {
        if let Some(rest) = declaration
            .split_once("encoding")
            .map(|(_, s)| s.trim_start())
        {
            let rest = rest
                .strip_prefix('=')
                .ok_or_else(|| OpcError::Invalid("Invalid XML encoding declaration".into()))?
                .trim_start();
            let quote = rest
                .chars()
                .next()
                .ok_or_else(|| OpcError::Invalid("Missing XML encoding".into()))?;
            if !matches!(quote, '\'' | '"') {
                return Err(OpcError::Invalid("Invalid XML encoding quote".into()));
            }
            let encoding = rest[1..].split(quote).next().unwrap_or("");
            if !encoding.eq_ignore_ascii_case(expected) {
                return Err(OpcError::Unsupported(format!(
                    "XML declaration {encoding} disagrees with {expected} bytes"
                )));
            }
        }
    }
    Ok(text)
}

fn parse<'a>(text: &'a str, limits: &ReadLimits) -> Result<Document<'a>, OpcError> {
    Ok(Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: limits.max_xml_nodes,
        },
    )?)
}

fn attributes(node: Node<'_, '_>, allowed: &[&str]) -> Result<(), OpcError> {
    for attribute in node.attributes() {
        if attribute.namespace().is_some() || !allowed.contains(&attribute.name()) {
            return Err(OpcError::Unsupported(format!(
                "Unsupported metadata attribute: {}",
                attribute.name()
            )));
        }
    }
    Ok(())
}
fn required(node: Node<'_, '_>, key: &str) -> Result<String, OpcError> {
    node.attribute(key)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| OpcError::Invalid(format!("Missing metadata attribute: {key}")))
}
fn no_children(node: Node<'_, '_>, allow_text: bool) -> Result<(), OpcError> {
    if node.children().any(|n| {
        n.is_element() || (!allow_text && n.is_text() && !n.text().unwrap_or("").trim().is_empty())
    }) {
        return Err(OpcError::Invalid("Unexpected metadata content".into()));
    }
    Ok(())
}
fn root_content(root: Node<'_, '_>) -> Result<(), OpcError> {
    if root
        .children()
        .any(|n| n.is_text() && !n.text().unwrap_or("").trim().is_empty())
    {
        return Err(OpcError::Invalid("Unexpected root text".into()));
    }
    Ok(())
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
fn media_type(value: &str) -> Result<(), OpcError> {
    if value.contains(';') {
        return Err(OpcError::Unsupported(
            "parameterized content types in this reader batch".into(),
        ));
    }
    let (major, minor) = value
        .split_once('/')
        .ok_or_else(|| OpcError::Invalid("Invalid content type".into()))?;
    if !token(major) || !token(minor) {
        return Err(OpcError::Invalid("Invalid MIME type/subtype".into()));
    }
    Ok(())
}

pub(super) struct ContentTypes {
    defaults: BTreeMap<String, String>,
    overrides: BTreeMap<String, String>,
}
impl ContentTypes {
    pub fn parse(bytes: &[u8], limits: &ReadLimits) -> Result<Self, OpcError> {
        let text = decode(bytes, limits)?;
        let doc = parse(&text, limits)?;
        let root = doc.root_element();
        if !root.has_tag_name((TYPES_NS, "Types")) {
            return Err(OpcError::Invalid(
                "Invalid Content Types root namespace".into(),
            ));
        }
        attributes(root, &[])?;
        root_content(root)?;
        let mut result = Self {
            defaults: BTreeMap::new(),
            overrides: BTreeMap::new(),
        };
        for node in root.children().filter(|n| n.is_element()) {
            no_children(node, false)?;
            let content_type = required(node, "ContentType")?;
            media_type(&content_type)?;
            let inserted = if node.has_tag_name((TYPES_NS, "Default")) {
                attributes(node, &["Extension", "ContentType"])?;
                let extension = required(node, "Extension")?;
                check_reference(&extension)?;
                if extension.bytes().any(|b| b"/.?#;[]".contains(&b)) {
                    return Err(OpcError::Invalid("Invalid content type extension".into()));
                }
                result
                    .defaults
                    .insert(extension.to_ascii_lowercase(), content_type)
            } else if node.has_tag_name((TYPES_NS, "Override")) {
                attributes(node, &["PartName", "ContentType"])?;
                result.overrides.insert(
                    PartName::parse(&required(node, "PartName")?)?.key(),
                    content_type,
                )
            } else {
                return Err(OpcError::Unsupported("unknown Content Types markup".into()));
            };
            if inserted.is_some() {
                return Err(OpcError::Invalid("Duplicate content type mapping".into()));
            }
        }
        Ok(result)
    }
    pub fn for_part(&self, name: &PartName) -> Result<String, OpcError> {
        let extension = name
            .as_str()
            .rsplit('/')
            .next()
            .unwrap()
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        let mapped = self
            .overrides
            .get(&name.key())
            .or_else(|| extension.as_ref().and_then(|e| self.defaults.get(e)));
        if name.is_relationship_part() {
            if mapped.is_some_and(|value| !value.eq_ignore_ascii_case(RELATIONSHIPS_MEDIA_TYPE)) {
                return Err(OpcError::Invalid(
                    "Incorrect Relationships part content type".into(),
                ));
            }
            // Part 2 7.2.3.2.1 excludes Relationships parts from required mappings.
            return Ok(RELATIONSHIPS_MEDIA_TYPE.into());
        }
        mapped
            .cloned()
            .ok_or_else(|| OpcError::Invalid(format!("Missing content type: {}", name.as_str())))
    }
}

pub(super) fn relationships(
    bytes: &[u8],
    source: Option<&PartName>,
    limits: &ReadLimits,
) -> Result<Vec<Relationship>, OpcError> {
    let text = decode(bytes, limits)?;
    let doc = parse(&text, limits)?;
    let root = doc.root_element();
    if !root.has_tag_name((RELS_NS, "Relationships")) {
        return Err(OpcError::Invalid(
            "Invalid Relationships root namespace".into(),
        ));
    }
    attributes(root, &[])?;
    root_content(root)?;
    let mut ids = BTreeSet::new();
    let mut result = Vec::new();
    for node in root.children().filter(|n| n.is_element()) {
        if result.len() >= limits.max_relationships {
            return Err(OpcError::Limit("relationships"));
        }
        if !node.has_tag_name((RELS_NS, "Relationship")) {
            return Err(OpcError::Unsupported(
                "MCE or unknown Relationships markup".into(),
            ));
        }
        attributes(node, &["Id", "Type", "Target", "TargetMode"])?;
        no_children(node, true)?;
        let id = required(node, "Id")?;
        if !id.is_ascii() {
            return Err(OpcError::Unsupported("non-ASCII relationship IDs".into()));
        }
        if !id
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(OpcError::Invalid(
                "Relationship Id must be an XML NCName".into(),
            ));
        }
        if !ids.insert(id.clone()) {
            return Err(OpcError::Invalid(
                "Duplicate relationship Id in one source".into(),
            ));
        }
        let relationship_type = required(node, "Type")?;
        check_reference(&relationship_type)?;
        let raw_target = required(node, "Target")?;
        check_reference(&raw_target)?;
        let target = match node.attribute("TargetMode").unwrap_or("Internal") {
            "Internal" => {
                let (part, fragment) = internal_target(source, &raw_target)?;
                RelationshipTarget::Internal { part, fragment }
            }
            "External" => RelationshipTarget::External {
                reference: raw_target.clone(),
            },
            _ => return Err(OpcError::Invalid("Invalid TargetMode".into())),
        };
        result.push(Relationship {
            source: source.cloned(),
            id,
            relationship_type,
            raw_target,
            target,
        });
    }
    Ok(result)
}
