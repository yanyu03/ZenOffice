//! Independent approximate line formatting; not an implementation of MSLS70.
//!
//! 职责：
//! 1. 单段落文本测量、字符度量与断词断行 (Break Candidate Selection)
//! 2. 悬挂缩进、首行缩进与左右段落缩进计算
//! 3. 行内对齐计算（左对齐、居中、右对齐、两端对齐）
//! 4. 纯几何坐标与基线 (Baseline)、上凸高 (Ascent)、下延深 (Descent) 确定

use crate::model::{Align, BreakKind, LineSpacing, Paragraph, Run, RunContent, RunProps};
use super::font_metrics::{self, FontLineHeightProvider, LineHeightTrace};

/// Pagination consumes hard breaks separately from measured text lines.
#[derive(Debug, Clone)]
pub enum ParagraphFlow {
    Lines(Vec<LayoutLine>),
    Break(BreakKind),
}

/// 布局行中的文本运行块
#[derive(Debug, Clone)]
pub struct LayoutRun {
    pub text: String,
    pub x_twips: i32,
    pub y_twips: i32,
    pub width_twips: u32,
    pub height_twips: u32,
    pub props: RunProps,
}

/// 格式化后的排版行
#[derive(Debug, Clone)]
pub struct LayoutLine {
    pub runs: Vec<LayoutRun>,
    pub width_twips: u32,
    pub height_twips: u32,
    /// Required page-bottom footprint. height_twips remains the full advance.
    /// Only known font metrics with automatic spacing can reduce this value.
    pub fit_height_twips: u32,
    pub ascent_twips: u32,
    pub descent_twips: u32,
    pub baseline_y_twips: i32,
}

pub struct LineServices;

impl LineServices {
    /// 估算字符宽度 (单位: Twips)
    ///
    /// Heuristic preview metrics, without real font shaping or Office parity:
    /// - 默认西文字符宽度约等于字号的 0.52 倍
    /// - larger Unicode code points are approximated as one em
    /// 1 point = 20 twips; font_size_half_points / 2 = points.
    pub fn estimate_char_width_twips(c: char, sz_half_pts: u32) -> u32 {
        let pts = (sz_half_pts as f64) / 2.0;
        let em_twips = (pts * 20.0).round() as u32;

        if (c as u32) > 0x2E80 {
            // 中日韩全角字符与表意文字：1 em
            em_twips
        } else if c == ' ' {
            // 空格：约 0.28 em
            (em_twips as f64 * 0.28).max(1.0) as u32
        } else if c == 'i' || c == 'l' || c == '.' || c == ',' || c == ':' || c == ';' || c == '!' {
            // 窄字符
            (em_twips as f64 * 0.25).max(1.0) as u32
        } else if c == 'm' || c == 'w' || c == 'M' || c == 'W' || c == '@' {
            // 宽字符
            (em_twips as f64 * 0.85) as u32
        } else {
            // 普通半角英数：约 0.52 em
            (em_twips as f64 * 0.52).max(1.0) as u32
        }
    }

    /// 估算文本运行块总宽度
    pub fn measure_text_twips(text: &str, sz_half_pts: u32) -> u32 {
        text.chars()
            .map(|c| Self::estimate_char_width_twips(c, sz_half_pts))
            .fold(0, u32::saturating_add)
    }

    pub fn format_paragraph_flow(para: &Paragraph, width: u32) -> Vec<ParagraphFlow> {
        Self::format_paragraph_flow_impl(para, width, None, &mut Vec::new(), 420)
    }

    /// Hard-break-aware opt-in measurement. Trace entries retain lookup order.
    pub fn format_paragraph_flow_with_metrics(
        para: &Paragraph, width: u32, provider: &dyn FontLineHeightProvider,
    ) -> (Vec<ParagraphFlow>, Vec<LineHeightTrace>) {
        let mut traces = Vec::new();
        let flow = Self::format_paragraph_flow_impl(para, width, Some(provider), &mut traces, 420);
        (flow, traces)
    }

