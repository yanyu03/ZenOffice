//! Caller-supplied line-height data. Font discovery and metric selection belong
//! to the provider; this module does not read files or select a global ratio.
use crate::model::RunProps;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontLineHeight {
    units_per_em: u16,
    height_units: u32,
    source: String,
}

impl FontLineHeight {
    /// `height_units` is the positive total selected by the caller, e.g.
    /// ascender - descender + lineGap. The source should identify the face,
    /// font-file identity and metric policy. It is an opaque audit label.
    pub fn new(units_per_em: u16, height_units: u32, source: String) -> Result<Self, &'static str> {
        if !(16..=16384).contains(&units_per_em) { return Err("Invalid units per em"); }
        // Bound the selected sum to three 16-bit metric magnitudes.
        if height_units == 0 || height_units > 3 * u32::from(u16::MAX) { return Err("Invalid line-height units"); }
        if source.trim().is_empty() { return Err("Missing font metric source"); }
        Ok(Self { units_per_em, height_units, source })
    }

    /// Nearest twip; half points are 10 twips. Extreme inputs saturate.
    pub fn scale_twips(&self, size_half_points: u32) -> u32 {
        let numerator = u64::from(size_half_points) * 10 * u64::from(self.height_units);
        ((numerator + u64::from(self.units_per_em) / 2) / u64::from(self.units_per_em))
            .clamp(1, u64::from(u32::MAX)) as u32
    }
}

pub trait FontLineHeightProvider {
    /// Input must already have resolved styles. A provider must return None
    /// when it cannot supply this face/script/variant safely.
    fn lookup(&self, props: &RunProps) -> Option<FontLineHeight>;

    /// Optional chunk context. Existing providers keep their strict behavior.
    fn lookup_for_text(&self, props: &RunProps, _text: &str) -> Option<FontLineHeight> {
        self.lookup(props)
    }
}

/// An explicit policy, not an automatic claim about every Office font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontLineHeightPolicy { Hhea }

/// One caller-selected static face. Parsing copies only metadata and metrics;
/// no font bytes or file handles are retained. Source identity is caller-owned.
#[derive(Debug, Clone)]
pub struct ParsedFontLineHeight {
    family: String,
    bold: bool,
    italic: bool,
    metric: FontLineHeight,
}

impl ParsedFontLineHeight {
    pub fn from_bytes(
        data: &[u8], face_index: u32, expected_family: &str,
        policy: FontLineHeightPolicy, source_identity: &str,
    ) -> Result<Self, &'static str> {
        if expected_family.trim().is_empty() || source_identity.trim().is_empty() { return Err("Missing font identity"); }
        match ttf_parser::fonts_in_collection(data) {
            Some(count) if face_index >= count => return Err("Invalid face index"),
            None if face_index != 0 => return Err("Invalid standalone face index"),
            _ => {}
        }
        let face = ttf_parser::Face::parse(data, face_index).map_err(|_| "Invalid font face")?;
        if face.raw_face().table(ttf_parser::Tag::from_bytes(b"fvar")).is_some() { return Err("Variable font metrics are not supported"); }
        if face.tables().os2.is_none() { return Err("Missing font variant metadata"); }
        let matches_family = face.names().into_iter().filter(|n| n.name_id == ttf_parser::name_id::FAMILY || n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
            .filter_map(|n| n.to_string()).any(|name| name.eq_ignore_ascii_case(expected_family));
        if !matches_family { return Err("Font family does not match requested identity"); }
        let height = match policy {
            FontLineHeightPolicy::Hhea => {
                let h = face.tables().hhea;
                if h.ascender <= 0 || h.descender > 0 || h.line_gap < 0 { return Err("Unsupported signed line metrics"); }
                i32::from(h.ascender) - i32::from(h.descender) + i32::from(h.line_gap)
            }
        };
        let metric = FontLineHeight::new(face.units_per_em(), height as u32,
            format!("{source_identity}:face={face_index}:hhea"))?;
        Ok(Self { family: expected_family.into(), bold: face.is_bold(), italic: face.is_italic() || face.is_oblique(), metric })
    }
}

impl FontLineHeightProvider for ParsedFontLineHeight {
    fn lookup(&self, props: &RunProps) -> Option<FontLineHeight> {
        if !props.font_ascii.as_deref()?.eq_ignore_ascii_case(&self.family)
            || props.bold.unwrap_or(false) != self.bold
            || props.italic.unwrap_or(false) != self.italic
            || props.font_east_asia.as_ref().is_some_and(|f| !f.eq_ignore_ascii_case(&self.family)) {
            return None;
        }
        Some(self.metric.clone())
    }

    fn lookup_for_text(&self, props: &RunProps, text: &str) -> Option<FontLineHeight> {
        // Bounded Latin path: empty paragraph marks and printable ASCII use
        // the explicit ASCII face. Other scripts retain strict slot checking.
        if text.bytes().all(|b| b.is_ascii_graphic() || b == b' ' || b == b'\t') {
            if props.font_ascii.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&self.family))
                && props.bold.unwrap_or(false) == self.bold
                && props.italic.unwrap_or(false) == self.italic {
                return Some(self.metric.clone());
            }
            return None;
        }
        self.lookup(props)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHeightTrace {
    pub requested_font: Option<String>,
    pub size_half_points: u32,
    /// None explicitly identifies the old heuristic fallback.
    pub source: Option<String>,
    pub natural_height_twips: u32,
}

pub(crate) fn natural_height(props: &RunProps, text: &str, provider: &dyn FontLineHeightProvider, traces: &mut Vec<LineHeightTrace>) -> u32 {
    let size = props.size_half_points.unwrap_or(21);
    let metric = provider.lookup_for_text(props, text);
    let height = metric.as_ref().map_or_else(
        || (size as f64 * 10.0 * 1.2).round() as u32,
        |m| m.scale_twips(size),
    );
    traces.push(LineHeightTrace {
        requested_font: props.font_ascii.clone(), size_half_points: size,
        source: metric.map(|m| m.source), natural_height_twips: height,
    });
    height
}
