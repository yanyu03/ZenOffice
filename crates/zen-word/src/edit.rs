//! 编辑操作（窗口无关的模型层）。
//!
//! 这一层只做"文本与段落结构怎么变"，不关心光标怎么画、按键从哪来——
//! 所以它能被单元测试完整覆盖，也能在没有窗口的环境里跑脚本验证（`--edit`）。
//!
//! ## v0.4 支持范围
//!
//! * 插入文本、回车分段、退格（跨 run 与跨段合并）、左右移动光标
//! * **只在顶层段落里编辑**：表格单元格内的编辑未实现（光标不会落在单元格里）
//! * 不做选区、不做撤销、不做输入法组合（IME）
//!
//! ## 光标模型
//!
//! `Caret { block, run, ch }`：`block` 是**顶层块下标**，`run` 是该段落的运行下标，
//! `ch` 是运行内**字符**下标（不是字节下标——中文一个字算一个）。合法范围由
//! [`Document::normalize_caret`] 夹紧，任何编辑前都会先夹一次，所以调用方不必自己防越界。

use crate::model::*;

/// 插入位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    pub block: usize,
    pub run: usize,
    pub ch: usize,
}

impl Caret {
    pub const START: Caret = Caret { block: 0, run: 0, ch: 0 };

    pub fn new(block: usize, run: usize, ch: usize) -> Self {
        Self { block, run, ch }
    }
}

/// 编辑结果：新光标 + 本次是否真的改动了模型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditOutcome {
    pub caret: Caret,
    pub changed: bool,
}

impl Document {
    /// 顶层块数量
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// 顶层块是不是段落（v0.4 只能在段落里编辑）
    pub fn paragraph_at(&self, block: usize) -> Option<&Paragraph> {
        match self.blocks.get(block) {
            Some(Block::Paragraph(p)) => Some(p),
            _ => None,
        }
    }

    /// 段落里某运行的文本
    pub fn run_text(&self, caret: Caret) -> Option<String> {
        let p = self.paragraph_at(caret.block)?;
        let r = p.runs.get(caret.run)?;
        Some(r.plain_text())
    }

