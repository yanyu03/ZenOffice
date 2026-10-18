//! Zen PresentationML 数据模型 (ECMA-376 Part 1 Chapter 19).
//! 单位约定：
//! - 长度与位置：EMU (English Metric Units, 1 pt = 12700 EMU, 1 in = 914400 EMU)
//! - 字号：1/100 pt (e.g. 2400 = 24pt, 对应 DrawingML a:rPr sz)
//! - 颜色：6位大写十六进制 RGB (e.g. "FFFFFF", "262E3E")

pub const EMU_PER_INCH: i64 = 914_400;
pub const EMU_PER_PT: i64 = 12_700;

/// 默认 16:9 幻灯片尺寸 (EMU)
pub const SLIDE_WIDTH_16_9: i64 = 12_192_000; // 13.333 in
pub const SLIDE_HEIGHT_16_9: i64 = 6_858_000; // 7.5 in

/// 默认 4:3 幻灯片尺寸 (EMU)
pub const SLIDE_WIDTH_4_3: i64 = 9_144_000;   // 10.0 in
pub const SLIDE_HEIGHT_4_3: i64 = 6_858_000;  // 7.5 in

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideDiagnostic {
    pub part: String,
    pub element: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presentation {
    pub title: String,
    pub author: String,
    pub width_emu: i64,
    pub height_emu: i64,
    pub slides: Vec<Slide>,
    pub diagnostics: Vec<SlideDiagnostic>,
    /// Source-backed text editing retains opaque tables and inherited layout.
    #[doc(hidden)]
    pub preservation: Option<std::sync::Arc<crate::preserve::PreservedPresentation>>,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            title: "Zen Presentation".into(),
            author: "ZenOffice".into(),
            width_emu: SLIDE_WIDTH_16_9,
            height_emu: SLIDE_HEIGHT_16_9,
            slides: Vec::new(),
            diagnostics: Vec::new(),
            preservation: None,
        }
    }
}

impl Presentation {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn add_slide(&mut self, slide: Slide) -> usize {
        self.slides.push(slide);
        self.slides.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slide {
    pub title: Option<String>,
    pub shapes: Vec<Shape>,
}

impl Slide {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(title: impl Into<String>) -> Self {
        let t = title.into();
        let mut slide = Self::default();
        slide.title = Some(t.clone());
        let mut title_shape = Shape::textbox(
            EMU_PER_INCH,
            EMU_PER_INCH / 2,
            SLIDE_WIDTH_16_9 - 2 * EMU_PER_INCH,
            EMU_PER_INCH,
        );
        title_shape.name = "Title 1".into();
        title_shape.add_paragraph(Paragraph::title(t));
        slide.shapes.push(title_shape);
        slide
    }

    pub fn add_shape(&mut self, shape: Shape) {
        self.shapes.push(shape);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub id: u32,
    pub name: String,
    pub kind: ShapeKind,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub fill_color: Option<String>,
    pub text_body: Option<TextBody>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    TextBox,
    Rectangle,
    RoundedRectangle,
    Ellipse,
}

impl Shape {
    pub fn textbox(x: i64, y: i64, width: i64, height: i64) -> Self {
        Self {
            id: 2,
            name: "TextBox".into(),
            kind: ShapeKind::TextBox,
            x,
            y,
            width,
            height,
            fill_color: None,
            text_body: Some(TextBody::default()),
        }
    }

    pub fn rectangle(x: i64, y: i64, width: i64, height: i64, fill_color: impl Into<String>) -> Self {
        Self {
            id: 3,
            name: "Rectangle".into(),
            kind: ShapeKind::Rectangle,
            x,
            y,
            width,
            height,
            fill_color: Some(fill_color.into()),
            text_body: None,
        }
    }

    pub fn rounded_rectangle(x: i64, y: i64, width: i64, height: i64, fill_color: impl Into<String>) -> Self {
        Self {
            id: 4,
            name: "RoundedRectangle".into(),
            kind: ShapeKind::RoundedRectangle,
            x,
            y,
            width,
            height,
            fill_color: Some(fill_color.into()),
            text_body: None,
        }
    }

    pub fn ellipse(x: i64, y: i64, width: i64, height: i64, fill_color: impl Into<String>) -> Self {
        Self {
            id: 5,
            name: "Ellipse".into(),
            kind: ShapeKind::Ellipse,
            x,
            y,
            width,
            height,
            fill_color: Some(fill_color.into()),
            text_body: None,
        }
    }

    pub fn card(
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        fill_color: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        let mut shape = Self::rounded_rectangle(x, y, width, height, fill_color);
        shape.name = "Card".into();
        let mut p_title = Paragraph::title(title);
        for r in &mut p_title.runs {
            r.size_hundredth_pt = Some(2000); // 20pt
            r.color = Some("FFFFFF".into());
        }
        let mut p_body = Paragraph::body(body);
        for r in &mut p_body.runs {
            r.size_hundredth_pt = Some(1400); // 14pt
            r.color = Some("E2E8F0".into());
        }
        shape.text_body = Some(TextBody {
            paragraphs: vec![p_title, p_body],
        });
        shape
    }

    pub fn add_paragraph(&mut self, p: Paragraph) {
        if let Some(tb) = &mut self.text_body {
            tb.paragraphs.push(p);
        } else {
            self.text_body = Some(TextBody { paragraphs: vec![p] });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextBody {
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph {
    pub align: Option<TextAlign>,
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Paragraph {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            align: None,
            runs: vec![Run::text(text)],
        }
    }

    pub fn title(text: impl Into<String>) -> Self {
        Self {
            align: Some(TextAlign::Left),
            runs: vec![Run {
                text: text.into(),
                bold: true,
                italic: false,
                size_hundredth_pt: Some(3600), // 36 pt
                color: Some("1F2328".into()),
            }],
        }
    }

    pub fn body(text: impl Into<String>) -> Self {
        Self {
            align: Some(TextAlign::Left),
            runs: vec![Run {
                text: text.into(),
                bold: false,
                italic: false,
                size_hundredth_pt: Some(1800), // 18 pt
                color: Some("424A53".into()),
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub size_hundredth_pt: Option<u32>,
    pub color: Option<String>,
}

impl Run {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            size_hundredth_pt: Some(1800),
            color: None,
        }
    }

    pub fn styled(text: impl Into<String>, bold: bool, italic: bool, pt: u32, color_hex: Option<&str>) -> Self {
        Self {
            text: text.into(),
            bold,
            italic,
            size_hundredth_pt: Some(pt * 100),
            color: color_hex.map(|s| s.to_string()),
        }
    }
}
