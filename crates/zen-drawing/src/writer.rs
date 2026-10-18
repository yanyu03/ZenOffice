//! DrawingML XML 序列化生成器 (对标 OART.DLL 内部 OpenXML Serializer)

use crate::geometry::Geometry;
use crate::scene::{Fill, Shape, TextBody};

pub struct DrawingMLWriter;

impl DrawingMLWriter {
    /// 将 Shape 节点序列化为符合 ECMA-376 DrawingML 的 `<p:sp>` (PresentationML) 或 `<wps:wsp>`
    pub fn write_shape_element(shape: &Shape, namespace_prefix: &str) -> String {
        let sp_prefix = namespace_prefix; // "p" for Presentation, "wps" for Word

        let mut out = String::new();
        let shape_id = shape.id;
        let escaped_name = escape(&shape.name);
        let x = shape.transform.x;
        let y = shape.transform.y;
        let w = shape.transform.width;
        let h = shape.transform.height;

        let geom_xml = match &shape.geometry {
            Geometry::Preset(p) => {
                let prst = p.xml_name();
                format!("    <a:prstGeom prst=\"{prst}\">\n      <a:avLst/>\n    </a:prstGeom>\n")
            }
            Geometry::CustomPath(d) => {
                let path_cmds = match svg_path_to_drawingml_path(d, w, h) {
                    Ok(p) => p,
                    Err(e) => format!("          <!-- SVG 路径转换错误: {e} -->\n"),
                };
                format!(
                    "    <a:custGeom>\n      <a:avLst/>\n      <a:gdLst/>\n      <a:ahLst/>\n      <a:cxnLst/>\n      <a:rect l=\"0\" t=\"0\" r=\"{w}\" b=\"{h}\"/>\n      <a:pathLst>\n        <a:path w=\"{w}\" h=\"{h}\">\n{path_cmds}        </a:path>\n      </a:pathLst>\n    </a:custGeom>\n"
                )
            }
        };

        out.push_str(&format!(
            r#"<{sp_prefix}:sp>
  <{sp_prefix}:nvSpPr>
    <{sp_prefix}:cNvPr id="{shape_id}" name="{escaped_name}"/>
    <{sp_prefix}:cNvSpPr/>
    <{sp_prefix}:nvPr/>
  </{sp_prefix}:nvSpPr>
  <{sp_prefix}:spPr>
    <a:xfrm>
      <a:off x="{x}" y="{y}"/>
      <a:ext cx="{w}" cy="{h}"/>
    </a:xfrm>
{geom_xml}"#
        ));

        // 填充
        match &shape.fill {
            Fill::None => {
                out.push_str("    <a:noFill/>\n");
            }
            Fill::Solid(color) => {
                let hex = color.to_hex_string();
                out.push_str(&format!(
                    "    <a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill>\n"
                ));
            }
        }

        // 描边
        if let Some(stroke) = &shape.stroke {
            let stroke_hex = stroke.color.to_hex_string();
            let w_emu = stroke.width_emu;
            let prst_dash = if stroke.dashed { "<a:prstDash val=\"dash\"/>" } else { "" };
            out.push_str(&format!(
                "    <a:ln w=\"{w_emu}\"><a:solidFill><a:srgbClr val=\"{stroke_hex}\"/></a:solidFill>{prst_dash}</a:ln>\n"
            ));
        }

        out.push_str(&format!("  </{sp_prefix}:spPr>\n"));

        // 文本体 (txBody)
        if let Some(tb) = &shape.text_body {
            out.push_str(&Self::write_text_body(tb, sp_prefix));
        }

        out.push_str(&format!("</{sp_prefix}:sp>\n"));
        out
    }

