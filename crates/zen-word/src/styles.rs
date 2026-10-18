//! Effective formatting for the supported properties. Resolution is read-only:
//! original style references and package bytes remain the source of truth.
use crate::model::*;
use crate::{Result, WordError};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct ResolvedParagraph {
    pub paragraph: Paragraph,
    pub diagnostics: Vec<String>,
}

pub struct StyleResolver<'a> {
    styles: &'a Styles,
    definitions: BTreeMap<String, StyleDef>,
}

fn error(message: impl Into<String>) -> WordError {
    WordError::UnsupportedEdit(format!("style resolution: {}", message.into()))
}

impl<'a> StyleResolver<'a> {
    pub fn new(styles: &'a Styles) -> Result<Self> {
        if let Some(ids) = &styles.imported_style_ids {
            let mut seen = BTreeSet::new();
            for id in ids {
                if !seen.insert(id) {
                    return Err(error(format!("duplicate imported style ID {id}")));
                }
            }
        }
        let mut definitions = BTreeMap::new();
        for template in template_style_defs() {
            if styles
                .imported_style_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&template.style_id))
            {
                definitions.insert(template.style_id.clone(), template);
            }
        }
        let mut overrides = BTreeSet::new();
        for style in &styles.builtin_overrides {
            if !is_template_style_id(&style.style_id) || !overrides.insert(&style.style_id) {
                return Err(error("invalid or duplicate built-in override"));
            }
            definitions.insert(style.style_id.clone(), style.clone());
        }
        for style in &styles.named {
            if definitions.contains_key(&style.style_id) || is_template_style_id(&style.style_id) {
                return Err(error(format!(
                    "duplicate or reserved custom style ID {}",
                    style.style_id
                )));
            }
            definitions.insert(style.style_id.clone(), style.clone());
        }
        Ok(Self {
            styles,
            definitions,
        })
    }

    /// Missing/wrong-kind links are ignored with diagnostics; cycles and
    /// excessive depth are rejected, without recursion or an unbounded walk.
    fn chain(
        &self,
        id: &str,
        kind: StyleKind,
        diagnostics: &mut Vec<String>,
    ) -> Result<Vec<&StyleDef>> {
        let mut current = Some(id);
        let mut seen = BTreeSet::new();
        let mut chain = Vec::new();
        while let Some(id) = current {
            if chain.len() >= 256 {
                return Err(error("style inheritance depth exceeds 256"));
            }
            if !seen.insert(id) {
                return Err(error(format!("basedOn cycle at {id}")));
            }
            let Some(style) = self.definitions.get(id) else {
                diagnostics.push(format!(
                    "Style {id} is missing; its unavailable properties were not invented."
                ));
                break;
            };
            if style.kind != kind {
                diagnostics.push(format!(
                    "Style {id} has the wrong kind; that inheritance link was ignored."
                ));
                break;
            }
            chain.push(style);
            current = style.based_on.as_deref();
        }
        chain.reverse();
        Ok(chain)
    }

    fn selected_chain(
        &self,
        requested: Option<&str>,
        fallback: Option<&str>,
        kind: StyleKind,
        diagnostics: &mut Vec<String>,
    ) -> Result<Vec<&StyleDef>> {
        let Some(id) = requested.or(fallback) else {
            return Ok(Vec::new());
        };
        let chain = self.chain(id, kind, diagnostics)?;
        if chain.is_empty() && requested.is_some() && fallback.is_some_and(|f| f != id) {
            return self.chain(fallback.unwrap(), kind, diagnostics);
        }
        Ok(chain)
    }

    pub fn resolve_paragraph(&self, source: &Paragraph) -> Result<ResolvedParagraph> {
        let mut diagnostics = Vec::new();
        let implicit_normal = self
            .styles
            .imported_style_ids
            .is_none()
            .then_some(STYLE_NORMAL);
        let default_paragraph = self
            .styles
            .default_paragraph_style_id
            .as_deref()
            .or(implicit_normal);
        let chain = self.selected_chain(
            source.props.style_id.as_deref(),
            default_paragraph,
            StyleKind::Paragraph,
            &mut diagnostics,
        )?;
        let mut para = ParagraphProps::default();
        merge_para(&mut para, &self.styles.default_para);
        let mut base_run = RunProps::default();
        merge_run(&mut base_run, &self.styles.default_run, false);
        for style in chain {
            merge_para(&mut para, &style.para);
            merge_run(&mut base_run, &style.run, true);
            // Native Word resolves explicit caps within a paragraph basedOn
            // chain to the nearest value. Keep character-style toggling and
            // direct formatting separate from that chain resolution.
            if style.run.caps.is_some() {
                base_run.caps = style.run.caps;
            }
        }
        merge_para(&mut para, &source.props);
        // References and section boundaries are local structure, not inherited.
        para.style_id = source.props.style_id.clone();
        para.section_end = source.props.section_end.clone();
        para.mark_run = source.props.mark_run.clone();
        let mut runs = Vec::with_capacity(source.runs.len());
        for run in &source.runs {
            let mut properties = base_run.clone();
            let char_chain = self.selected_chain(
                run.props.style_id.as_deref(),
                self.styles.default_character_style_id.as_deref(),
                StyleKind::Character,
                &mut diagnostics,
            )?;
            for style in char_chain {
                merge_run(&mut properties, &style.run, true);
            }
            merge_run(&mut properties, &run.props, false);
            properties.style_id = run.props.style_id.clone();
            if properties.size_half_points == Some(0) {
                return Err(error("font size must be positive"));
            }
            runs.push(Run {
                props: properties,
                contents: run.contents.clone(),
            });
        }
        // Empty paragraphs still have a line box governed by the paragraph's
        // effective run properties rather than a hard-coded font size.
        if runs.is_empty() || (!source.props.mark_run.is_empty()
            && source.runs.iter().all(Run::is_effectively_empty)) {
            let mut properties = base_run;
            for style in self.selected_chain(
                source.props.mark_run.style_id.as_deref(),
                self.styles.default_character_style_id.as_deref(),
                StyleKind::Character,
                &mut diagnostics,
            )? {
                merge_run(&mut properties, &style.run, true);
            }
            merge_run(&mut properties, &source.props.mark_run, false);
            if properties.size_half_points == Some(0) {
                return Err(error("font size must be positive"));
            }
            runs.clear();
            runs.push(Run {
                props: properties,
                contents: Vec::new(),
            });
        }
        diagnostics.sort();
        diagnostics.dedup();
        Ok(ResolvedParagraph {
            paragraph: Paragraph { props: para, runs },
            diagnostics,
        })
    }
}

