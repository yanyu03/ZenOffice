use super::OpcError;

/// ECMA-376-2 6.2.2: ASCII subset, ASCII case-insensitive identity.
/// Never converted into a filesystem path.
#[derive(Debug, Clone)]
pub struct PartName(String);

impl PartialEq for PartName {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}
impl Eq for PartName {}
impl PartialOrd for PartName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PartName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .bytes()
            .map(|b| b.to_ascii_lowercase())
            .cmp(other.0.bytes().map(|b| b.to_ascii_lowercase()))
    }
}
impl std::hash::Hash for PartName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.key(), state);
    }
}

impl PartName {
    pub fn parse(value: &str) -> Result<Self, OpcError> {
        if !value.is_ascii() {
            return Err(OpcError::Unsupported("non-ASCII part names".into()));
        }
        if !value.starts_with('/') || value == "/" || value.ends_with('/') {
            return Err(OpcError::Invalid(format!("Invalid part name: {value}")));
        }
        for segment in value[1..].split('/') {
            if segment.is_empty() || segment.ends_with('.') {
                return Err(OpcError::Invalid(format!("Invalid part segment: {value}")));
            }
            let bytes = segment.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                let byte = bytes[i];
                if byte == b'%' {
                    let hex = bytes
                        .get(i + 1..i + 3)
                        .ok_or_else(|| OpcError::Invalid(format!("Invalid escape: {value}")))?;
                    if !hex.iter().all(u8::is_ascii_hexdigit) {
                        return Err(OpcError::Invalid(format!("Invalid escape: {value}")));
                    }
                    let encoded = std::str::from_utf8(hex)
                        .ok()
                        .and_then(|s| u8::from_str_radix(s, 16).ok())
                        .ok_or_else(|| OpcError::Invalid(format!("Invalid escape: {value}")))?;
                    if unreserved(encoded) || matches!(encoded, b'/' | b'\\') {
                        return Err(OpcError::Invalid(format!(
                            "Disallowed encoded character: {value}"
                        )));
                    }
                    if encoded >= 128 {
                        return Err(OpcError::Unsupported(
                            "percent-encoded non-ASCII part names".into(),
                        ));
                    }
                    i += 3;
                } else if unreserved(byte) || b"!$&'()*+,;=:@".contains(&byte) {
                    i += 1;
                } else {
                    return Err(OpcError::Invalid(format!(
                        "Invalid part character: {value}"
                    )));
                }
            }
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn key(&self) -> String {
        self.0.to_ascii_lowercase()
    }
    pub fn is_relationship_part(&self) -> bool {
        let key = self.key();
        let (parent, file) = key.rsplit_once('/').unwrap();
        (parent == "/_rels" || parent.ends_with("/_rels")) && file.ends_with(".rels")
    }

    /// None is the package source; Some is a normal source part.
    pub(crate) fn relationship_source(&self) -> Result<Option<Self>, OpcError> {
        if self.as_str().eq_ignore_ascii_case("/_rels/.rels") {
            return Ok(None);
        }
        let (parent, file) = self.0.rsplit_once('/').unwrap();
        let (directory, reserved) = parent.rsplit_once('/').unwrap_or(("", parent));
        if !reserved.eq_ignore_ascii_case("_rels") || !file.to_ascii_lowercase().ends_with(".rels")
        {
            return Err(OpcError::Invalid(format!(
                "Invalid relationships part: {}",
                self.0
            )));
        }
        let source = Self::parse(&format!("{directory}/{}", &file[..file.len() - 5]))?;
        if source.is_relationship_part() {
            return Err(OpcError::Invalid(
                "A Relationships part cannot be a relationship source".into(),
            ));
        }
        Ok(Some(source))
    }
}

fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~".contains(&byte)
}

pub(crate) fn check_reference(value: &str) -> Result<(), OpcError> {
    // General IRI syntax is intentionally outside this ASCII first batch.
    if !value.is_ascii() {
        return Err(OpcError::Unsupported("non-ASCII relationship IRIs".into()));
    }
    if value.bytes().filter(|&b| b == b'#').count() > 1 {
        return Err(OpcError::Invalid(
            "Multiple fragment separators in a relationship URI".into(),
        ));
    }
    let mut chars = value.bytes();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            if !chars.next().is_some_and(|b| b.is_ascii_hexdigit())
                || !chars.next().is_some_and(|b| b.is_ascii_hexdigit())
            {
                return Err(OpcError::Invalid(
                    "Invalid relationship percent escape".into(),
                ));
            }
        } else if !(unreserved(byte) || b"!$&'()*+,;=:@/?#[]".contains(&byte)) {
            return Err(OpcError::Invalid(
                "Invalid relationship URI character".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn internal_target(
    source: Option<&PartName>,
    reference: &str,
) -> Result<(PartName, Option<String>), OpcError> {
    check_reference(reference)?;
    let (path, fragment) = reference
        .split_once('#')
        .map_or((reference, None), |(p, f)| (p, Some(f.to_owned())));
    if path.contains('?') {
        return Err(OpcError::Unsupported(
            "query components in internal relationship targets".into(),
        ));
    }
    if path.starts_with("//") || path.split('/').next().unwrap_or("").contains(':') {
        return Err(OpcError::Invalid(
            "Internal target must be a relative reference".into(),
        ));
    }
    if path.is_empty() {
        return source
            .cloned()
            .map(|name| (name, fragment))
            .ok_or_else(|| OpcError::Invalid("Package relationship must target a part".into()));
    }
    // RFC 3986 preserves a trailing directory separator after dot removal.
    // Such a resolved URI cannot identify an OPC part.
    if path.ends_with('/')
        || path.ends_with("/.")
        || path.ends_with("/..")
        || matches!(path, "." | "..")
    {
        return Err(OpcError::Invalid(
            "Internal relationship resolves to a directory, not a part".into(),
        ));
    }
    let mut segments: Vec<&str> = if path.starts_with('/') {
        vec![]
    } else {
        source
            .map(|s| s.as_str()[1..].split('/').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    if !path.starts_with('/') && source.is_some() {
        segments.pop();
    }
    for segment in path.trim_start_matches('/').split('/') {
        match segment {
            "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(OpcError::Unsupported(
                        "reference traverses above package root".into(),
                    ));
                }
            }
            "" => {
                return Err(OpcError::Invalid(
                    "Empty relationship target segment".into(),
                ));
            }
            _ => segments.push(segment),
        }
    }
    Ok((
        PartName::parse(&format!("/{}", segments.join("/")))?,
        fragment,
    ))
}