    fn write_text_body(tb: &TextBody, sp_prefix: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            r#"  <{sp_prefix}:txBody>
    <a:bodyPr rtlCol="0" anchor="ctr"/>
    <a:lstStyle/>
"#
        ));

        for p in &tb.paragraphs {
            let align_attr = match p.align {
                crate::scene::TextAlign::Left => "algn=\"l\"",
                crate::scene::TextAlign::Center => "algn=\"ctr\"",
                crate::scene::TextAlign::Right => "algn=\"r\"",
                crate::scene::TextAlign::Justify => "algn=\"just\"",
            };
            out.push_str(&format!("    <a:p>\n      <a:pPr {align_attr}/>\n"));

            for r in &p.runs {
                let text = escape(&r.text);
                let sz = r.font_size_hundredth_pt;
                let b = if r.bold { " b=\"1\"" } else { "" };
                let i = if r.italic { " i=\"1\"" } else { "" };
                let color_xml = if let Some(c) = &r.color {
                    let hex = c.to_hex_string();
                    format!("<a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill>")
                } else {
                    "<a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>".to_string()
                };

                out.push_str(&format!(
                    r#"      <a:r>
        <a:rPr lang="en-US" sz="{sz}"{b}{i} dirty="0">
          {color_xml}
        </a:rPr>
        <a:t>{text}</a:t>
      </a:r>
"#
                ));
            }

            out.push_str("    </a:p>\n");
        }

        out.push_str(&format!("  </{sp_prefix}:txBody>\n"));
        out
    }
}

fn escape(s: &str) -> String {
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

#[derive(Debug, PartialEq, Clone)]
enum SvgToken {
    Cmd(char),
    Num(f64),
}

fn tokenize_svg_path(d: &str) -> crate::Result<Vec<SvgToken>> {
    let chars: Vec<char> = d.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }

        if c.is_ascii_alphabetic() {
            tokens.push(SvgToken::Cmd(c));
            i += 1;
            continue;
        }

        if c.is_ascii_digit() || c == '+' || c == '-' || c == '.' {
            let start = i;
            let mut has_digit = false;
            let mut has_dot = false;
            let mut has_exp = false;

            if c == '+' || c == '-' {
                i += 1;
            }

            while i < chars.len() {
                let ch = chars[i];
                if ch.is_ascii_digit() {
                    has_digit = true;
                    i += 1;
                } else if ch == '.' && !has_dot && !has_exp {
                    has_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && has_digit && !has_exp {
                    has_exp = true;
                    i += 1;
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    if i >= chars.len() || !chars[i].is_ascii_digit() {
                        return Err(crate::Error::InvalidGeometry(format!(
                            "科学计数法指数缺少数字: {:?}",
                            &d[start..i.min(chars.len())]
                        )));
                    }
                } else {
                    break;
                }
            }

            if !has_digit {
                return Err(crate::Error::InvalidGeometry(format!(
                    "无效的数字字面量: {:?}",
                    &d[start..i]
                )));
            }

            let num_str: String = chars[start..i].iter().collect();
            let val = num_str.parse::<f64>().map_err(|e| {
                crate::Error::InvalidGeometry(format!("无法解析数字 '{num_str}': {e}"))
            })?;
            tokens.push(SvgToken::Num(val));
            continue;
        }

        return Err(crate::Error::InvalidGeometry(format!(
            "SVG 路径中包含非法字符: '{c}'"
        )));
    }

    if tokens.is_empty() {
        return Err(crate::Error::InvalidGeometry("SVG 路径数据为空".into()));
    }
    Ok(tokens)
}