    /// 把光标夹到合法位置。空段落、空运行、越界下标都在这里被吸收。
    pub fn normalize_caret(&self, c: Caret) -> Caret {
        // 找最近的可用段落（块存在但不是段落时，向后找；找不到就退到 0）
        let block = if self.paragraph_at(c.block).is_some() {
            c.block
        } else {
            self.blocks
                .iter()
                .enumerate()
                .skip(c.block)
                .find_map(|(i, b)| matches!(b, Block::Paragraph(_)).then_some(i))
                .or_else(|| {
                    self.blocks
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(i, b)| matches!(b, Block::Paragraph(_)).then_some(i))
                })
                .unwrap_or(0)
        };
        let Some(p) = self.paragraph_at(block) else {
            return Caret::START;
        };
        let run = c.run.min(p.runs.len().saturating_sub(1));
        let len = p.runs.get(run).map(|r| r.plain_text().chars().count()).unwrap_or(0);
        Caret { block, run, ch: c.ch.min(len) }
    }

    /// 文档纯文本（段落一行，表格行用制表符）——与 `read::extract_text` 同口径，
    /// 编辑测试与脚本验收共用。
    pub fn plain_text(&self) -> String {
        crate::read::extract_text(self)
    }

    /// 在光标处插入文本。返回新光标（落在插入内容之后）。
    pub fn insert_text(&mut self, caret: Caret, text: &str) -> EditOutcome {
        let caret = self.normalize_caret(caret);
        if text.is_empty() {
            return EditOutcome { caret, changed: false };
        }
        let Some(Block::Paragraph(p)) = self.blocks.get_mut(caret.block) else {
            return EditOutcome { caret, changed: false };
        };
        if p.runs.is_empty() {
            // 空段落：造一个默认运行来承载文字
            p.runs.push(Run::text(""));
        }
        let run_idx = caret.run.min(p.runs.len() - 1);
        let at_byte = byte_index(&p.runs[run_idx].plain_text(), caret.ch);
        insert_into_run(&mut p.runs[run_idx], at_byte, text);
        EditOutcome {
            caret: Caret { ch: caret.ch + text.chars().count(), ..caret },
            changed: true,
        }
    }

    /// 回车：在光标处把段落切成两段。
    ///
    /// 光标所在运行被就地切成左右两半，左侧留在原段，右侧连同**其后的所有运行**
    /// 成为新段落——这与常见编辑器的行为一致。
    pub fn split_paragraph(&mut self, caret: Caret) -> EditOutcome {
        let caret = self.normalize_caret(caret);
        let Some(Block::Paragraph(p)) = self.blocks.get(caret.block) else {
            return EditOutcome { caret, changed: false };
        };
        let props = p.props.clone();
        let run_count = p.runs.len();
        let run_idx = caret.run.min(run_count.saturating_sub(1));

        // 取出原段，切成 head / tail
        let Block::Paragraph(old) = self.blocks[caret.block].clone() else {
            unreachable!()
        };
        let mut head: Vec<Run> = old.runs[..run_idx].to_vec();
        let split_run = old.runs.get(run_idx).cloned();
        let mut tail: Vec<Run> = if run_idx + 1 <= old.runs.len() {
            old.runs[run_idx + 1..].to_vec()
        } else {
            Vec::new()
        };

        let (left_run, right_run) = match split_run {
            Some(r) => {
                let (lt, rt) = split_run_at(&r, caret.ch);
                (lt, rt)
            }
            None => (Run::text(""), Run::text("")),
        };
        head.push(left_run);
        tail.insert(0, right_run);

        let mut head_props = props.clone();
        head_props.section_end = None;
        self.blocks[caret.block] = Block::Paragraph(Paragraph { props: head_props, runs: head });
        self.blocks.insert(caret.block + 1, Block::Paragraph(Paragraph { props, runs: tail }));

        // 新段落里，"右侧第一段"可能不止一个 run；光标落在新段的第一个 run 的头部。
        // 若右侧为空（回车在段尾），新段只有一个空 run，光标在 (0,0)。
        EditOutcome { caret: Caret { block: caret.block + 1, run: 0, ch: 0 }, changed: true }
    }

    /// 退格：合并运行、必要时合并段落。
    pub fn delete_backward(&mut self, caret: Caret) -> EditOutcome {
        let caret = self.normalize_caret(caret);

        // 段内：删掉前一个字符
        if caret.ch > 0 {
            let Some(Block::Paragraph(p)) = self.blocks.get_mut(caret.block) else {
                return EditOutcome { caret, changed: false };
            };
            if let Some(run) = p.runs.get_mut(caret.run) {
                let text = run.plain_text();
                let mut chars: Vec<char> = text.chars().collect();
                chars.remove(caret.ch - 1);
                set_run_text(run, &chars.into_iter().collect::<String>());
                return EditOutcome {
                    caret: Caret { ch: caret.ch - 1, ..caret },
                    changed: true,
                };
            }
            return EditOutcome { caret, changed: false };
        }

        // 运行头：与上一个运行合并
        if caret.run > 0 {
            let Some(Block::Paragraph(p)) = self.blocks.get_mut(caret.block) else {
                return EditOutcome { caret, changed: false };
            };
            let prev_len = p.runs[caret.run - 1].plain_text().chars().count();
            let moved = p.runs.remove(caret.run);
            append_to_run(&mut p.runs[caret.run - 1], &moved);
            // 合并本身不删字：光标停在两段文本的接缝处
            return EditOutcome {
                caret: Caret { run: caret.run - 1, ch: prev_len, ..caret },
                changed: true,
            };
        }

        // 段落头：把这一段并回上一段
        if caret.block > 0 {
            // A normal Backspace must not silently delete a section boundary.
            if self.paragraph_at(caret.block - 1).is_some_and(|p| p.props.section_end.is_some()) {
                return EditOutcome { caret, changed: false };
            }
            let Some(prev_is_para) = self.blocks.get(caret.block - 1).map(|b| matches!(b, Block::Paragraph(_)))
            else {
                return EditOutcome { caret, changed: false };
            };
            if !prev_is_para {
                return EditOutcome { caret, changed: false };
            }
            let Block::Paragraph(cur) = self.blocks.remove(caret.block) else {
                unreachable!()
            };
            let Block::Paragraph(prev) = &mut self.blocks[caret.block - 1] else {
                unreachable!()
            };
            prev.props.section_end = cur.props.section_end.clone();
            if prev.runs.is_empty() {
                prev.runs.push(Run::text(""));
            }
            // 合并前记住"上一段最后一个运行有多长"：合并后光标就停在那里
            // （即原两段的接缝处）。
            let join_run = prev.runs.len() - 1;
            let join_ch = prev.runs[join_run].plain_text().chars().count();

            // 当前段首个运行并入上一段最后一个运行；其余运行整段追加
            let mut cur = cur.runs;
            if !cur.is_empty() {
                let first = cur.remove(0);
                append_to_run(&mut prev.runs[join_run], &first);
                prev.runs.extend(cur);
            }
            return EditOutcome {
                caret: Caret { block: caret.block - 1, run: join_run, ch: join_ch },
                changed: true,
            };
        }

        EditOutcome { caret, changed: false }
    }

    /// 左移一格（跨运行、跨段落）
    pub fn move_left(&self, caret: Caret) -> Caret {
        let c = self.normalize_caret(caret);
        if c.ch > 0 {
            return Caret { ch: c.ch - 1, ..c };
        }
        if c.run > 0 {
            let prev = self
                .paragraph_at(c.block)
                .and_then(|p| p.runs.get(c.run - 1))
                .map(|r| r.plain_text().chars().count())
                .unwrap_or(0);
            return Caret { run: c.run - 1, ch: prev, ..c };
        }
        // 上一段的末尾
        for b in (0..c.block).rev() {
            if let Some(p) = self.paragraph_at(b) {
                if p.runs.is_empty() {
                    return Caret { block: b, run: 0, ch: 0 };
                }
                let r = p.runs.len() - 1;
                let ch = p.runs[r].plain_text().chars().count();
                return Caret { block: b, run: r, ch };
            }
        }
        Caret { ch: 0, ..c }
    }

    /// 光标是否落在文档末尾（最后一个可编辑位置）
    pub fn is_at_end(&self, caret: Caret) -> bool {
        let c = self.normalize_caret(caret);
        let last = (0..self.blocks.len())
            .rev()
            .find(|b| self.paragraph_at(*b).is_some());
        match last {
            Some(b) => {
                let p = self.paragraph_at(b).unwrap();
                c.block == b
                    && c.run + 1 >= p.runs.len().max(1)
                    && c.ch >= self.run_text(c).map(|t| t.chars().count()).unwrap_or(0)
            }
            None => true,
        }
    }

    /// 右移一格（跨运行、跨段落）
    pub fn move_right(&self, caret: Caret) -> Caret {
        let c = self.normalize_caret(caret);
        let cur_len = self.run_text(c).map(|t| t.chars().count()).unwrap_or(0);
        if c.ch < cur_len {
            return Caret { ch: c.ch + 1, ..c };
        }
        if let Some(p) = self.paragraph_at(c.block) {
            if c.run + 1 < p.runs.len() {
                return Caret { run: c.run + 1, ch: 0, ..c };
            }
        }
        // 下一段的开头
        for b in c.block + 1..self.blocks.len() {
            if self.paragraph_at(b).is_some() {
                return Caret { block: b, run: 0, ch: 0 };
            }
        }
        Caret { ch: cur_len, ..c }
    }
}

