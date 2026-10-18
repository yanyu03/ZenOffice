//! Worksheet print metadata. It does not change ordinary grid geometry.
use super::Sheet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PageOrientation {
    #[default]
    Portrait,
    Landscape,
}

impl PageOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Portrait => "纵向",
            Self::Landscape => "横向",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaperSize {
    #[default]
    A4,
    A3,
    A5,
    B5,
    Letter,
    Legal,
}

impl PaperSize {
    pub const ALL: [Self; 6] = [
        Self::A4,
        Self::A3,
        Self::A5,
        Self::B5,
        Self::Letter,
        Self::Legal,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::A5 => "A5",
            Self::B5 => "B5 (JIS)",
            Self::Letter => "Letter",
            Self::Legal => "Legal",
        }
    }
    /// OOXML/Excel paper identifiers, corroborated by the extracted XlPaperSize.
    pub fn ooxml_id(self) -> u8 {
        match self {
            Self::A4 => 9,
            Self::A3 => 8,
            Self::A5 => 11,
            Self::B5 => 13,
            Self::Letter => 1,
            Self::Legal => 5,
        }
    }
    pub fn dimensions_mm(self) -> [f32; 2] {
        match self {
            Self::A4 => [210.0, 297.0],
            Self::A3 => [297.0, 420.0],
            Self::A5 => [148.0, 210.0],
            Self::B5 => [182.0, 257.0],
            Self::Letter => [215.9, 279.4],
            Self::Legal => [215.9, 355.6],
        }
    }
}

/// Four page margins in micrometres, ordered left/right/top/bottom.
/// This first editing subset permits 0..=50 mm per edge. All supported paper
/// sizes remain printable in either orientation; NaN/infinity cannot enter it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageMargins([u32; 4]);

impl PageMargins {
    pub const NORMAL: Self = Self([17_780, 17_780, 19_050, 19_050]);
    pub const WIDE: Self = Self([25_400; 4]);
    pub const NARROW: Self = Self([6_350; 4]);
    pub const PRESETS: [(&'static str, Self); 3] = [
        ("普通", Self::NORMAL),
        ("宽", Self::WIDE),
        ("窄", Self::NARROW),
    ];

    pub fn from_mm(values: [f64; 4]) -> Result<Self, &'static str> {
        if values
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=50.0).contains(v))
        {
            return Err("每一边请输入 0 至 50 mm 的有限数值");
        }
        Ok(Self(values.map(|v| (v * 1000.0).round() as u32)))
    }

    pub fn mm(self) -> [f64; 4] {
        self.0.map(|v| f64::from(v) / 1000.0)
    }

    /// SpreadsheetML pageMargins use inches, whereas WPS/Excel COM uses points.
    pub fn inches(self) -> [f64; 4] {
        self.0.map(|v| f64::from(v) / 25_400.0)
    }
}

impl Default for PageMargins {
    fn default() -> Self {
        // Retain the previous rust_xlsxwriter output defaults exactly.
        Self::NORMAL
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageSettings {
    pub orientation: PageOrientation,
    pub paper: PaperSize,
    pub print_gridlines: bool,
    pub print_headings: bool,
    pub margins: PageMargins,
}

impl PageSettings {
    pub fn dimensions_mm(self) -> [f32; 2] {
        let [width, height] = self.paper.dimensions_mm();
        match self.orientation {
            PageOrientation::Portrait => [width, height],
            PageOrientation::Landscape => [height, width],
        }
    }
}

impl Sheet {
    pub fn page_settings(&self) -> PageSettings {
        self.page_settings
    }
    pub fn set_page_orientation(&mut self, orientation: PageOrientation) {
        self.page_settings.orientation = orientation;
    }
    pub fn set_paper_size(&mut self, paper: PaperSize) {
        self.page_settings.paper = paper;
    }
    pub fn set_print_gridlines(&mut self, enabled: bool) {
        self.page_settings.print_gridlines = enabled;
    }
    pub fn set_print_headings(&mut self, enabled: bool) {
        self.page_settings.print_headings = enabled;
    }
    pub fn set_page_margins(&mut self, margins: PageMargins) {
        self.page_settings.margins = margins;
    }
}