pub fn svg_path_to_drawingml_path(d: &str, _w: i64, _h: i64) -> crate::Result<String> {
    let tokens = tokenize_svg_path(d)?;
    let mut out = String::new();
    let mut curr_x = 0i64;
    let mut curr_y = 0i64;
    let mut subpath_start = (0i64, 0i64);
    let mut last_cp_x = 0i64;
    let mut last_cp_y = 0i64;
    let mut last_cmd = ' ';

    let mut idx = 0;
    while idx < tokens.len() {
        let cmd = match &tokens[idx] {
            SvgToken::Cmd(c) => *c,
            SvgToken::Num(n) => {
                return Err(crate::Error::InvalidGeometry(format!(
                    "预期 SVG 指令字母，实际遇到孤立数字: {n}"
                )));
            }
        };
        idx += 1;

        match cmd {
            'M' | 'm' => {
                let mut first = true;
                while idx < tokens.len() && matches!(tokens[idx], SvgToken::Num(_)) {
                    let x_val = match tokens[idx] {
                        SvgToken::Num(n) => n,
                        _ => unreachable!(),
                    };
                    if idx + 1 >= tokens.len() || !matches!(tokens[idx + 1], SvgToken::Num(_)) {
                        return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少 Y 坐标")));
                    }
                    let y_val = match tokens[idx + 1] {
                        SvgToken::Num(n) => n,
                        _ => unreachable!(),
                    };
                    idx += 2;

                    let (nx, ny) = if cmd == 'm' {
                        (curr_x + x_val.round() as i64, curr_y + y_val.round() as i64)
                    } else {
                        (x_val.round() as i64, y_val.round() as i64)
                    };
                    curr_x = nx;
                    curr_y = ny;

                    if first {
                        subpath_start = (curr_x, curr_y);
                        out.push_str(&format!("          <a:moveTo><a:pt x=\"{curr_x}\" y=\"{curr_y}\"/></a:moveTo>\n"));
                        first = false;
                    } else {
                        // SVG 规范：M/m 后的多组坐标隐式按 L/l 处理
                        out.push_str(&format!("          <a:lnTo><a:pt x=\"{curr_x}\" y=\"{curr_y}\"/></a:lnTo>\n"));
                    }
                }
                if first {
                    return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少坐标参数")));
                }
                last_cmd = cmd;
            }
            'L' | 'l' => {
                let mut count = 0;
                while idx < tokens.len() && matches!(tokens[idx], SvgToken::Num(_)) {
                    let x_val = match tokens[idx] {
                        SvgToken::Num(n) => n,
                        _ => unreachable!(),
                    };
                    if idx + 1 >= tokens.len() || !matches!(tokens[idx + 1], SvgToken::Num(_)) {
                        return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少 Y 坐标")));
                    }
                    let y_val = match tokens[idx + 1] {
                        SvgToken::Num(n) => n,
                        _ => unreachable!(),
                    };
                    idx += 2;
                    let (nx, ny) = if cmd == 'l' {
                        (curr_x + x_val.round() as i64, curr_y + y_val.round() as i64)
                    } else {
                        (x_val.round() as i64, y_val.round() as i64)
                    };
                    curr_x = nx;
                    curr_y = ny;
                    out.push_str(&format!("          <a:lnTo><a:pt x=\"{curr_x}\" y=\"{curr_y}\"/></a:lnTo>\n"));
                    count += 1;
                }
                if count == 0 {
                    return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少坐标参数")));
                }
                last_cmd = cmd;
            }
            'H' | 'h' => {
                let mut count = 0;
                while idx < tokens.len() && matches!(tokens[idx], SvgToken::Num(_)) {
                    let x_val = match tokens[idx] {
                        SvgToken::Num(n) => n,
                        _ => unreachable!(),
                    };
                    idx += 1;
                    curr_x = if cmd == 'h' {
                        curr_x + x_val.round() as i64
                    } else {
                        x_val.round() as i64
                    };
                    out.push_str(&format!("          <a:lnTo><a:pt x=\"{curr_x}\" y=\"{curr_y}\"/></a:lnTo>\n"));
                    count += 1;
                }
                if count == 0 {
                    return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少 X 坐标参数")));
                }
                last_cmd = cmd;
            }
            'V' | 'v' => {
                let mut count = 0;
                while idx < tokens.len() && matches!(tokens[idx], SvgToken::Num(_)) {
                    let y_val = match tokens[idx] {
                        SvgToken::Num(n) => n,
                        _ => unreachable!(),
                    };
                    idx += 1;
                    curr_y = if cmd == 'v' {
                        curr_y + y_val.round() as i64
                    } else {
                        y_val.round() as i64
                    };
                    out.push_str(&format!("          <a:lnTo><a:pt x=\"{curr_x}\" y=\"{curr_y}\"/></a:lnTo>\n"));
                    count += 1;
                }
                if count == 0 {
                    return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少 Y 坐标参数")));
                }
                last_cmd = cmd;
            }
            'C' | 'c' => {
                let mut count = 0;
                while idx < tokens.len() && matches!(tokens[idx], SvgToken::Num(_)) {
                    if idx + 5 >= tokens.len() {
                        return Err(crate::Error::InvalidGeometry(format!("{cmd} 贝塞尔曲线缺少完整的 6 个坐标参数")));
                    }
                    let mut vals = [0.0; 6];
                    for k in 0..6 {
                        vals[k] = match tokens[idx + k] {
                            SvgToken::Num(n) => n,
                            _ => return Err(crate::Error::InvalidGeometry(format!("{cmd} 参数预期数字"))),
                        };
                    }
                    idx += 6;
                    let (x1, y1, x2, y2, x, y) = (vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]);
                    let (p1x, p1y, p2x, p2y, px, py) = if cmd == 'c' {
                        (
                            curr_x + x1.round() as i64, curr_y + y1.round() as i64,
                            curr_x + x2.round() as i64, curr_y + y2.round() as i64,
                            curr_x + x.round() as i64, curr_y + y.round() as i64,
                        )
                    } else {
                        (x1.round() as i64, y1.round() as i64, x2.round() as i64, y2.round() as i64, x.round() as i64, y.round() as i64)
                    };
                    last_cp_x = p2x;
                    last_cp_y = p2y;
                    curr_x = px;
                    curr_y = py;
                    out.push_str(&format!("          <a:cubicBezTo><a:pt x=\"{p1x}\" y=\"{p1y}\"/><a:pt x=\"{p2x}\" y=\"{p2y}\"/><a:pt x=\"{px}\" y=\"{py}\"/></a:cubicBezTo>\n"));
                    count += 1;
                }
                if count == 0 {
                    return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少坐标参数")));
                }
                last_cmd = cmd;
            }
            'S' | 's' => {
                let mut count = 0;
                while idx < tokens.len() && matches!(tokens[idx], SvgToken::Num(_)) {
                    if idx + 3 >= tokens.len() {
                        return Err(crate::Error::InvalidGeometry(format!("{cmd} 平滑贝塞尔曲线缺少完整的 4 个坐标参数")));
                    }
                    let mut vals = [0.0; 4];
                    for k in 0..4 {
                        vals[k] = match tokens[idx + k] {
                            SvgToken::Num(n) => n,
                            _ => return Err(crate::Error::InvalidGeometry(format!("{cmd} 参数预期数字"))),
                        };
                    }
                    idx += 4;
                    let (x2, y2, x, y) = (vals[0], vals[1], vals[2], vals[3]);
                    let (p2x, p2y, px, py) = if cmd == 's' {
                        (curr_x + x2.round() as i64, curr_y + y2.round() as i64, curr_x + x.round() as i64, curr_y + y.round() as i64)
                    } else {
                        (x2.round() as i64, y2.round() as i64, x.round() as i64, y.round() as i64)
                    };

                    let (p1x, p1y) = if matches!(last_cmd, 'C' | 'c' | 'S' | 's') {
                        (2 * curr_x - last_cp_x, 2 * curr_y - last_cp_y)
                    } else {
                        (curr_x, curr_y)
                    };

                    last_cp_x = p2x;
                    last_cp_y = p2y;
                    curr_x = px;
                    curr_y = py;
                    out.push_str(&format!("          <a:cubicBezTo><a:pt x=\"{p1x}\" y=\"{p1y}\"/><a:pt x=\"{p2x}\" y=\"{p2y}\"/><a:pt x=\"{px}\" y=\"{py}\"/></a:cubicBezTo>\n"));
                    count += 1;
                }
                if count == 0 {
                    return Err(crate::Error::InvalidGeometry(format!("{cmd} 命令缺少坐标参数")));
                }
                last_cmd = cmd;
            }
            'Z' | 'z' => {
                out.push_str("          <a:close/>\n");
                curr_x = subpath_start.0;
                curr_y = subpath_start.1;
                last_cmd = cmd;
            }
            other => {
                return Err(crate::Error::UnsupportedGeometry(format!(
                    "DrawingML 转换器暂不支持的 SVG 指令: '{other}'"
                )));
            }
        }
    }

    if out.is_empty() {
        return Err(crate::Error::InvalidGeometry("SVG 路径未生成任何有效的 DrawingML 指令".into()));
    }

    Ok(out)
}