    /// Measure a style-resolved paragraph using the same document tab settings
    /// as checked document layout. Legacy standalone entry points use 420 twips.
    pub fn format_paragraph_flow_with_settings_and_metrics(
        para: &Paragraph, width: u32, settings: &crate::model::DocumentSettings,
        provider: &dyn FontLineHeightProvider,
    ) -> crate::Result<(Vec<ParagraphFlow>, Vec<LineHeightTrace>)> {
        settings.validate()?;
        if settings.default_tab_stop_twips == 0 {
            return Err(crate::WordError::UnsupportedEdit(
                "preview zero defaultTabStop is not implemented".into(),
            ));
        }
        let mut traces = Vec::new();
        let flow = Self::format_paragraph_flow_impl(
            para, width, Some(provider), &mut traces, settings.default_tab_stop_twips,
        );
        Ok((flow, traces))
    }

    pub(crate) fn format_paragraph_flow_impl(
        para: &Paragraph, width: u32, provider: Option<&dyn FontLineHeightProvider>,
        traces: &mut Vec<LineHeightTrace>, default_tab_twips: u32,
    ) -> Vec<ParagraphFlow> {
        let mut chunk = Paragraph {
            props: para.props.clone(),
            runs: Vec::new(),
        };
        chunk.props.section_end = None;
        let mut result = Vec::new();
        let mut trailing_line = false;
        let mut empty_run_props = para
            .runs
            .first()
            .map(|r| r.props.clone())
            .unwrap_or_default();
        for run in &para.runs {
            empty_run_props = run.props.clone();
            for content in &run.contents {
                if let RunContent::Break(kind) = content {
                    if !chunk.runs.is_empty() || *kind == BreakKind::Line {
                        if chunk.runs.is_empty() {
                            chunk.runs.push(Run {
                                props: run.props.clone(),
                                contents: Vec::new(),
                            });
                        }
                        result.push(ParagraphFlow::Lines(Self::format_paragraph_impl(&chunk, width, provider, traces, default_tab_twips)));
                        chunk.runs.clear();
                    }
                    if *kind != BreakKind::Line {
                        result.push(ParagraphFlow::Break(*kind));
                    }
                    trailing_line = *kind == BreakKind::Line;
                    chunk.props.ind_first_line_twips = Some(0);
                } else {
                    chunk.runs.push(Run {
                        props: run.props.clone(),
                        contents: vec![content.clone()],
                    });
                    trailing_line = false;
                }
            }
        }
        if !chunk.runs.is_empty() || result.is_empty() || trailing_line {
            if chunk.runs.is_empty() {
                chunk.runs.push(Run {
                    props: empty_run_props,
                    contents: Vec::new(),
                });
            }
            result.push(ParagraphFlow::Lines(Self::format_paragraph_impl(&chunk, width, provider, traces, default_tab_twips)));
        }
        result
    }

    /// 对单个段落执行多行折叠排版 (Line Formatting & Breaking)
    ///
    /// `available_width_twips`: 可用正文宽度 (版心宽)
    /// For hard line/page/column breaks use format_paragraph_flow; this primitive
    /// formats only a text chunk and does not carry pagination events.
    pub fn format_paragraph(para: &Paragraph, available_width_twips: u32) -> Vec<LayoutLine> {
        Self::format_paragraph_impl(para, available_width_twips, None, &mut Vec::new(), 420)
    }

    /// Opt-in height measurement for a style-resolved text chunk. Widths and
    /// baselines remain heuristic. Traces list each run-height lookup in order;
    /// None sources identify fallback, including with Exact line spacing.
    /// Use format_paragraph_flow_with_metrics to retain hard-break events.
    pub fn format_paragraph_with_metrics(
        para: &Paragraph, available_width_twips: u32, provider: &dyn FontLineHeightProvider,
    ) -> (Vec<LayoutLine>, Vec<LineHeightTrace>) {
        let mut traces = Vec::new();
        let lines = Self::format_paragraph_impl(para, available_width_twips, Some(provider), &mut traces, 420);
        (lines, traces)
    }

