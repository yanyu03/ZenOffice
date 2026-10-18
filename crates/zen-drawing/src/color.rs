//! 颜色抽象 (RGB 与 Office 统一主题调色板槽位)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Color {
    Rgb([u8; 3]),
    Hex(String),
    Theme(ThemeColorSlot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColorSlot {
    Dark1,
    Light1,
    Dark2,
    Light2,
    Accent1,
    Accent2,
    Accent3,
    Accent4,
    Accent5,
    Accent6,
    Hyperlink,
    FollowedHyperlink,
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb([r, g, b])
    }

    pub fn from_hex(s: impl AsRef<str>) -> Result<Self, crate::Error> {
        let mut st = s.as_ref().trim();
        if let Some(stripped) = st.strip_prefix('#') {
            st = stripped;
        }
        if !st.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(crate::Error::InvalidColor(format!(
                "Hex color contains non-hexadecimal characters: {st}"
            )));
        }
        let normalized = match st.len() {
            3 => {
                let chars: Vec<char> = st.chars().collect();
                format!(
                    "{}{}{}{}{}{}",
                    chars[0], chars[0], chars[1], chars[1], chars[2], chars[2]
                )
                .to_ascii_uppercase()
            }
            6 => st.to_ascii_uppercase(),
            _ => {
                return Err(crate::Error::InvalidColor(format!(
                    "Hex color must be 3 or 6 hex digits, got {}: {st}",
                    st.len()
                )));
            }
        };
        Ok(Self::Hex(normalized))
    }

    pub fn hex(s: impl AsRef<str>) -> Self {
        Self::from_hex(s.as_ref()).unwrap_or_else(|_| Self::Hex("000000".to_string()))
    }

    /// 转换为 6 位十六进制 RGB 字符串 (用于 a:srgbClr val="...")
    pub fn to_hex_string(&self) -> String {
        match self {
            Color::Rgb([r, g, b]) => format!("{r:02X}{g:02X}{b:02X}"),
            Color::Hex(h) => {
                let filtered: String = h
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit())
                    .collect::<String>()
                    .to_ascii_uppercase();
                if filtered.len() == 6 {
                    filtered
                } else if filtered.len() == 3 {
                    let chars: Vec<char> = filtered.chars().collect();
                    format!(
                        "{}{}{}{}{}{}",
                        chars[0], chars[0], chars[1], chars[1], chars[2], chars[2]
                    )
                } else {
                    "000000".to_string()
                }
            }
            Color::Theme(slot) => slot.default_hex().to_string(),
        }
    }

    /// 转换为 CSS/SVG 格式颜色字符串
    pub fn to_svg_string(&self) -> String {
        format!("#{}", self.to_hex_string())
    }
}

impl ThemeColorSlot {
    pub fn xml_name(&self) -> &'static str {
        match self {
            Self::Dark1 => "dk1",
            Self::Light1 => "lt1",
            Self::Dark2 => "dk2",
            Self::Light2 => "lt2",
            Self::Accent1 => "accent1",
            Self::Accent2 => "accent2",
            Self::Accent3 => "accent3",
            Self::Accent4 => "accent4",
            Self::Accent5 => "accent5",
            Self::Accent6 => "accent6",
            Self::Hyperlink => "hlink",
            Self::FollowedHyperlink => "folHlink",
        }
    }

    pub fn default_hex(&self) -> &'static str {
        match self {
            Self::Dark1 => "000000",
            Self::Light1 => "FFFFFF",
            Self::Dark2 => "1F497D",
            Self::Light2 => "EEECE1",
            Self::Accent1 => "4F81BD",
            Self::Accent2 => "C0504D",
            Self::Accent3 => "9BBB59",
            Self::Accent4 => "8064A2",
            Self::Accent5 => "4BACC6",
            Self::Accent6 => "F79646",
            Self::Hyperlink => "0000FF",
            Self::FollowedHyperlink => "800080",
        }
    }
}
