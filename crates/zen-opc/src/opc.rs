//! OPC 元数据与包装配：`[Content_Types].xml`、`_rels/*.rels`、部件表。
//!
//! 这一层刻意不依赖 XML 解析 crate：
//!   · 写：自己拼字符串（顺序与格式完全可控，输出可复现）；
//!   · 读：`[Content_Types].xml` 与 `.rels` 的形状极其固定，用属性扫描足够，
//!     遇到不认识的内容**保留原字节**，不猜语义。

use crate::{OpcError, Result, ZipArchive, ZipWriter};

// ---------------------------------------------------------------- XML 助手

/// 最小 XML 文本转义。文本节点与属性值共用（属性值里的 `"` 也要转）。
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 不允许这些控制字符；ECMA-376 Part 1 §22.4.2.4 要求用 _xHHHH_ 格式转义
            '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' => {
                use std::fmt::Write;
                let _ = write!(out, "_x{:04X}_", c as u32);
            }
            '_' => {
                // ECMA-376 §22.4.2.4: 避免与转义序列冲突，若原文匹配 _xHHHH_，前缀 _ 转义为 _x005F_
                if i + 6 < chars.len()
                    && chars[i + 1] == 'x'
                    && chars[i + 2].is_ascii_hexdigit()
                    && chars[i + 3].is_ascii_hexdigit()
                    && chars[i + 4].is_ascii_hexdigit()
                    && chars[i + 5].is_ascii_hexdigit()
                    && chars[i + 6] == '_'
                {
                    out.push_str("_x005F_");
                } else {
                    out.push('_');
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// 判断一段文本是否需要 `xml:space="preserve"`（首尾空白会被 XML 处理器吃掉）。
pub fn needs_space_preserve(s: &str) -> bool {
    s.starts_with([' ', '\t', '\n', '\r']) || s.ends_with([' ', '\t', '\n', '\r'])
}

// ---------------------------------------------------------------- 部件名

/// 规范化部件名：统一成 `/` 开头的绝对形式，拒绝越界路径。
pub fn normalize_part_name(name: &str) -> Result<String> {
    let n = name.replace('\\', "/");
    let body = n.trim_start_matches('/');
    if body.is_empty() {
        return Err(OpcError::BadPartName(name.to_string()));
    }
    for seg in body.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(OpcError::BadPartName(name.to_string()));
        }
    }
    Ok(format!("/{body}"))
}

/// 相对引用（`_rels` 的 Target）→ 包内部件名。v0.1 只处理包内相对路径与绝对路径。
pub fn resolve_target(source_part: &str, target: &str) -> Result<String> {
    if target.contains("://") {
        // 外部关系（超链接、外链图片）：不是包内部件
        return Err(OpcError::BadPartName(format!("外部目标不解析为部件：{target}")));
    }
    if target.starts_with('/') {
        return normalize_part_name(target);
    }
    let base = source_part.trim_start_matches('/');
    let base_dir = match base.rfind('/') {
        Some(i) => &base[..i],
        None => "",
    };
    let joined = if base_dir.is_empty() {
        target.to_string()
    } else {
        format!("{base_dir}/{target}")
    };
    // 处理 ./ 与 ../
    let mut stack: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    normalize_part_name(&stack.join("/"))
}

pub const TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
pub const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

// ---------------------------------------------------------------- Content Types

#[derive(Debug, Clone, Default)]
pub struct ContentTypes {
    /// 扩展名（小写，不含点） → 内容类型
    pub defaults: Vec<(String, String)>,
    /// 部件名（`/` 开头） → 内容类型
    pub overrides: Vec<(String, String)>,
}

impl ContentTypes {
    pub fn with_common_defaults() -> Self {
        let mut ct = Self::default();
        ct.add_default("rels", "application/vnd.openxmlformats-package.relationships+xml");
        ct.add_default("xml", "application/xml");
        ct
    }

    pub fn add_default(&mut self, ext: &str, content_type: &str) {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        if let Some(slot) = self.defaults.iter_mut().find(|(e, _)| *e == ext) {
            slot.1 = content_type.to_string();
        } else {
            self.defaults.push((ext, content_type.to_string()));
        }
    }

    pub fn add_override(&mut self, part_name: &str, content_type: &str) -> Result<()> {
        let name = normalize_part_name(part_name)?;
        if let Some(slot) = self.overrides.iter_mut().find(|(p, _)| *p == name) {
            slot.1 = content_type.to_string();
        } else {
            self.overrides.push((name, content_type.to_string()));
        }
        Ok(())
    }