// ---------------------------------------------------------------- 内部助手

fn byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// 往运行里插入文本：只在第一个 Text 内容里插，其余内容（换行/制表符）保持不变。
fn insert_into_run(run: &mut Run, at_byte: usize, text: &str) {
    use crate::model::RunContent;
    for c in run.contents.iter_mut() {
        if let RunContent::Text(t) = c {
            let at = at_byte.min(t.len());
            // at 必须落在字符边界上
            let at = if t.is_char_boundary(at) { at } else { t.len() };
            t.insert_str(at, text);
            return;
        }
    }
    run.contents.insert(0, RunContent::Text(text.to_string()));
}

/// 把运行的可见文本整体替换（保留换行/制表符之外的结构：本版直接把文本内容合并成一个 Text）
fn set_run_text(run: &mut Run, text: &str) {
    run.contents.retain(|c| !matches!(c, crate::model::RunContent::Text(_)));
    run.contents.insert(0, crate::model::RunContent::Text(text.to_string()));
}

/// 把 `other` 的文本接到 `run` 末尾（用于退格合并）
fn append_to_run(run: &mut Run, other: &Run) {
    let text = other.plain_text();
    if text.is_empty() {
        return;
    }
    let end = run.plain_text().len();
    insert_into_run(run, end, &text);
}

