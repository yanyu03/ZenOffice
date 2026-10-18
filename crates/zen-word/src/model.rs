//! RustOffice 的文字文档模型（WordprocessingML 的语义侧）。
//!
//! 单位约定：**一律用 OOXML 原生单位**，不做内部换算，避免来回转换丢精度：
//!   · 长度 → twips（1/20 磅；1 英寸 = 1440 twips）
//!   · 字号 → half-points（`24` = 12 磅）
//!   · 颜色 → 6 位十六进制大写，不带 `#`
//!
//! v0.1 支持范围（其余内容读取时进诊断，不静默丢弃）：
//!   段落 / 文本 / 换行 / 制表符 → 字符格式（粗斜下划线删除线、字号、中西文字体、颜色、上下标）
//!   → 段落格式（样式引用、对齐、缩进、段前后距、行距、段前分页）→ 表格（网格、宽、合并、边框）
//!   → 节属性（纸张、页边距、方向）

/// 1 英寸 = 1440 twips
pub const TWIPS_PER_INCH: i32 = 1440;
/// 常用纸张（twips）
pub const A4_WIDTH_TWIPS: i32 = 11_906;
pub const A4_HEIGHT_TWIPS: i32 = 16_838;
pub const LETTER_WIDTH_TWIPS: i32 = 12_240;
pub const LETTER_HEIGHT_TWIPS: i32 = 15_840;

/// Word 内置样式 ID（我们只写这几个，避免引入完整 styles.xml）
pub const STYLE_NORMAL: &str = "Normal";
pub const STYLE_HEADING1: &str = "Heading1";
pub const STYLE_HEADING2: &str = "Heading2";
pub const STYLE_HEADING3: &str = "Heading3";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    /// 正文块序列（段落与表格按文档顺序混排）
    pub blocks: Vec<Block>,
    /// Final section properties. Earlier sections end at paragraph.section_end.
    pub section: SectionProps,
    pub styles: Styles,
    pub core: CoreProps,
    pub settings: DocumentSettings,
    /// Original package and text mappings for safe imported-document saves.
    #[doc(hidden)]
    pub preservation: Option<std::sync::Arc<crate::preserve::PreservedDocument>>,
}

/// Document-wide settings. New documents retain Zen's historical 420-twip
/// interval; imported documents without this setting use OOXML's 720 twips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSettings {
    pub default_tab_stop_twips: u32,
}

impl Default for DocumentSettings {
    fn default() -> Self {
        Self { default_tab_stop_twips: 420 }
    }
}

