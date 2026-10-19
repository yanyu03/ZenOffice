//! Bounded, read-only SpreadsheetML projection above the existing OPC layer.
//! Original parts stay owned here. This API provides no editable round-trip save.
mod cells;
mod dates;
mod xml;
pub mod edit;

use crate::opc::{
    OpcError, Package, PackagePart, PartName, ReadLimits, Relationship, RelationshipTarget,
};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};
use zen_engine::workbook::{DateSystem, DefinedName, SheetId, SheetKind, SheetVisibility, Workbook, Worksheet};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WORKBOOK_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const WORKSHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const STRINGS_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Opc(#[from] OpcError),
    #[error(transparent)]
    Xml(#[from] roxmltree::Error),
    #[error(transparent)]
    Model(#[from] zen_engine::workbook::ModelError),
    #[error("Invalid SpreadsheetML: {0}")]
    Invalid(String),
    #[error("SpreadsheetML subset does not support: {0}")]
    Unsupported(String),
    #[error("SpreadsheetML resource limit: {0}")]
    Limit(&'static str),
}

#[derive(Debug, Clone)]
pub struct WorkbookReadLimits {
    pub package: ReadLimits,
    pub max_xml_bytes: u64,
    pub max_total_xml_bytes: u64,
    pub max_xml_nodes: u32,
    pub max_sheets: usize,
    pub max_cells: usize,
    pub max_shared_strings: usize,
    pub max_text_bytes: usize,
}
impl Default for WorkbookReadLimits {
    fn default() -> Self {
        Self {
            package: ReadLimits::default(),
            max_xml_bytes: 8 * 1024 * 1024,
            max_total_xml_bytes: 64 * 1024 * 1024,
            max_xml_nodes: 300_000,
            max_sheets: 128,
            max_cells: 100_000,
            max_shared_strings: 100_000,
            max_text_bytes: 16 * 1024 * 1024,
        }
    }
}
impl WorkbookReadLimits {
    /// Explicit bounded desktop tier: accepts the pinned 350k-cell delivery
    /// workload while retaining the smaller default for untrusted previews.
    pub fn office_standard() -> Self {
        Self {
            max_xml_bytes: 32 * 1024 * 1024,
            max_total_xml_bytes: 128 * 1024 * 1024,
            max_xml_nodes: 3_000_000,
            max_cells: 500_000,
            max_shared_strings: 200_000,
            max_text_bytes: 32 * 1024 * 1024,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadDiagnostic {
    pub location: String,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveCapability {
    ReadOnlyProjection,
}

/// Retains raw OPC bytes and source mappings alongside the typed projection.
/// Read-only access prevents the preview from authorizing a destructive save.
#[derive(Debug)]
pub struct ReadWorkbook {
    package: Package,
    workbook: Workbook,
    sheet_parts: BTreeMap<SheetId, PartName>,
    workbook_part: PartName,
    diagnostics: Vec<ReadDiagnostic>,
}
impl ReadWorkbook {
    pub fn open(path: &Path, limits: &WorkbookReadLimits) -> Result<Self, ReadError> {
        Self::from_package(Package::open(path, &limits.package)?, limits)
    }
    pub fn read(bytes: &[u8], limits: &WorkbookReadLimits) -> Result<Self, ReadError> {
        Self::from_package(Package::read(bytes, &limits.package)?, limits)
    }
    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }
    pub fn package(&self) -> &Package {
        &self.package
    }
    pub fn workbook_part(&self) -> &PartName {
        &self.workbook_part
    }
    pub fn sheet_part(&self, id: SheetId) -> Option<&PartName> {
        self.sheet_parts.get(&id)
    }
    pub fn diagnostics(&self) -> &[ReadDiagnostic] {
        &self.diagnostics
    }
    pub fn save_capability(&self) -> SaveCapability {
        SaveCapability::ReadOnlyProjection
    }

    pub fn from_package(package: Package, limits: &WorkbookReadLimits) -> Result<Self, ReadError> {
        if package.relationships_from(None).any(|r| {
            matches!(
                r.relationship_type.as_str(),
                crate::opc::OFFICE_DOCUMENT_RELATIONSHIP
                    | crate::opc::STRICT_OFFICE_DOCUMENT_RELATIONSHIP
            ) && matches!(
                &r.target,
                RelationshipTarget::Internal {
                    fragment: Some(_),
                    ..
                }
            )
        }) {
            return Err(invalid("officeDocument relationship must be fragment-free"));
        }
        let main = package
            .office_document()?
            .ok_or_else(|| invalid("officeDocument relationship missing"))?;
        if main.content_type() != WORKBOOK_TYPE {
            return Err(ReadError::Unsupported(format!(
                "workbook media type {} (XLSX Transitional only)",
                main.content_type()
            )));
        }
        let mut budget = Budget::new(limits);
        for diagnostic in package
            .diagnostics()
            .iter()
            .filter(|d| d.code != crate::opc::DiagnosticCode::OpaqueDocumentContent)
        {
            budget.warn(&diagnostic.location, &diagnostic.message);
        }
        let text = xml::decode(main, &mut budget)?;
        let document = xml::parse(&text, "workbook", limits)?;
        let root = document.root_element();
        let date_system = match xml::one(root, "workbookPr")?.and_then(|n| n.attribute("date1904"))
        {
            None | Some("0" | "false") => DateSystem::Excel1900,
            Some("1" | "true") => DateSystem::Excel1904,
            Some(_) => return Err(invalid("invalid date1904 boolean")),
        };
        let shared = singleton_link(&package, main.name(), "sharedStrings")?;
        let strings = if let Some(link) = shared {
            let part = related_part(&package, link)?;
            if part.content_type() != STRINGS_TYPE {
                return Err(invalid("sharedStrings media type mismatch"));
            }
            cells::shared_strings(part, &mut budget)?
        } else {
            Vec::new()
        };
        let sheets_node =
            xml::one(root, "sheets")?.ok_or_else(|| invalid("sheets element missing"))?;
        let mut sheets = Vec::new();
        let mut sheet_parts = BTreeMap::new();
        let mut targets = HashSet::new();
        for node in xml::children(sheets_node, "sheet") {
            if sheets.len() >= limits.max_sheets {
                return Err(ReadError::Limit("sheets"));
            }
            let id = SheetId::new(
                xml::required(node, "sheetId")?
                    .parse()
                    .map_err(|_| invalid("invalid sheetId"))?,
            )
            .ok_or_else(|| invalid("zero sheetId"))?;
            if sheet_parts.contains_key(&id) {
                return Err(invalid("duplicate sheetId"));
            }
            let name = xml::escaped(xml::required(node, "name")?)?;
            budget.text(&name)?;
            let visibility = match node.attribute("state").unwrap_or("visible") {
                "visible" => SheetVisibility::Visible,
                "hidden" => SheetVisibility::Hidden,
                "veryHidden" => SheetVisibility::VeryHidden,
                _ => return Err(invalid("invalid sheet visibility")),
            };
            let rid = node
                .attribute((REL_NS, "id"))
                .ok_or_else(|| invalid("sheet r:id missing"))?;
            let link = package
                .relationships_from(Some(main.name()))
                .find(|r| r.id == rid)
                .ok_or_else(|| invalid("sheet relationship missing"))?;
            let part = related_part(&package, link)?;
            if !targets.insert(part.name().as_str().to_ascii_lowercase()) {
                return Err(invalid("two sheets target the same part"));
            }
            let (kind, data) = match link.relationship_type.strip_prefix(&format!("{REL_NS}/")) {
                Some("worksheet") => {
                    if part.content_type() != WORKSHEET_TYPE {
                        return Err(invalid("worksheet media type mismatch"));
                    }
                    (
                        SheetKind::Worksheet,
                        cells::worksheet(part, &strings, &mut budget)?,
                    )
                }
                Some("chartsheet") => {
                    if part.content_type()
                        != "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml"
                    {
                        return Err(invalid("chartsheet media type mismatch"));
                    }
                    budget.warn(
                        part.name().as_str(),
                        "Chartsheet retained as opaque; no cells projected",
                    );
                    (SheetKind::Chartsheet, BTreeMap::new())
                }
                Some("dialogsheet" | "xlMacrosheet" | "xlIntlMacrosheet") => {
                    budget.warn(
                        part.name().as_str(),
                        "Legacy sheet retained as opaque; not executed",
                    );
                    (
                        SheetKind::Opaque(link.relationship_type.clone()),
                        BTreeMap::new(),
                    )
                }
                _ => return Err(ReadError::Unsupported("sheet relationship type".into())),
            };
            sheet_parts.insert(id, part.name().clone());
            sheets.push(Worksheet::new(id, name, visibility, kind, data)?);
        }
        let mut defined_names = Vec::new();
        if let Some(container) = xml::one(root, "definedNames")? {
            for node in xml::children(container, "definedName") {
                if defined_names.len() >= limits.max_cells {
                    return Err(ReadError::Limit("defined names"));
                }
                let name = xml::escaped(xml::required(node, "name")?)?;
                let formula = xml::leaf(node)?;
                budget.text(&name)?;
                budget.text(&formula)?;
                let local_sheet = node.attribute("localSheetId").map(|index| {
                    let index: usize = index.parse().map_err(|_| invalid("invalid localSheetId"))?;
                    sheets.get(index).map(Worksheet::id)
                        .ok_or_else(|| invalid("localSheetId outside workbook sheet order"))
                }).transpose()?;
                defined_names.push(DefinedName { name, local_sheet, formula });
            }
        }
        xml::warn_unprojected(
            root,
            &["sheets", "workbookPr", "definedNames"],
            main.name().as_str(),
            &mut budget,
        );
        budget.warn(main.name().as_str(), "Read-only cell projection: styles, dates, shared/array calculation and opaque part editing are not implemented; source caches may be stale");
        if package
            .relationships()
            .iter()
            .any(|r| matches!(r.target, RelationshipTarget::External { .. }))
        {
            budget.warn(
                "/",
                "External relationships retained without opening or downloading their targets",
            );
        }
        let workbook_part = main.name().clone();
        let workbook = Workbook::new(sheets, date_system)?.with_defined_names(defined_names)?;
        Ok(Self {
            package,
            workbook,
            workbook_part,
            sheet_parts,
            diagnostics: budget.diagnostics,
        })
    }
}

fn invalid(message: &str) -> ReadError {
    ReadError::Invalid(message.into())
}
fn related_part<'a>(
    package: &'a Package,
    link: &Relationship,
) -> Result<&'a PackagePart, ReadError> {
    match &link.target {
        RelationshipTarget::Internal {
            part,
            fragment: None,
        } => package
            .part(part)
            .ok_or_else(|| invalid("related part missing")),
        _ => Err(invalid(
            "workbook semantic relationships must be internal and fragment-free",
        )),
    }
}
fn singleton_link<'a>(
    package: &'a Package,
    source: &'a PartName,
    kind: &str,
) -> Result<Option<&'a Relationship>, ReadError> {
    let uri = format!("{REL_NS}/{kind}");
    let mut links = package
        .relationships_from(Some(source))
        .filter(|r| r.relationship_type == uri);
    let link = links.next();
    if links.next().is_some() {
        return Err(invalid("duplicate singleton relationship"));
    }
    Ok(link)
}

struct Budget<'a> {
    limits: &'a WorkbookReadLimits,
    xml_bytes: u64,
    cells: usize,
    text_bytes: usize,
    diagnostics: Vec<ReadDiagnostic>,
}
impl<'a> Budget<'a> {
    fn new(limits: &'a WorkbookReadLimits) -> Self {
        Self {
            limits,
            xml_bytes: 0,
            cells: 0,
            text_bytes: 0,
            diagnostics: Vec::new(),
        }
    }
    fn text(&mut self, text: &str) -> Result<(), ReadError> {
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(ReadError::Limit("decoded text"))?;
        if self.text_bytes > self.limits.max_text_bytes {
            return Err(ReadError::Limit("decoded text"));
        }
        Ok(())
    }
    fn warn(&mut self, location: &str, message: &str) {
        if self.diagnostics.len() < 255 {
            self.diagnostics.push(ReadDiagnostic {
                location: location.into(),
                message: message.into(),
            });
        } else if self.diagnostics.len() == 255 {
            self.diagnostics.push(ReadDiagnostic {
                location: "/".into(),
                message: "Additional diagnostic details omitted (256-entry bound)".into(),
            });
        }
    }
}
