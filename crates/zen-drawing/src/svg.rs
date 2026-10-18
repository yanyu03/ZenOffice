//! W3C SVG 导出器 (对标 MSOSVG.DLL 渲染通道)
//!
//! 将 DrawingNode 场景树直接栅格化/转换为高保真标准 SVG 矢量图形。

use crate::geometry::{Geometry, PresetGeometry};
use crate::scene::{Fill, Shape};
use crate::EMU_PER_PT;

pub struct SvgExporter;

impl SvgExporter {
    /// 将形状集合导出为完整的 SVG 文本
    pub fn export_svg(shapes: &[Shape], width_emu: i64, height_emu: i64) -> String {
        let width_pt = width_emu as f64 / EMU_PER_PT as f64;
        let height_pt = height_emu as f64 / EMU_PER_PT as f64;

        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width_pt:.2} {height_pt:.2}" width="{width_pt:.2}pt" height="{height_pt:.2}pt">
  <defs>
    <style>
      .zen-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; }}
    </style>
  </defs>
"#
        );

        for shape in shapes {
            svg.push_str(&Self::shape_to_svg(shape));
        }

        svg.push_str("</svg>\n");
        svg
    }

    fn shape_to_svg(shape: &Shape) -> String {
        let x_pt = shape.transform.x as f64 / EMU_PER_PT as f64;
        let y_pt = shape.transform.y as f64 / EMU_PER_PT as f64;
        let w_pt = shape.transform.width as f64 / EMU_PER_PT as f64;
        let h_pt = shape.transform.height as f64 / EMU_PER_PT as f64;

        let fill_attr = match &shape.fill {
            Fill::None => "fill=\"none\"".to_string(),
            Fill::Solid(color) => format!("fill=\"{}\"", color.to_svg_string()),
        };

        let stroke_attr = if let Some(stroke) = &shape.stroke {
            let stroke_pt = (stroke.width_emu as f64 / EMU_PER_PT as f64).max(0.5);
            let dash_attr = if stroke.dashed { " stroke-dasharray=\"4 2\"" } else { "" };
            format!(" stroke=\"{}\" stroke-width=\"{stroke_pt:.2}\"{dash_attr}", stroke.color.to_svg_string())
        } else {
            "".to_string()
        };

        let mut out = String::new();
        match &shape.geometry {
            Geometry::Preset(PresetGeometry::Rectangle) => {
                out.push_str(&format!(
                    "  <rect x=\"{x_pt:.2}\" y=\"{y_pt:.2}\" width=\"{w_pt:.2}\" height=\"{h_pt:.2}\" {fill_attr}{stroke_attr}/>\n"
                ));
            }
            Geometry::Preset(PresetGeometry::RoundedRectangle) => {
                let rx = 8.0_f64.min(w_pt / 4.0);
                out.push_str(&format!(
                    "  <rect x=\"{x_pt:.2}\" y=\"{y_pt:.2}\" width=\"{w_pt:.2}\" height=\"{h_pt:.2}\" rx=\"{rx:.2}\" {fill_attr}{stroke_attr}/>\n"
                ));
            }
            Geometry::Preset(PresetGeometry::Ellipse) => {
                let cx = x_pt + w_pt / 2.0;
                let cy = y_pt + h_pt / 2.0;
                let rx = w_pt / 2.0;
                let ry = h_pt / 2.0;
                out.push_str(&format!(
                    "  <ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" {fill_attr}{stroke_attr}/>\n"
                ));
            }
            Geometry::Preset(PresetGeometry::Line) => {
                out.push_str(&format!(
                    "  <line x1=\"{x_pt:.2}\" y1=\"{y_pt:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {stroke_attr}/>\n",
                    x_pt + w_pt,
                    y_pt + h_pt
                ));
            }
            Geometry::Preset(PresetGeometry::Triangle) => {
                let p1 = (x_pt + w_pt / 2.0, y_pt);
                let p2 = (x_pt + w_pt, y_pt + h_pt);
                let p3 = (x_pt, y_pt + h_pt);
                out.push_str(&format!(
                    "  <polygon points=\"{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\" {fill_attr}{stroke_attr}/>\n",
                    p1.0, p1.1, p2.0, p2.1, p3.0, p3.1
                ));
            }
            Geometry::Preset(PresetGeometry::Diamond) => {
                let cx = x_pt + w_pt / 2.0;
                let cy = y_pt + h_pt / 2.0;
                out.push_str(&format!(
                    "  <polygon points=\"{cx:.2},{y_pt:.2} {:.2},{cy:.2} {cx:.2},{:.2} {x_pt:.2},{cy:.2}\" {fill_attr}{stroke_attr}/>\n",
                    x_pt + w_pt,
                    y_pt + h_pt
                ));
            }
            Geometry::Preset(PresetGeometry::RightArrow) => {
                let x0 = x_pt;
                let x1 = x_pt + w_pt * 0.6;
                let x2 = x_pt + w_pt;
                let y0 = y_pt;
                let y1 = y_pt + h_pt * 0.25;
                let ym = y_pt + h_pt * 0.5;
                let y2 = y_pt + h_pt * 0.75;
                let y3 = y_pt + h_pt;
                out.push_str(&format!(
                    "  <polygon points=\"{x0:.2},{y1:.2} {x1:.2},{y1:.2} {x1:.2},{y0:.2} {x2:.2},{ym:.2} {x1:.2},{y3:.2} {x1:.2},{y2:.2} {x0:.2},{y2:.2}\" {fill_attr}{stroke_attr}/>\n"
                ));
            }
            Geometry::Preset(PresetGeometry::LeftArrow) => {
                let x0 = x_pt;
                let x1 = x_pt + w_pt * 0.4;
                let x2 = x_pt + w_pt;
                let y0 = y_pt;
                let y1 = y_pt + h_pt * 0.25;
                let ym = y_pt + h_pt * 0.5;
                let y2 = y_pt + h_pt * 0.75;
                let y3 = y_pt + h_pt;
                out.push_str(&format!(
                    "  <polygon points=\"{x1:.2},{y0:.2} {x0:.2},{ym:.2} {x1:.2},{y3:.2} {x1:.2},{y2:.2} {x2:.2},{y2:.2} {x2:.2},{y1:.2} {x1:.2},{y1:.2}\" {fill_attr}{stroke_attr}/>\n"
                ));
            }
            Geometry::Preset(PresetGeometry::Star5) => {
                let cx = x_pt + w_pt / 2.0;
                let cy = y_pt + h_pt / 2.0;
                let r_outer = w_pt.min(h_pt) / 2.0;
                let r_inner = r_outer * 0.382;
                let mut pts = Vec::new();
                for i in 0..10 {
                    let angle = std::f64::consts::PI / 5.0 * (i as f64) - std::f64::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { r_outer } else { r_inner };
                    pts.push(format!("{:.2},{:.2}", cx + r * angle.cos(), cy + r * angle.sin()));
                }
                out.push_str(&format!(
                    "  <polygon points=\"{}\" {fill_attr}{stroke_attr}/>\n",
                    pts.join(" ")
                ));
            }
            Geometry::CustomPath(d) => {
                out.push_str(&format!(
                    "  <path d=\"{}\" {fill_attr}{stroke_attr}/>\n",
                    escape_xml(d)
                ));
            }
        }

        // 渲染内部文本
        if let Some(tb) = &shape.text_body {
            let mut curr_y = y_pt + 20.0;
            for p in &tb.paragraphs {
                let text_anchor = match p.align {
                    crate::scene::TextAlign::Left => "start",
                    crate::scene::TextAlign::Center => "middle",
                    crate::scene::TextAlign::Right => "end",
                    crate::scene::TextAlign::Justify => "start",
                };
                let text_x = match p.align {
                    crate::scene::TextAlign::Left => x_pt + 10.0,
                    crate::scene::TextAlign::Center => x_pt + w_pt / 2.0,
                    crate::scene::TextAlign::Right => x_pt + w_pt - 10.0,
                    crate::scene::TextAlign::Justify => x_pt + 10.0,
                };

                for r in &p.runs {
                    let sz_pt = r.font_size_hundredth_pt as f64 / 100.0;
                    let font_weight = if r.bold { "bold" } else { "normal" };
                    let font_style = if r.italic { "italic" } else { "normal" };
                    let fill_color = r.color.as_ref().map(|c| c.to_svg_string()).unwrap_or_else(|| "#FFFFFF".into());

                    out.push_str(&format!(
                        "  <text x=\"{text_x:.2}\" y=\"{curr_y:.2}\" text-anchor=\"{text_anchor}\" font-size=\"{sz_pt:.1}pt\" font-weight=\"{font_weight}\" font-style=\"{font_style}\" fill=\"{fill_color}\" class=\"zen-text\">{}</text>\n",
                        escape_xml(&r.text)
                    ));
                    curr_y += sz_pt * 1.3;
                }
            }
        }

        out
    }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}