impl DocumentSettings {
    pub(crate) fn validate(&self) -> crate::Result<()> {
        // Preserve zero as setting data. Tab layout must handle it explicitly
        // before using this interval in division or cursor arithmetic.
        if self.default_tab_stop_twips > 32_767 {
            return Err(crate::WordError::UnsupportedEdit(
                "defaultTabStop outside the supported 0..=32767 twip range".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
}

impl Block {
    pub fn paragraph(text: impl Into<String>) -> Self {
        Block::Paragraph(Paragraph::text(text))
    }
}

// ---------------------------------------------------------------- 段落与运行

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph {
    pub props: ParagraphProps,
    pub runs: Vec<Run>,
}

impl Paragraph {
    pub fn text(text: impl Into<String>) -> Self {
        Self { props: ParagraphProps::default(), runs: vec![Run::text(text)] }
    }

    pub fn with_props(props: ParagraphProps) -> Self {
        Self { props, runs: Vec::new() }
    }

    pub fn push_run(mut self, run: Run) -> Self {
        self.runs.push(run);
        self
    }

    /// 纯文本（把换行/制表符还原成字符），用于取值与对拍。
    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        for r in &self.runs {
            s.push_str(&r.plain_text());
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub props: RunProps,
    pub contents: Vec<RunContent>,
}

impl Run {
    pub fn text(text: impl Into<String>) -> Self {
        Self { props: RunProps::default(), contents: vec![RunContent::Text(text.into())] }
    }

    pub fn styled(props: RunProps, text: impl Into<String>) -> Self {
        Self { props, contents: vec![RunContent::Text(text.into())] }
    }

    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        for c in &self.contents {
            match c {
                RunContent::Text(t) => s.push_str(t),
                RunContent::Break(BreakKind::Line) => s.push('\n'),
                RunContent::Break(BreakKind::Page) => s.push('\u{000C}'),
                RunContent::Break(BreakKind::Column) => s.push('\u{000B}'),
                RunContent::Tab => s.push('\t'),
            }
        }
        s
    }

    /// 只含空文本的 run 在写出时会被丢掉（规范化），用这个判断。
    pub fn is_effectively_empty(&self) -> bool {
        self.contents.iter().all(|c| matches!(c, RunContent::Text(t) if t.is_empty()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunContent {
    Text(String),
    Break(BreakKind),
    /// 制表符（`<w:tab/>`），与文本里的 `\t` 不是一回事
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakKind {
    Line,
    Page,
    Column,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunProps {
    /// Referenced character style (w:rStyle); not direct formatting.
    pub style_id: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    /// Display-only all capitals. None inherits; direct false cancels it.
    pub caps: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    /// half-points：24 = 12 磅
    pub size_half_points: Option<u32>,
    pub font_ascii: Option<String>,
    pub font_east_asia: Option<String>,
    /// 6 位十六进制，大写，不带 `#`
    pub color: Option<String>,
    /// Explicit automatic color overrides an inherited fixed color.
    pub color_auto: bool,
    pub vert_align: Option<VertAlign>,
    pub highlight: Option<HighlightColor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightColor {
    Yellow,
    Green,
    Cyan,
    Magenta,
    Blue,
    Red,
    DarkBlue,
    DarkCyan,
    DarkGreen,
    DarkMagenta,
    DarkRed,
    DarkYellow,
    DarkGray,
    LightGray,
    Black,
}

impl HighlightColor {
    pub fn as_ooxml(self) -> &'static str {
        match self {
            HighlightColor::Yellow => "yellow",
            HighlightColor::Green => "green",
            HighlightColor::Cyan => "cyan",
            HighlightColor::Magenta => "magenta",
            HighlightColor::Blue => "blue",
            HighlightColor::Red => "red",
            HighlightColor::DarkBlue => "darkBlue",
            HighlightColor::DarkCyan => "darkCyan",
            HighlightColor::DarkGreen => "darkGreen",
            HighlightColor::DarkMagenta => "darkMagenta",
            HighlightColor::DarkRed => "darkRed",
            HighlightColor::DarkYellow => "darkYellow",
            HighlightColor::DarkGray => "darkGray",
            HighlightColor::LightGray => "lightGray",
            HighlightColor::Black => "black",
        }
    }

    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "yellow" => Some(HighlightColor::Yellow),
            "green" => Some(HighlightColor::Green),
            "cyan" => Some(HighlightColor::Cyan),
            "magenta" => Some(HighlightColor::Magenta),
            "blue" => Some(HighlightColor::Blue),
            "red" => Some(HighlightColor::Red),
            "darkBlue" => Some(HighlightColor::DarkBlue),
            "darkCyan" => Some(HighlightColor::DarkCyan),
            "darkGreen" => Some(HighlightColor::DarkGreen),
            "darkMagenta" => Some(HighlightColor::DarkMagenta),
            "darkRed" => Some(HighlightColor::DarkRed),
            "darkYellow" => Some(HighlightColor::DarkYellow),
            "darkGray" => Some(HighlightColor::DarkGray),
            "lightGray" => Some(HighlightColor::LightGray),
            "black" => Some(HighlightColor::Black),
            _ => None,
        }
    }
}

impl RunProps {
    /// Locale-independent preview casing; never changes stored run contents.
    /// Native parity for language-specific casing and small caps is separate.
    pub fn display_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if self.caps == Some(true) {
            std::borrow::Cow::Owned(text.to_uppercase())
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }

    pub fn bold() -> Self {
        Self { bold: Some(true), ..Default::default() }
    }

    pub fn size(half_points: u32) -> Self {
        Self { size_half_points: Some(half_points), ..Default::default() }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    Single,
    Double,
    None,
}

impl Underline {
    pub fn as_ooxml(self) -> &'static str {
        match self {
            Underline::Single => "single",
            Underline::Double => "double",
            Underline::None => "none",
        }
    }

    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "single" => Some(Underline::Single),
            "double" => Some(Underline::Double),
            "none" => Some(Underline::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertAlign {
    Superscript,
    Subscript,
}

impl VertAlign {
    pub fn as_ooxml(self) -> &'static str {
        match self {
            VertAlign::Superscript => "superscript",
            VertAlign::Subscript => "subscript",
        }
    }

    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "superscript" => Some(VertAlign::Superscript),
            "subscript" => Some(VertAlign::Subscript),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParagraphProps {
    /// 段落样式引用（`w:pStyle`），例如 `Heading1`
    pub style_id: Option<String>,
    pub align: Option<Align>,
    /// 左缩进（twips）
    pub ind_left_twips: Option<i32>,
    pub ind_right_twips: Option<i32>,
    /// 首行缩进（twips，负值为悬挂缩进 → 写出为 `w:hanging`）
    pub ind_first_line_twips: Option<i32>,
    pub space_before_twips: Option<u32>,
    pub space_after_twips: Option<u32>,
    pub line_spacing: Option<LineSpacing>,
    /// `w:keepNext`：与下一段同页
    pub keep_next: bool,
    /// Distinguish an explicit off value from absence, retaining the bool API.
    pub keep_next_off: bool,
    /// `w:outlineLvl`：0–8，决定导航窗格层级（内置标题用得到）
    pub outline_level: Option<u8>,
    pub page_break_before: bool,
    pub page_break_before_off: bool,
    pub shading: Option<Shading>,
    /// Local w:pPr/w:rPr properties of the paragraph mark. Retained for all
    /// paragraphs; used for the empty paragraph line box, not as text formatting.
    pub mark_run: RunProps,
    /// Properties of the section ending at this paragraph (w:pPr/w:sectPr).
    /// Stored on the paragraph so block insertion cannot invalidate an index map.
    pub section_end: Option<SectionProps>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shading {
    /// 6 位十六进制大写，不带 `#`（例如 `F2F2F2`）
    pub fill_hex: String,
}

impl Shading {
    pub fn new(fill_hex: impl Into<String>) -> Self {
        Self { fill_hex: fill_hex.into() }
    }
}

impl ParagraphProps {
    pub fn style(style_id: impl Into<String>) -> Self {
        Self { style_id: Some(style_id.into()), ..Default::default() }
    }

    pub fn align(align: Align) -> Self {
        Self { align: Some(align), ..Default::default() }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    /// 两端对齐（`both`）
    Justify,
}

impl Align {
    pub fn as_ooxml(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "both",
        }
    }

    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "left" | "start" => Some(Align::Left),
            "center" => Some(Align::Center),
            "right" | "end" => Some(Align::Right),
            "both" | "distribute" => Some(Align::Justify),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSpacing {
    /// `w:line` + `w:lineRule="auto"`：240 = 单倍
    Auto(u32),
    /// `w:lineRule="exact"`，单位 twips
    Exact(u32),
    /// `w:lineRule="atLeast"`，单位 twips
    AtLeast(u32),
}

// ---------------------------------------------------------------- 表格

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    pub props: TableProps,
    /// 各列宽度（twips），对应 `w:tblGrid`
    pub grid_twips: Vec<u32>,
    pub rows: Vec<TableRow>,
}

impl Table {
    /// 行列数与 `grid_twips` 一致的简单表格。
    pub fn new(grid_twips: Vec<u32>) -> Self {
        Self { props: TableProps::default(), grid_twips, rows: Vec::new() }
    }

    pub fn push_row(mut self, row: TableRow) -> Self {
        self.rows.push(row);
        self
    }

    pub fn column_count(&self) -> usize {
        self.grid_twips.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableProps {
    /// 整表偏好宽度（twips）；None = auto
    pub width_twips: Option<u32>,
    /// 是否写四边单线边框
    pub borders: bool,
    /// 表格样式引用（v0.1 只做读取保留，不写自定义样式）
    pub style_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

impl TableRow {
    pub fn from_texts(cells: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            cells: cells
                .into_iter()
                .map(|t| TableCell { blocks: vec![Block::paragraph(t)], ..Default::default() })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    /// `w:tcW`（twips）
    pub width_twips: Option<u32>,
    /// `w:gridSpan`；1 = 不合并（写出时不写该元素）
    pub grid_span: u32,
    pub v_merge: Option<VMerge>,
    pub shading: Option<Shading>,
    pub blocks: Vec<Block>,
}

impl Default for TableCell {
    fn default() -> Self {
        // grid_span 默认必须是 1：写出端只在 >1 时才写 gridSpan，
        // 若这里默认 0，读回就会变成 1，round-trip 立刻破。
        Self { width_twips: None, grid_span: 1, v_merge: None, shading: None, blocks: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VMerge {
    /// `w:vMerge w:val="restart"`：合并区的第一格
    Restart,
    /// `w:vMerge`（无 val 或 val="continue"）：被纵向合并的后续格
    Continue,
}

// ---------------------------------------------------------------- 节属性

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionProps {
    pub page_width_twips: i32,
    pub page_height_twips: i32,
    pub margin_top_twips: i32,
    pub margin_bottom_twips: i32,
    pub margin_left_twips: i32,
    pub margin_right_twips: i32,
    pub orientation: Orientation,
    /// `w:titlePg`
    pub title_page: bool,
    /// 多栏排版栏数（默认 1）
    pub column_count: u32,
    /// 栏间距（twips，默认 720 即 0.5 英寸）
    pub column_space_twips: u32,
    pub start: SectionStart,
    pub page_number_start: Option<u32>,
    /// OOXML number format, e.g. decimal or lowerRoman; opaque formats survive.
    pub page_number_format: Option<String>,
    pub header_distance_twips: u32,
    pub footer_distance_twips: u32,
    pub gutter_twips: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SectionStart {
    #[default]
    NextPage,
    Continuous,
    NextColumn,
    EvenPage,
    OddPage,
    Unknown(String),
}

impl SectionStart {
    pub fn as_ooxml(&self) -> &str {
        match self {
            Self::NextPage => "nextPage", Self::Continuous => "continuous",
            Self::NextColumn => "nextColumn", Self::EvenPage => "evenPage",
            Self::OddPage => "oddPage", Self::Unknown(value) => value,
        }
    }
    pub fn from_ooxml(value: &str) -> Self {
        match value {
            "nextPage" => Self::NextPage, "continuous" => Self::Continuous,
            "nextColumn" => Self::NextColumn, "evenPage" => Self::EvenPage,
            "oddPage" => Self::OddPage, other => Self::Unknown(other.into()),
        }
    }
}

/// A computed view into the block sequence, never a stored, stale offset table.
#[derive(Debug, Clone)]
pub struct SectionSpan<'a> {
    pub index: usize,
    pub blocks: std::ops::Range<usize>,
    pub properties: &'a SectionProps,
}

impl Document {
    pub fn sections(&self) -> Vec<SectionSpan<'_>> {
        let mut result = Vec::new();
        let mut start = 0;
        for (index, block) in self.blocks.iter().enumerate() {
            if let Block::Paragraph(paragraph) = block {
                if let Some(properties) = &paragraph.props.section_end {
                    result.push(SectionSpan { index: result.len(), blocks: start..index+1, properties });
                    start = index+1;
                }
            }
        }
        result.push(SectionSpan { index: result.len(), blocks: start..self.blocks.len(), properties: &self.section });
        result
    }

    /// Start another section in a newly authored document. Imported package
    /// restructuring requires a dedicated preserving operation and is refused.
    pub fn append_section(&mut self, properties: SectionProps) -> crate::Result<()> {
        if self.preservation.is_some() {
            return Err(crate::WordError::UnsupportedEdit("section insertion into an imported package is not supported".into()));
        }
        properties.validate_geometry()?;
        self.validate_sections()?;
        if !matches!(self.blocks.last(), Some(Block::Paragraph(p)) if p.props.section_end.is_none()) {
            self.blocks.push(Block::paragraph(""));
        }
        if let Some(Block::Paragraph(last)) = self.blocks.last_mut() {
            last.props.section_end = Some(self.section.clone());
        }
        self.section = properties;
        Ok(())
    }

    pub fn validate_sections(&self) -> crate::Result<()> {
        for section in self.sections() {
            section.properties.validate_geometry()?;
        }
        fn check_table(table: &Table) -> bool {
            table.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|b| match b {
                Block::Paragraph(p) => p.props.section_end.is_some(),
                Block::Table(t) => check_table(t),
            })
        }
        if self.blocks.iter().any(|b| matches!(b, Block::Table(t) if check_table(t))) ||
            self.styles.default_para.section_end.is_some() ||
            self.styles.named.iter().chain(&self.styles.builtin_overrides).any(|s| s.para.section_end.is_some()) {
            return Err(crate::WordError::UnsupportedEdit("section boundaries must be main-story paragraphs, not table cells or styles".into()));
        }
        Ok(())
    }
}

impl SectionProps {
    pub fn validate_geometry(&self) -> crate::Result<()> {
        let horizontal = i64::from(self.margin_left_twips) + i64::from(self.margin_right_twips) + i64::from(self.gutter_twips);
        let vertical = i64::from(self.margin_top_twips) + i64::from(self.margin_bottom_twips);
        if !(1..=31680).contains(&self.page_width_twips) || !(1..=31680).contains(&self.page_height_twips) ||
            [self.margin_left_twips,self.margin_right_twips,self.margin_top_twips,self.margin_bottom_twips].iter().any(|m| *m < 0) ||
            horizontal >= i64::from(self.page_width_twips) || vertical >= i64::from(self.page_height_twips) || self.column_count == 0 {
            return Err(crate::WordError::UnsupportedEdit("invalid section paper size, margins or columns".into()));
        }
        let body_width = i64::from(self.page_width_twips) - horizontal;
        let column_gaps = u64::from(self.column_count - 1) * u64::from(self.column_space_twips);
        if column_gaps >= body_width as u64 || self.header_distance_twips > self.page_height_twips as u32 || self.footer_distance_twips > self.page_height_twips as u32 {
            return Err(crate::WordError::UnsupportedEdit("section columns or header/footer distances do not fit the paper".into()));
        }
        if matches!(self.start, SectionStart::Unknown(_)) {
            return Err(crate::WordError::UnsupportedEdit("unknown section start is preserved only in the source package".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

impl Orientation {
    pub fn as_ooxml(self) -> &'static str {
        match self {
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
        }
    }

    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "portrait" => Some(Orientation::Portrait),
            "landscape" => Some(Orientation::Landscape),
            _ => None,
        }
    }
}

impl Default for SectionProps {
    /// A4 纵向，上下 1440 twips、左右 1800 twips（≈3.17 cm）
    fn default() -> Self {
        Self {
            page_width_twips: A4_WIDTH_TWIPS,
            page_height_twips: A4_HEIGHT_TWIPS,
            margin_top_twips: 1440,
            margin_bottom_twips: 1440,
            margin_left_twips: 1800,
            margin_right_twips: 1800,
            orientation: Orientation::Portrait,
            title_page: false,
            column_count: 1,
            column_space_twips: 720,
            start: SectionStart::NextPage,
            page_number_start: None,
            page_number_format: None,
            header_distance_twips: 851,
            footer_distance_twips: 992,
            gutter_twips: 0,
        }
    }
}

// ---------------------------------------------------------------- 样式

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Styles {
    /// `w:docDefaults/w:rPrDefault`
    pub default_run: RunProps,
    /// `w:docDefaults/w:pPrDefault`
    pub default_para: ParagraphProps,
    pub named: Vec<StyleDef>,
    /// External definitions replacing built-in template IDs. Kept separately
    /// so canonical generated styles still round-trip without duplicating named.
    pub builtin_overrides: Vec<StyleDef>,
    pub default_paragraph_style_id: Option<String>,
    pub default_character_style_id: Option<String>,
    /// None for new documents; imported IDs prevent inventing missing styles.
    pub imported_style_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDef {
    pub style_id: String,
    pub name: String,
    pub kind: StyleKind,
    pub based_on: Option<String>,
    pub run: RunProps,
    pub para: ParagraphProps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Paragraph,
    Character,
    Table,
    Numbering,
}

impl StyleKind {
    pub fn as_ooxml(self) -> &'static str {
        match self {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            StyleKind::Table => "table",
            StyleKind::Numbering => "numbering",
        }
    }

    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "paragraph" => Some(StyleKind::Paragraph),
            "character" => Some(StyleKind::Character),
            "table" => Some(StyleKind::Table),
            "numbering" => Some(StyleKind::Numbering),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- 文档属性

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoreProps {    pub title: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub last_modified_by: Option<String>,
    /// ISO-8601（`dcterms:created` 需要 `xsi:type="dcterms:W3CDTF"`）
    pub created: Option<String>,
    pub modified: Option<String>,
    pub keywords: Option<String>,
}

// ---------------------------------------------------------------- 模板样式

/// 写出端固定生成的内置样式定义。
///
/// **唯一事实源**：写出端按它生成 `styles.xml` 里的内置样式，读入端按它判断
/// "读到的内置样式是否就是模板本身"。两边共用一份，避免"生成用一套、比对用另一套"
/// 这种迟早会漂移的写法。
///
/// `Normal` 是段落默认样式（写出时带 `w:default="1"`）；三个标题带大纲级别，
/// 以便 WPS/Word 的导航窗格能识别层级。
pub fn template_style_defs() -> Vec<StyleDef> {
    let mut out = vec![StyleDef {
        style_id: STYLE_NORMAL.to_string(),
        name: "Normal".to_string(),
        kind: StyleKind::Paragraph,
        based_on: None,
        run: RunProps::default(),
        para: ParagraphProps::default(),
    }];

    for (id, name, sz, lvl, before, after) in [
        (STYLE_HEADING1, "heading 1", 32u32, 0u8, 240u32, 120u32),
        (STYLE_HEADING2, "heading 2", 28, 1, 240, 120),
        (STYLE_HEADING3, "heading 3", 26, 2, 240, 120),
    ] {
        out.push(StyleDef {
            style_id: id.to_string(),
            name: name.to_string(),
            kind: StyleKind::Paragraph,
            based_on: Some(STYLE_NORMAL.to_string()),
            run: RunProps { bold: Some(true), size_half_points: Some(sz), ..Default::default() },
            para: ParagraphProps {
                keep_next: true,
                space_before_twips: Some(before),
                space_after_twips: Some(after),
                outline_level: Some(lvl),
                ..Default::default()
            },
        });
    }
    out
}

/// 是否是写出端自己生成的内置样式 ID。
pub fn is_template_style_id(id: &str) -> bool {
    template_style_defs().iter().any(|s| s.style_id == id)
}