    pub fn to_xml(&self) -> String {
        let mut s = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        );
        s.push_str(
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        );
        for (ext, ct) in &self.defaults {
            s.push_str(&format!(
                r#"<Default Extension="{}" ContentType="{}"/>"#,
                xml_escape(ext),
                xml_escape(ct)
            ));
        }
        for (part, ct) in &self.overrides {
            s.push_str(&format!(
                r#"<Override PartName="{}" ContentType="{}"/>"#,
                xml_escape(part),
                xml_escape(ct)
            ));
        }
        s.push_str("</Types>");
        s
    }

    pub fn parse(xml: &str) -> Result<Self> {
        let mut ct = Self::default();
        let doc = roxmltree::Document::parse(xml)
            .map_err(|e| OpcError::Xml(format!("ContentTypes XML 语法错误: {e}")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "Types" {
            return Err(OpcError::Xml(format!(
                "ContentTypes 根元素名不合法，期望 <Types>，实际为 <{}>",
                root.tag_name().name()
            )));
        }
        if root.tag_name().namespace() != Some(TYPES_NS) {
            return Err(OpcError::Xml(format!(
                "ContentTypes 根元素命名空间不合法，期望 {TYPES_NS}，实际为 {:?}",
                root.tag_name().namespace()
            )));
        }
        for node in root.children().filter(|n| n.is_element()) {
            let name = node.tag_name().name();
            let ns = node.tag_name().namespace();
            if ns != Some(TYPES_NS) {
                return Err(OpcError::Xml(format!(
                    "ContentTypes 子元素 <{name}> 命名空间不合法或缺失: 期望 {TYPES_NS}，实际为 {ns:?}"
                )));
            }
            if name == "Default" {
                let ext = node.attribute("Extension").ok_or_else(|| {
                    OpcError::Xml("Default 元素缺少必需的 Extension 属性".into())
                })?;
                let ty = node.attribute("ContentType").ok_or_else(|| {
                    OpcError::Xml("Default 元素缺少必需的 ContentType 属性".into())
                })?;
                ct.add_default(ext, ty);
            } else if name == "Override" {
                let part = node.attribute("PartName").ok_or_else(|| {
                    OpcError::Xml("Override 元素缺少必需的 PartName 属性".into())
                })?;
                let ty = node.attribute("ContentType").ok_or_else(|| {
                    OpcError::Xml("Override 元素缺少必需的 ContentType 属性".into())
                })?;
                ct.add_override(part, ty)?;
            } else {
                return Err(OpcError::Xml(format!(
                    "ContentTypes 包含不支持或非法的子元素 <{name}>"
                )));
            }
        }
        Ok(ct)
    }

    /// 按 OPC 规则求某个部件的内容类型：Override 优先，其次扩展名 Default。
    pub fn content_type_for(&self, part_name: &str) -> Option<&str> {
        let name = normalize_part_name(part_name).ok()?;
        if let Some((_, ct)) = self.overrides.iter().find(|(p, _)| *p == name) {
            return Some(ct.as_str());
        }
        let ext = name.rsplit('.').next()?.to_ascii_lowercase();
        self.defaults
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, ct)| ct.as_str())
    }
}

// ---------------------------------------------------------------- Relationships

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    /// 关系类型全 URI，例如
    /// `http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument`
    pub ty: String,
    /// 原始 Target 字符串（可能相对、可能外部）
    pub target: String,
    /// `TargetMode="External"` 时存在
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Relationships {
    pub items: Vec<Relationship>,
}

impl Relationships {
    pub fn add(&mut self, id: &str, ty: &str, target: &str) -> &mut Self {
        self.items.push(Relationship {
            id: id.to_string(),
            ty: ty.to_string(),
            target: target.to_string(),
            mode: None,
        });
        self
    }

    pub fn add_external(&mut self, id: &str, ty: &str, target: &str) -> &mut Self {
        self.items.push(Relationship {
            id: id.to_string(),
            ty: ty.to_string(),
            target: target.to_string(),
            mode: Some("External".to_string()),
        });
        self
    }

    pub fn get(&self, id: &str) -> Option<&Relationship> {
        self.items.iter().find(|r| r.id == id)
    }

    pub fn by_type(&self, ty: &str) -> Option<&Relationship> {
        self.items.iter().find(|r| r.ty == ty)
    }