fn merge_para(target: &mut ParagraphProps, source: &ParagraphProps) {
    macro_rules! copy { ($($field:ident),* $(,)?) => { $(if source.$field.is_some() { target.$field = source.$field.clone(); })* }; }
    copy!(
        align,
        ind_left_twips,
        ind_right_twips,
        ind_first_line_twips,
        space_before_twips,
        space_after_twips,
        line_spacing,
        outline_level,
        shading
    );
    if source.keep_next || source.keep_next_off {
        target.keep_next = source.keep_next && !source.keep_next_off;
        target.keep_next_off = source.keep_next_off;
    }
    if source.page_break_before || source.page_break_before_off {
        target.page_break_before = source.page_break_before && !source.page_break_before_off;
        target.page_break_before_off = source.page_break_before_off;
    }
}

fn merge_run(target: &mut RunProps, source: &RunProps, in_style: bool) {
    macro_rules! copy { ($($field:ident),* $(,)?) => { $(if source.$field.is_some() { target.$field = source.$field.clone(); })* }; }
    copy!(
        underline,
        size_half_points,
        font_ascii,
        font_east_asia,
        vert_align,
        highlight
    );
    for (destination, value) in [
        (&mut target.bold, source.bold),
        (&mut target.italic, source.italic),
        (&mut target.caps, source.caps),
        (&mut target.strike, source.strike),
    ] {
        if in_style {
            if value == Some(true) {
                *destination = Some(!destination.unwrap_or(false));
            }
        } else if value.is_some() {
            *destination = value;
        }
    }
    if source.color_auto {
        target.color = None;
        target.color_auto = true;
    } else if let Some(color) = &source.color {
        target.color = Some(color.clone());
        target.color_auto = false;
    }
}