/// 在字符下标处把一个运行切成两个（属性各自复制）
fn split_run_at(run: &Run, char_index: usize) -> (Run, Run) {
    use crate::model::RunContent;
    let char_index = char_index.min(run.plain_text().chars().count());
    let mut left = Run { props: run.props.clone(), contents: Vec::new() };
    let mut right = Run { props: run.props.clone(), contents: Vec::new() };
    let mut seen = 0usize;
    for c in &run.contents {
        match c {
            RunContent::Text(t) => {
                let chars: Vec<char> = t.chars().collect();
                for ch in chars {
                    if seen < char_index {
                        left.contents.push(RunContent::Text(ch.to_string()));
                    } else {
                        right.contents.push(RunContent::Text(ch.to_string()));
                    }
                    seen += 1;
                }
            }
            other => {
                if seen < char_index {
                    left.contents.push(other.clone());
                } else {
                    right.contents.push(other.clone());
                }
            }
        }
    }
    coalesce(&mut left);
    coalesce(&mut right);
    (left, right)
}

/// 相邻的 Text 片段合并成一个（切分后会产生大量单字符片段）
fn coalesce(run: &mut Run) {
    use crate::model::RunContent;
    let mut out: Vec<RunContent> = Vec::with_capacity(run.contents.len());
    for c in run.contents.drain(..) {
        match (out.last_mut(), c) {
            (Some(RunContent::Text(prev)), RunContent::Text(t)) => prev.push_str(&t),
            (_, other) => out.push(other),
        }
    }
    run.contents = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(texts: &[&str]) -> Document {
        Document {
            blocks: texts.iter().map(|t| Block::paragraph(*t)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn insert_in_the_middle_and_at_the_end() {
        let mut d = doc_with(&["abcdef"]);
        let c = d.insert_text(Caret::new(0, 0, 3), "XYZ");
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "abcXYZdef");
        assert_eq!(c.caret, Caret::new(0, 0, 6), "光标应停在插入内容之后");

        let c = d.insert_text(Caret::new(0, 0, 9), "!");
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "abcXYZdef!");
        assert_eq!(c.caret.ch, 10);
        assert!(c.changed);
    }

    #[test]
    fn insert_into_empty_paragraph_creates_a_run() {
        let mut d = doc_with(&[""]);
        let c = d.insert_text(Caret::START, "你好");
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "你好");
        assert_eq!(c.caret, Caret::new(0, 0, 2), "中文按字符计数");
    }

    #[test]
    fn caret_is_clamped_instead_of_panicking() {
        let d = doc_with(&["abc"]);
        assert_eq!(d.normalize_caret(Caret::new(0, 9, 99)), Caret::new(0, 0, 3));
        assert_eq!(d.normalize_caret(Caret::new(77, 0, 0)), Caret::new(0, 0, 0));
        let d2 = Document::default();
        assert_eq!(d2.normalize_caret(Caret::new(3, 4, 5)), Caret::START);
    }

    #[test]
    fn enter_splits_text_and_moves_following_runs() {
        // 一段里两个运行："前半" + "后半"
        let mut p = Paragraph::default();
        p.runs.push(Run::text("前半"));
        p.runs.push(Run::text("后半"));
        let mut d = Document { blocks: vec![Block::Paragraph(p)], ..Default::default() };

        // 在第一个运行的第 1 个字符处回车
        let out = d.split_paragraph(Caret::new(0, 0, 1));
        assert_eq!(out.caret, Caret::new(1, 0, 0));
        assert_eq!(d.block_count(), 2);
        let a = d.paragraph_at(0).unwrap().plain_text();
        let b = d.paragraph_at(1).unwrap().plain_text();
        assert_eq!(a, "前");
        assert_eq!(b, "半后半", "被切开的右半与其后的运行都要跟到新段");
    }

    #[test]
    fn enter_at_end_creates_an_empty_paragraph() {
        let mut d = doc_with(&["abc"]);
        let out = d.split_paragraph(Caret::new(0, 0, 3));
        assert_eq!(d.block_count(), 2);
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "abc");
        assert_eq!(d.paragraph_at(1).unwrap().plain_text(), "");
        assert_eq!(out.caret, Caret::new(1, 0, 0));
    }

    #[test]
    fn backspace_deletes_within_a_run() {
        let mut d = doc_with(&["abcd"]);
        let out = d.delete_backward(Caret::new(0, 0, 2));
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "acd");
        assert_eq!(out.caret.ch, 1);
    }

    #[test]
    fn backspace_at_run_start_merges_runs() {
        let mut p = Paragraph::default();
        p.runs.push(Run::text("甲"));
        p.runs.push(Run::text("乙"));
        let mut d = Document { blocks: vec![Block::Paragraph(p)], ..Default::default() };
        let out = d.delete_backward(Caret::new(0, 1, 0));
        let para = d.paragraph_at(0).unwrap();
        assert_eq!(para.runs.len(), 1, "两个运行应合并");
        assert_eq!(para.plain_text(), "甲乙");
        assert_eq!(out.caret, Caret::new(0, 0, 1), "光标停在接缝处");
    }

    #[test]
    fn backspace_at_paragraph_start_merges_paragraphs() {
        let mut d = doc_with(&["第一段", "第二段"]);
        let out = d.delete_backward(Caret::new(1, 0, 0));
        assert_eq!(d.block_count(), 1);
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "第一段第二段");
        assert_eq!(out.caret, Caret::new(0, 0, 3), "光标在接缝处");
    }

    #[test]
    fn backspace_at_document_start_does_nothing() {
        let mut d = doc_with(&["abc"]);
        let before = d.clone();
        let out = d.delete_backward(Caret::START);
        assert!(!out.changed);
        assert_eq!(d, before, "文档开头退格不得改动模型");
    }

    #[test]
    fn left_and_right_traverse_runs_and_paragraphs() {
        let mut p = Paragraph::default();
        p.runs.push(Run::text("ab"));
        p.runs.push(Run::text("cd"));
        let d = Document {
            blocks: vec![Block::Paragraph(p), Block::paragraph("尾")],
            ..Default::default()
        };

        // 段内：从 (0,1,0) 左移 → (0,0,2)
        assert_eq!(d.move_left(Caret::new(0, 1, 0)), Caret::new(0, 0, 2));
        // (0,1,2) 右移 → 下一段段首
        assert_eq!(d.move_right(Caret::new(0, 1, 2)), Caret::new(1, 0, 0));
        // 第二段段首左移 → 回到上一段末尾
        assert_eq!(d.move_left(Caret::new(1, 0, 0)), Caret::new(0, 1, 2));
        // 文档最左/最右不越界
        assert_eq!(d.move_left(Caret::START), Caret::START);
        assert_eq!(d.move_right(Caret::new(1, 0, 1)), Caret::new(1, 0, 1));
        assert!(d.is_at_end(Caret::new(1, 0, 1)));
        assert!(!d.is_at_end(Caret::new(0, 0, 0)));
    }

    #[test]
    fn editing_sequence_survives_write_and_read() {
        // 建一段、打字、回车、再打字、退格，然后写出再读回：内容必须一致
        let mut d = doc_with(&["标题"]);
        let mut c = d.insert_text(Caret::new(0, 0, 2), "与副标题").caret;
        assert_eq!(d.paragraph_at(0).unwrap().plain_text(), "标题与副标题");
        c = d.split_paragraph(c).caret;
        c = d.insert_text(c, "正文第一行").caret;
        assert_eq!(d.block_count(), 2);
        c = d.insert_text(c, "，继续").caret;
        c = d.delete_backward(c).caret; // 删掉"续"
        assert_eq!(d.paragraph_at(1).unwrap().plain_text(), "正文第一行，继");

        let bytes = crate::write_docx(&d).expect("写出");
        let back = crate::read_docx(&bytes).expect("读回").document;
        assert_eq!(back.blocks, d.blocks, "编辑后写出再读回必须一致");
        assert_eq!(back.plain_text(), "标题与副标题
正文第一行，继
");
        let _ = c;
    }

    #[test]
    fn caret_skips_non_paragraph_blocks() {
        // 表格块不可编辑：光标应被夹到最近的段落
        let mut d = doc_with(&["前"]);
        d.blocks.push(Block::Table(crate::model::Table::new(vec![2000])));
        d.blocks.push(Block::paragraph("后"));
        assert_eq!(d.normalize_caret(Caret::new(1, 0, 0)), Caret::new(2, 0, 0));
    }
}