    /// 下一个可用的 `rIdN`（不复用已存在的号，避免和外部文件里保留的关系撞号）。
    pub fn next_id(&self) -> String {
        let mut n = 1usize;
        loop {
            let id = format!("rId{n}");
            if self.get(&id).is_none() {
                return id;
            }
            n += 1;
        }
    }

    pub fn to_xml(&self) -> String {
        let mut s = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
        s.push_str(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        for r in &self.items {
            match &r.mode {
                Some(m) => s.push_str(&format!(
                    r#"<Relationship Id="{}" Type="{}" Target="{}" TargetMode="{}"/>"#,
                    xml_escape(&r.id),
                    xml_escape(&r.ty),
                    xml_escape(&r.target),
                    xml_escape(m)
                )),
                None => s.push_str(&format!(
                    r#"<Relationship Id="{}" Type="{}" Target="{}"/>"#,
                    xml_escape(&r.id),
                    xml_escape(&r.ty),
                    xml_escape(&r.target)
                )),
            }
        }
        s.push_str("</Relationships>");
        s
    }

    pub fn parse(xml: &str) -> Result<Self> {
        let mut rels = Self::default();
        let doc = roxmltree::Document::parse(xml)
            .map_err(|e| OpcError::Xml(format!("Relationships XML 语法错误: {e}")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "Relationships" {
            return Err(OpcError::Xml(format!(
                "Relationships 根元素名不合法，期望 <Relationships>，实际为 <{}>",
                root.tag_name().name()
            )));
        }
        if root.tag_name().namespace() != Some(RELS_NS) {
            return Err(OpcError::Xml(format!(
                "Relationships 根元素命名空间不合法，期望 {RELS_NS}，实际为 {:?}",
                root.tag_name().namespace()
            )));
        }
        for node in root.children().filter(|n| n.is_element()) {
            let name = node.tag_name().name();
            let ns = node.tag_name().namespace();
            if ns != Some(RELS_NS) {
                return Err(OpcError::Xml(format!(
                    "Relationships 子元素 <{name}> 命名空间不合法或缺失: 期望 {RELS_NS}，实际为 {ns:?}"
                )));
            }
            if name == "Relationship" {
                let id = node.attribute("Id").ok_or_else(|| {
                    OpcError::Xml("Relationship 元素缺少必需的 Id 属性".into())
                })?;
                let ty = node.attribute("Type").ok_or_else(|| {
                    OpcError::Xml("Relationship 元素缺少必需的 Type 属性".into())
                })?;
                let target = node.attribute("Target").ok_or_else(|| {
                    OpcError::Xml("Relationship 元素缺少必需的 Target 属性".into())
                })?;
                let mode = node.attribute("TargetMode").map(|s| s.to_string());
                rels.items.push(Relationship {
                    id: id.to_string(),
                    ty: ty.to_string(),
                    target: target.to_string(),
                    mode,
                });
            } else {
                return Err(OpcError::Xml(format!(
                    "Relationships 包含不支持或非法的子元素 <{name}>"
                )));
            }
        }
        Ok(rels)
    }
}

// ---------------------------------------------------------------- Package

#[derive(Debug, Clone)]
pub struct Part {
    /// `/` 开头的包内部件名，例如 `/word/document.xml`
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Package {
    pub content_types: ContentTypes,
    pub parts: Vec<Part>,
}

/// 包根关系部件名（OPC 固定）
pub const ROOT_RELS: &str = "/_rels/.rels";
pub const CONTENT_TYPES: &str = "/[Content_Types].xml";

impl Package {
    pub fn with_common_defaults() -> Self {
        Self { content_types: ContentTypes::with_common_defaults(), parts: Vec::new() }
    }

    pub fn put(&mut self, name: &str, data: impl Into<Vec<u8>>) -> Result<()> {
        let name = normalize_part_name(name)?;
        let data = data.into();
        if let Some(slot) = self.parts.iter_mut().find(|p| p.name == name) {
            slot.data = data;
        } else {
            self.parts.push(Part { name, data });
        }
        Ok(())
    }

    pub fn put_text(&mut self, name: &str, text: impl AsRef<str>) -> Result<()> {
        self.put(name, text.as_ref().as_bytes().to_vec())
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let name = normalize_part_name(name).ok()?;
        self.parts.iter().find(|p| p.name == name).map(|p| p.data.as_slice())
    }

    pub fn get_text(&self, name: &str) -> Option<String> {
        self.get(name).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn part_names(&self) -> Vec<&str> {
        self.parts.iter().map(|p| p.name.as_str()).collect()
    }

    /// 读某个部件自己的关系（`/a/b.xml` → `/a/_rels/b.xml.rels`）。
    /// 如果关系部件存在但 XML 畸形，显式返回 Err(OpcError::Xml)。
    pub fn try_rels_for(&self, part_name: &str) -> Result<Relationships> {
        match rels_part_for(part_name) {
            Some(p) => match self.get(&p) {
                Some(bytes) => {
                    let text = std::str::from_utf8(bytes)
                        .map_err(|e| OpcError::Xml(format!("关系部件 {p} 非有效 UTF-8: {e}")))?;
                    Relationships::parse(text).map_err(|e| match e {
                        OpcError::Xml(msg) => OpcError::Xml(format!("关系部件 {p} 解析失败: {msg}")),
                        other => other,
                    })
                }
                None => Ok(Relationships::default()),
            },
            None => Ok(Relationships::default()),
        }
    }

    pub fn rels_for(&self, part_name: &str) -> Result<Relationships> {
        self.try_rels_for(part_name)
    }

    pub fn set_rels_for(&mut self, part_name: &str, rels: &Relationships) -> Result<()> {
        let p = rels_part_for(part_name)
            .ok_or_else(|| OpcError::BadPartName(format!("无法为 {part_name} 推导关系部件名")))?;
        self.put_text(&p, rels.to_xml())
    }

    pub fn try_root_rels(&self) -> Result<Relationships> {
        match self.get(ROOT_RELS) {
            Some(bytes) => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|e| OpcError::Xml(format!("根关系部件 {ROOT_RELS} 非有效 UTF-8: {e}")))?;
                Relationships::parse(text).map_err(|e| match e {
                    OpcError::Xml(msg) => OpcError::Xml(format!("根关系部件 {ROOT_RELS} 解析失败: {msg}")),
                    other => other,
                })
            }
            None => Ok(Relationships::default()),
        }
    }

    pub fn root_rels(&self) -> Result<Relationships> {
        self.try_root_rels()
    }

    pub fn set_root_rels(&mut self, rels: &Relationships) -> Result<()> {
        self.put_text(ROOT_RELS, rels.to_xml())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut w = ZipWriter::new();
        // [Content_Types].xml 放最前：多数读取器都期望它先出现
        w.add("[Content_Types].xml", self.content_types.to_xml().into_bytes());
        for p in &self.parts {
            if p.name == CONTENT_TYPES {
                continue;
            }
            w.add(p.name.trim_start_matches('/'), p.data.clone());
        }
        w.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let archive = ZipArchive::read(bytes)?;
        Self::from_archive(&archive)
    }

    /// 从已读出的 ZIP 归档装配包（调用方可能还要用 CRC 报告）。
    pub fn from_archive(archive: &ZipArchive) -> Result<Self> {
        let ct_xml = archive
            .get("[Content_Types].xml")
            .ok_or_else(|| OpcError::BadZip("缺少 [Content_Types].xml".into()))?;
        let ct_str = std::str::from_utf8(ct_xml)
            .map_err(|e| OpcError::Xml(format!("[Content_Types].xml 非 UTF-8 编码: {e}")))?;
        let content_types = ContentTypes::parse(ct_str)?;

        let mut parts = Vec::with_capacity(archive.entries.len());
        for e in &archive.entries {
            if !e.crc_ok {
                return Err(OpcError::CrcMismatch {
                    name: e.name.clone(),
                    expected: e.crc32,
                    actual: 0,
                });
            }
            // [Content_Types].xml 单独存放；目录条目（以 / 结尾）不是部件
            if e.name == "[Content_Types].xml" || e.name.ends_with('/') {
                continue;
            }
            parts.push(Part { name: format!("/{}", e.name), data: e.data.clone() });
        }
        Ok(Self { content_types, parts })
    }
}

/// `/word/document.xml` → `/word/_rels/document.xml.rels`；根部件 → `/_rels/.rels`
pub fn rels_part_for(part_name: &str) -> Option<String> {
    let name = normalize_part_name(part_name).ok()?;
    let body = name.trim_start_matches('/');
    if body == ".rels" {
        return Some(ROOT_RELS.to_string());
    }
    match body.rfind('/') {
        Some(i) => Some(format!("/{}/_rels/{}.rels", &body[..i], &body[i + 1..])),
        None => Some(format!("/_rels/{body}.rels")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_and_preserve() {
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert!(needs_space_preserve(" 前导空格"));
        assert!(!needs_space_preserve("正常"));
    }

    #[test]
    fn content_types_roundtrip() {
        let mut ct = ContentTypes::with_common_defaults();
        ct.add_override("/word/document.xml", "application/vnd.ms-word.document.main+xml")
            .unwrap();
        let xml = ct.to_xml();
        let back = ContentTypes::parse(&xml).unwrap();
        assert_eq!(
            back.content_type_for("/word/document.xml"),
            Some("application/vnd.ms-word.document.main+xml")
        );
        assert_eq!(back.content_type_for("/word/styles.xml"), Some("application/xml"));
    }

    #[test]
    fn rels_with_newlines_and_comments_parse_correctly() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- 这是一个关系部件注释 -->
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship
    Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide"
    Target="slides/slide1.xml"
  />
</Relationships>"#;
        let rels = Relationships::parse(xml).unwrap();
        assert_eq!(rels.items.len(), 1);
        assert_eq!(rels.items[0].id, "rId1");
        assert_eq!(rels.items[0].target, "slides/slide1.xml");
    }

    #[test]
    fn test_escape_control_chars_and_literal_underscore() {
        assert_eq!(xml_escape("a\u{1}b"), "a_x0001_b");
        assert_eq!(xml_escape("a_x0001_b"), "a_x005F_x0001_b");
        assert_eq!(xml_escape("normal_underscore"), "normal_underscore");
    }

    #[test]
    fn rels_roundtrip_and_external_mode() {
        let mut rels = Relationships::default();
        rels.add("rId1", "urn:officeDocument", "word/document.xml");
        rels.add_external("rId2", "urn:hyperlink", "https://example.com/a?b=1&c=2");
        let xml = rels.to_xml();
        let back = Relationships::parse(&xml).unwrap();
        assert_eq!(back.items.len(), 2);
        assert_eq!(back.get("rId2").unwrap().mode.as_deref(), Some("External"));
        assert_eq!(back.get("rId2").unwrap().target, "https://example.com/a?b=1&c=2");
        assert_eq!(back.next_id(), "rId3");
    }

    #[test]
    fn part_name_and_target_resolution() {
        assert_eq!(normalize_part_name("word/document.xml").unwrap(), "/word/document.xml");
        assert!(normalize_part_name("../escape.xml").is_err());
        assert_eq!(
            resolve_target("/word/document.xml", "styles.xml").unwrap(),
            "/word/styles.xml"
        );
        assert_eq!(
            resolve_target("/word/document.xml", "media/image1.png").unwrap(),
            "/word/media/image1.png"
        );
        assert_eq!(resolve_target("/word/document.xml", "/docProps/core.xml").unwrap(), "/docProps/core.xml");
        assert!(resolve_target("/word/document.xml", "https://x/y").is_err());
    }

    #[test]
    fn rels_part_naming() {
        assert_eq!(rels_part_for("/word/document.xml").unwrap(), "/word/_rels/document.xml.rels");
        assert_eq!(rels_part_for("/document.xml").unwrap(), "/_rels/document.xml.rels");
    }

    #[test]
    fn directory_entries_are_not_parts() {
        // 真实 WPS 包里有 /word/ 这类目录条目；它们不是 OPC 部件
        let mut w = ZipWriter::new();
        w.add(
            "[Content_Types].xml",
            format!(r#"<Types xmlns="{TYPES_NS}"/>"#).into_bytes(),
        );
        w.add("word/", Vec::new());
        w.add("word/document.xml", b"<d/>".to_vec());
        let archive = ZipArchive::read(&w.finish().unwrap()).unwrap();
        let pkg = Package::from_archive(&archive).unwrap();
        assert_eq!(pkg.part_names(), vec!["/word/document.xml"]);
    }

    #[test]
    fn package_roundtrip_keeps_unknown_parts() {
        let mut pkg = Package::with_common_defaults();
        pkg.put_text("/word/document.xml", "<d/>").unwrap();
        pkg.put("/word/unknown.bin", vec![1u8, 2, 3, 4]).unwrap();
        let mut rels = Relationships::default();
        rels.add("rId1", "urn:officeDocument", "word/document.xml");
        pkg.set_root_rels(&rels).unwrap();

        let bytes = pkg.to_bytes().unwrap();
        let back = Package::from_bytes(&bytes).unwrap();
        assert_eq!(back.get("/word/unknown.bin").unwrap(), &[1, 2, 3, 4]);
        assert_eq!(back.root_rels().unwrap().get("rId1").unwrap().target, "word/document.xml");
        assert!(!back.has("/[Content_Types].xml"), "Content Types 单独存放，不重复计入部件");
    }

    #[test]
    fn test_malformed_xml_returns_explicit_error() {
        // 1. ContentTypes 畸形 / 截断 / 错误命名空间 / 缺失属性 必须显式报错拒绝，严禁静默返回空
        assert!(ContentTypes::parse("<Types><unclosed").is_err());
        assert!(ContentTypes::parse(&format!(r#"<WrongRoot xmlns="{TYPES_NS}"/>"#)).is_err());
        assert!(ContentTypes::parse(r#"<Types xmlns="http://wrong.namespace/types"/>"#).is_err());
        assert!(ContentTypes::parse(&format!(r#"<Types xmlns="{TYPES_NS}"><Default Extension="xml"/></Types>"#)).is_err()); // 缺少 ContentType
        assert!(ContentTypes::parse(&format!(r#"<Types xmlns="{TYPES_NS}"><Override PartName="/doc.xml"/></Types>"#)).is_err()); // 缺少 ContentType
        assert!(ContentTypes::parse(&format!(r#"<Types xmlns="{TYPES_NS}"><IllegalTag/></Types>"#)).is_err());
        // 测试 xmlns="" 清除子元素命名空间必须被严格拒绝
        assert!(ContentTypes::parse(&format!(r#"<Types xmlns="{TYPES_NS}"><Default xmlns="" Extension="xml" ContentType="application/xml"/></Types>"#)).is_err());
        assert!(ContentTypes::parse(&format!(r#"<Types xmlns="{TYPES_NS}"><Override xmlns="http://wrong.ns" PartName="/doc.xml" ContentType="application/xml"/></Types>"#)).is_err());

        // 2. Relationships 畸形 / 截断 / 错误命名空间 / 缺失属性 必须显式报错拒绝，严禁静默返回空
        assert!(Relationships::parse("<Relationships><Relationship id=").is_err());
        assert!(Relationships::parse(&format!(r#"<WrongRoot xmlns="{RELS_NS}"/>"#)).is_err());
        assert!(Relationships::parse(r#"<Relationships xmlns="http://wrong.namespace/rels"/>"#).is_err());
        assert!(Relationships::parse(&format!(r#"<Relationships xmlns="{RELS_NS}"><Relationship Id="rId1" Type="foo"/></Relationships>"#)).is_err()); // 缺少 Target
        assert!(Relationships::parse(&format!(r#"<Relationships xmlns="{RELS_NS}"><UnknownNode/></Relationships>"#)).is_err());
        // 测试 xmlns="" 清除子元素命名空间必须被严格拒绝
        assert!(Relationships::parse(&format!(r#"<Relationships xmlns="{RELS_NS}"><Relationship xmlns="" Id="rId1" Type="type" Target="target"/></Relationships>"#)).is_err());
        assert!(Relationships::parse(&format!(r#"<Relationships xmlns="{RELS_NS}"><Relationship xmlns="http://wrong.ns" Id="rId1" Type="type" Target="target"/></Relationships>"#)).is_err());

        // 3. Package 从含有畸形 ContentTypes 的归档加载时必须拒绝
        let mut w = ZipWriter::new();
        w.add("[Content_Types].xml", b"<Types><broken".to_vec());
        let archive = ZipArchive::read(&w.finish().unwrap()).unwrap();
        let pkg_err = Package::from_archive(&archive).unwrap_err();
        match pkg_err {
            OpcError::Xml(_) => {}
            other => panic!("Expected OpcError::Xml, got {other:?}"),
        }

        // 4. Package try_root_rels / try_rels_for 遇到坏关系必须返回 Err(Xml)
        let mut pkg = Package::with_common_defaults();
        pkg.put_text("/_rels/.rels", "<Relationships><broken").unwrap();
        let root_err = pkg.try_root_rels().unwrap_err();
        match root_err {
            OpcError::Xml(msg) => assert!(msg.contains("解析失败") || msg.contains("语法错误"), "{msg}"),
            other => panic!("Expected OpcError::Xml, got {other:?}"),
        }
    }
}