    fn format_paragraph_impl(
        para: &Paragraph, available_width_twips: u32, provider: Option<&dyn FontLineHeightProvider>,
        traces: &mut Vec<LineHeightTrace>, default_tab_twips: u32,
    ) -> Vec<LayoutLine> {
        let mut lines = Vec::new();
        if available_width_twips == 0 {
            return lines;
        }

        // 段落缩进
        let left_indent = para.props.ind_left_twips.unwrap_or(0);
        let right_indent = para.props.ind_right_twips.unwrap_or(0);
        let first_line_indent = para.props.ind_first_line_twips.unwrap_or(0);

        // 提取所有文本 run
        let mut tokens: Vec<(String, RunProps)> = Vec::new();
        for run in &para.runs {
            for content in &run.contents {
                match content {
                    RunContent::Text(text) => {
                        let text = run.props.display_text(text);
                        if text.is_empty() {
                            continue;
                        }
                        let mut current_word = String::new();
                        for c in text.chars() {
                            if (c as u32) > 0x2E80 {
                                // CJK 字符随时可断行
                                if !current_word.is_empty() {
                                    tokens.push((
                                        std::mem::take(&mut current_word),
                                        run.props.clone(),
                                    ));
                                }
                                tokens.push((c.to_string(), run.props.clone()));
                            } else if c.is_whitespace() {
                                current_word.push(c);
                                tokens.push((std::mem::take(&mut current_word), run.props.clone()));
                            } else {
                                current_word.push(c);
                            }
                        }
                        if !current_word.is_empty() {
                            tokens.push((current_word, run.props.clone()));
                        }
                    }
                    RunContent::Tab => {
                        tokens.push(("\t".to_string(), run.props.clone()));
                    }
                    RunContent::Break(_) => {
                        // 换行/分节处理
                    }
                }
            }
        }

        if tokens.is_empty() {
            // 空段落占位行
            let default_sz = para
                .runs
                .iter()
                .filter_map(|r| r.props.size_half_points)
                .max()
                .unwrap_or(21);
            let trace_start = traces.len();
            let natural = if let Some(provider) = provider {
                para.runs.iter().map(|r| font_metrics::natural_height(&r.props, "", provider, traces))
                    .max().unwrap_or_else(|| font_metrics::natural_height(&RunProps::default(), "", provider, traces))
            } else {
                (default_sz as f64 * 10.0 * 1.2).round() as u32
            };
            let h = Self::line_height(natural, para.props.line_spacing);
            let known = provider.is_some() && traces[trace_start..].iter().all(|t| t.source.is_some());
            let fit = Self::fit_height(natural, h, para.props.line_spacing, known);
            let ascent = ((h as f64 * 0.8) as u32).min(fit);
            lines.push(LayoutLine {
                runs: Vec::new(),
                width_twips: 0,
                height_twips: h,
                fit_height_twips: fit,
                ascent_twips: ascent,
                descent_twips: if fit < h { h - ascent } else { (h as f64 * 0.2) as u32 },
                baseline_y_twips: ascent as i32,
            });
            return lines;
        }

        let mut token_idx = 0;
        let mut token_byte_offset = 0;
        let mut remaining_widths: Vec<u64> = tokens
            .iter()
            .map(|(text, props)| {
                text.chars()
                    .map(|c| {
                        u64::from(Self::estimate_char_width_twips(
                            c,
                            props.size_half_points.unwrap_or(21),
                        ))
                    })
                    .sum()
            })
            .collect();
        let mut is_first_line = true;

        while token_idx < tokens.len() {
            // 当前行的净宽度预算
            let indent_first = if is_first_line { first_line_indent } else { 0 };
            let line_left_margin = left_indent.saturating_add(indent_first).max(0);
            let line_budget = available_width_twips
                .saturating_sub(line_left_margin as u32)
                .saturating_sub(right_indent.max(0) as u32);
            let effective_budget = line_budget.max(1);

            let mut line_tokens = Vec::new();
            let mut current_line_w: u32 = 0;

            while token_idx < tokens.len() {
                let (whole_text, props) = &tokens[token_idx];
                let text = &whole_text[token_byte_offset..];
                let sz = props.size_half_points.unwrap_or(21);
                // Default stops are positions on the body/cell text-area grid,
                // independent of the glyph width used for ordinary text.
                if text == "\t" {
                    debug_assert!(default_tab_twips > 0);
                    let interval = u64::from(default_tab_twips);
                    let cursor = u64::from(line_left_margin as u32) + u64::from(current_line_w);
                    let advance = interval - cursor % interval;
                    // Native boundary probes: a tab that lands exactly on the
                    // right edge travels with following text to the next line.
                    // Consecutive tabs can still break between tab tokens.
                    let reaches_edge_with_text = u64::from(current_line_w) + advance == u64::from(effective_budget)
                        && tokens.get(token_idx + 1).is_some_and(|(next, _)| next != "\t");
                    if !line_tokens.is_empty()
                        && (u64::from(current_line_w) + advance > u64::from(effective_budget)
                            || reaches_edge_with_text)
                    {
                        break;
                    }
                    // Consume even an over-wide leading tab so layout always
                    // progresses. Native right-edge behavior remains unaccepted.
                    let w = advance.min(u64::from(effective_budget - current_line_w)) as u32;
                    line_tokens.push((text.to_owned(), props.clone(), w));
                    current_line_w += w;
                    token_idx += 1;
                    token_byte_offset = 0;
                    continue;
                }
                let w = remaining_widths[token_idx];

                if !line_tokens.is_empty()
                    && (u64::from(current_line_w) + w > u64::from(effective_budget))
                {
                    // 超出预算，在此断行
                    break;
                }

                if w > u64::from(effective_budget) {
                    let mut prefix = String::new();
                    let mut prefix_width = 0u32;
                    for c in text.chars() {
                        let cw = Self::estimate_char_width_twips(c, sz);
                        if !prefix.is_empty() && prefix_width.saturating_add(cw) > effective_budget
                        {
                            break;
                        }
                        prefix.push(c);
                        prefix_width = prefix_width.saturating_add(cw);
                    }
                    token_byte_offset += prefix.len();
                    line_tokens.push((prefix, props.clone(), prefix_width));
                    current_line_w = prefix_width;
                    remaining_widths[token_idx] =
                        remaining_widths[token_idx].saturating_sub(u64::from(prefix_width));
                    if token_byte_offset == whole_text.len() {
                        token_idx += 1;
                        token_byte_offset = 0;
                    }
                    break;
                }

                current_line_w += w as u32;
                line_tokens.push((text.to_owned(), props.clone(), w as u32));
                token_idx += 1;
                token_byte_offset = 0;
            }

            // 计算该行高度与基线
            let mut max_sz: u32 = 1;
            for (_, props, _) in &line_tokens {
                let sz = props.size_half_points.unwrap_or(21);
                if sz > max_sz {
                    max_sz = sz;
                }
            }

            // 行高与基线（按 1.2 倍单倍行距计算）
            let pts = (max_sz as f64) / 2.0;
            let trace_start = traces.len();
            let natural = if let Some(provider) = provider {
                line_tokens.iter().map(|(text, props, _)| font_metrics::natural_height(props, text, provider, traces)).max().unwrap_or(1)
            } else {
                (pts * 20.0 * 1.2).round() as u32
            };
            let line_h = Self::line_height(natural, para.props.line_spacing);
            let known = provider.is_some() && traces[trace_start..].iter().all(|t| t.source.is_some());
            let fit = Self::fit_height(natural, line_h, para.props.line_spacing, known);
            let ascent = (pts * 20.0 * 0.82).round() as u32;
            let ascent = if provider.is_some() { ascent.min(fit) } else { ascent };
            let descent = line_h.saturating_sub(ascent);

            // 对齐计算 (Align)
            let align = para.props.align.unwrap_or(Align::Left);
            let spare_w = effective_budget.saturating_sub(current_line_w);
            let start_x: i32 = match align {
                Align::Left | Align::Justify => line_left_margin,
                Align::Center => line_left_margin + (spare_w as i32 / 2),
                Align::Right => line_left_margin + spare_w as i32,
            };

            // 放置 Run
            let mut curr_x = start_x;
            let mut layout_runs = Vec::new();
            for (text, props, run_w) in line_tokens {
                layout_runs.push(LayoutRun {
                    text,
                    x_twips: curr_x,
                    y_twips: 0,
                    width_twips: run_w,
                    height_twips: line_h,
                    props,
                });
                curr_x += run_w as i32;
            }

            lines.push(LayoutLine {
                runs: layout_runs,
                width_twips: current_line_w,
                height_twips: line_h,
                fit_height_twips: fit,
                ascent_twips: ascent,
                descent_twips: descent,
                baseline_y_twips: ascent as i32,
            });

            is_first_line = false;
        }

        lines
    }

    fn fit_height(natural: u32, advance: u32, spacing: Option<LineSpacing>, known: bool) -> u32 {
        if known && matches!(spacing, Some(LineSpacing::Auto(_))) {
            natural.min(advance).max(1)
        } else { advance }
    }

    fn line_height(natural: u32, spacing: Option<LineSpacing>) -> u32 {
        match spacing {
            Some(LineSpacing::Auto(value)) => {
                (u64::from(natural) * u64::from(value) / 240).min(u64::from(u32::MAX)) as u32
            }
            Some(LineSpacing::Exact(value)) => value,
            Some(LineSpacing::AtLeast(value)) => natural.max(value),
            None => natural,
        }
        .max(1)
    }
}
