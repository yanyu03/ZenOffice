#[derive(Debug, Clone)]
pub struct Paragraph {
    pub text: String,
    pub is_heading: bool,
}

impl Paragraph {
    pub fn new(text: &str, is_heading: bool) -> Self {
        Self {
            text: text.to_string(),
            is_heading,
        }
    }
}

/// In-memory Document model for Word processor.
pub struct Document {
    pub title: String,
    pub paragraphs: Vec<Paragraph>,
    pub cursor_p: usize,
    pub cursor_char: usize,
}

impl Document {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            paragraphs: vec![Paragraph::new("", false)],
            cursor_p: 0,
            cursor_char: 0,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch == '\r' || ch == '\n' {
            self.split_paragraph();
            return;
        }

        if self.cursor_p < self.paragraphs.len() {
            let p = &mut self.paragraphs[self.cursor_p];
            let mut byte_idx = 0;
            for (i, (idx, _)) in p.text.char_indices().enumerate() {
                if i == self.cursor_char {
                    byte_idx = idx;
                    break;
                }
                byte_idx = p.text.len();
            }
            p.text.insert(byte_idx, ch);
            self.cursor_char += 1;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor_char > 0 && self.cursor_p < self.paragraphs.len() {
            let p = &mut self.paragraphs[self.cursor_p];
            self.cursor_char -= 1;
            let mut byte_idx = 0;
            for (i, (idx, _)) in p.text.char_indices().enumerate() {
                if i == self.cursor_char {
                    byte_idx = idx;
                    break;
                }
            }
            p.text.remove(byte_idx);
        } else if self.cursor_p > 0 {
            // Merge with previous paragraph
            let current = self.paragraphs.remove(self.cursor_p);
            self.cursor_p -= 1;
            let prev = &mut self.paragraphs[self.cursor_p];
            self.cursor_char = prev.text.chars().count();
            prev.text.push_str(&current.text);
        }
    }

    pub fn split_paragraph(&mut self) {
        if self.cursor_p < self.paragraphs.len() {
            let p = &mut self.paragraphs[self.cursor_p];
            let mut byte_idx = p.text.len();
            for (i, (idx, _)) in p.text.char_indices().enumerate() {
                if i == self.cursor_char {
                    byte_idx = idx;
                    break;
                }
            }
            let remainder = p.text.split_off(byte_idx);
            self.paragraphs.insert(self.cursor_p + 1, Paragraph::new(&remainder, false));
            self.cursor_p += 1;
            self.cursor_char = 0;
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_char > 0 {
            self.cursor_char -= 1;
        } else if self.cursor_p > 0 {
            self.cursor_p -= 1;
            self.cursor_char = self.paragraphs[self.cursor_p].text.chars().count();
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_p < self.paragraphs.len() {
            let count = self.paragraphs[self.cursor_p].text.chars().count();
            if self.cursor_char < count {
                self.cursor_char += 1;
            } else if self.cursor_p + 1 < self.paragraphs.len() {
                self.cursor_p += 1;
                self.cursor_char = 0;
            }
        }
    }

    pub fn move_cursor_up(&mut self) {
        if self.cursor_p > 0 {
            self.cursor_p -= 1;
            let count = self.paragraphs[self.cursor_p].text.chars().count();
            self.cursor_char = self.cursor_char.min(count);
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor_p + 1 < self.paragraphs.len() {
            self.cursor_p += 1;
            let count = self.paragraphs[self.cursor_p].text.chars().count();
            self.cursor_char = self.cursor_char.min(count);
        }
    }
}
