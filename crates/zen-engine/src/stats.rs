use crate::doc::Document;

#[derive(Debug, Clone, Default)]
pub struct DocumentStats {
    pub char_count: usize,
    pub cjk_count: usize,
    pub word_count: usize,
    pub paragraph_count: usize,
}

impl DocumentStats {
    pub fn compute(doc: &Document) -> Self {
        let mut char_count = 0;
        let mut cjk_count = 0;
        let mut word_count = 0;
        let paragraph_count = doc.paragraphs.len();

        for p in &doc.paragraphs {
            char_count += p.text.chars().count();
            for ch in p.text.chars() {
                // Check if CJK Unified Ideographs
                if ('\u{4e00}'..='\u{9fa5}').contains(&ch) {
                    cjk_count += 1;
                }
            }
            // Count ASCII words
            word_count += p.text.split_whitespace().count();
        }

        Self {
            char_count,
            cjk_count,
            word_count,
            paragraph_count,
        }
    }

    pub fn summary_string(&self) -> String {
        format!(
            "段落: {} | 字符总数: {} | 中文字数: {} | 词数: {}",
            self.paragraph_count, self.char_count, self.cjk_count, self.word_count
        )
    }
}