/// 可切换的字符格式（对应 WPS「开始」标签里的按钮）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunProp {
    Bold,
    Italic,
    Underline,
}

impl Document {
    /// 切换光标所在运行的字符格式。
    ///
    /// v0.5 只作用于**光标所在的那一个运行**（没有选区）——所以工具栏按钮能立刻看到效果，
    /// 但"选中一段再加粗"要等选区批次。返回值里 `changed=false` 表示没找到运行。
    pub fn toggle_run_prop(&mut self, caret: Caret, which: RunProp) -> EditOutcome {
        let caret = self.normalize_caret(caret);
        let Some(Block::Paragraph(p)) = self.blocks.get_mut(caret.block) else {
            return EditOutcome { caret, changed: false };
        };
        if p.runs.is_empty() {
            p.runs.push(Run::text(""));
        }
        let idx = caret.run.min(p.runs.len() - 1);
        // 先读出当前值再写回：直接在 &mut 借用里读会撞上借用检查
        let (bold, italic, underline) = {
            let r = &p.runs[idx];
            (r.props.bold, r.props.italic, r.props.underline)
        };
        let flip = |v: Option<bool>| Some(v != Some(true));
        match which {
            RunProp::Bold => p.runs[idx].props.bold = flip(bold),
            RunProp::Italic => p.runs[idx].props.italic = flip(italic),
            RunProp::Underline => {
                p.runs[idx].props.underline =
                    if matches!(underline, Some(crate::model::Underline::Single)) {
                        Some(crate::model::Underline::None)
                    } else {
                        Some(crate::model::Underline::Single)
                    }
            }
        }
        EditOutcome { caret, changed: true }
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;

    fn doc_with(texts: &[&str]) -> Document {
        Document {
            blocks: texts.iter().map(|t| Block::paragraph(*t)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn toggling_bold_italic_underline_on_the_run_at_caret() {
        let mut d = doc_with(&["abc"]);
        let c = Caret::new(0, 0, 1);

        d.toggle_run_prop(c, RunProp::Bold);
        assert_eq!(d.paragraph_at(0).unwrap().runs[0].props.bold, Some(true));
        d.toggle_run_prop(c, RunProp::Bold);
        assert_eq!(d.paragraph_at(0).unwrap().runs[0].props.bold, Some(false), "再点一次应关掉");

        d.toggle_run_prop(c, RunProp::Italic);
        assert_eq!(d.paragraph_at(0).unwrap().runs[0].props.italic, Some(true));

        d.toggle_run_prop(c, RunProp::Underline);
        assert!(matches!(
            d.paragraph_at(0).unwrap().runs[0].props.underline,
            Some(crate::model::Underline::Single)
        ));
        d.toggle_run_prop(c, RunProp::Underline);
        assert!(matches!(
            d.paragraph_at(0).unwrap().runs[0].props.underline,
            Some(crate::model::Underline::None)
        ));
    }

    #[test]
    fn format_survives_write_and_read() {
        let mut d = doc_with(&["加粗这一段"]);
        d.toggle_run_prop(Caret::START, RunProp::Bold);
        d.toggle_run_prop(Caret::START, RunProp::Italic);
        let bytes = crate::write_docx(&d).unwrap();
        let back = crate::read_docx(&bytes).unwrap().document;
        assert_eq!(back.blocks, d.blocks, "格式必须能原样读回");
        let r = &back.paragraph_at(0).unwrap().runs[0].props;
        assert_eq!(r.bold, Some(true));
        assert_eq!(r.italic, Some(true));
    }

    #[test]
    fn toggling_in_an_empty_paragraph_creates_a_run_to_hold_the_format() {
        let mut d = doc_with(&[""]);
        let out = d.toggle_run_prop(Caret::START, RunProp::Bold);
        assert!(out.changed);
        assert_eq!(d.paragraph_at(0).unwrap().runs[0].props.bold, Some(true));
    }
}
