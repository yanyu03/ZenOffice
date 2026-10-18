//! ZIP 容器读写（OPC 包就是 ZIP）。
//!
//! 写入：本地文件头 + 数据 + 中央目录 + EOCD；默认 deflate，压不动就回落 store。
//!       时间戳固定为 1980-01-01 00:00:00，让同样的输入产出**逐字节相同**的包
//!       （round-trip 测试与 diff 都需要这个性质）。
//! 读取：以中央目录为准（本地头在带 data descriptor 时尺寸为 0），逐条校验 CRC。
//!       结构损坏 → 报错；CRC 不符 → 记录在条目上，不丢已经读到的内容。

use crate::crc32::crc32;
use crate::{OpcError, Result};
use std::io::Write;

const SIG_LOCAL: u32 = 0x0403_4B50;
const SIG_CENTRAL: u32 = 0x0201_4B50;
const SIG_EOCD: u32 = 0x0605_4B50;

const METHOD_STORE: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

/// 1980-01-01 00:00:00（DOS 时间戳的最小合法值），保证输出可复现。
const DOS_TIME: u16 = 0x0000;
const DOS_DATE: u16 = 0x0021;

#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    /// 解压后的内容；CRC 不符时依然保留，并在 `crc_ok=false` 标记。
    pub data: Vec<u8>,
    pub crc_ok: bool,
}

#[derive(Debug, Default)]
pub struct ZipWriter {
    entries: Vec<(String, Vec<u8>)>,
    compress: bool,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self { entries: Vec::new(), compress: true }
    }

    /// 全部条目都按 store 写入（调试或需要肉眼可读时用）。
    pub fn store_only() -> Self {
        Self { entries: Vec::new(), compress: false }
    }

    pub fn add(&mut self, name: impl Into<String>, data: impl Into<Vec<u8>>) {
        self.entries.push((name.into(), data.into()));
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|(n, _)| n == name)
    }

    pub fn finish(self) -> Result<Vec<u8>> {
        if self.entries.len() > 0xFFFF - 1 {
            return Err(OpcError::Zip64Unsupported("条目数超过传统 ZIP 上限 (65534)".into()));
        }

        let mut out = Vec::new();
        let mut central = Vec::new();

        for (name, raw) in &self.entries {
            let name_bytes = name.as_bytes();
            if name_bytes.len() > u16::MAX as usize {
                return Err(OpcError::BadZip(format!("条目名称过长: {name}")));
            }
            if raw.len() > u32::MAX as usize {
                return Err(OpcError::Zip64Unsupported(format!("条目 {name} 尺寸超过 4GB，需要 ZIP64 支持")));
            }

            let crc = crc32(raw);

            let (method, payload) = if self.compress {
                let mut enc = flate2::write::DeflateEncoder::new(
                    Vec::new(),
                    flate2::Compression::new(6),
                );
                enc.write_all(raw)?;
                let compressed = enc.finish()?;
                if compressed.len() < raw.len() {
                    (METHOD_DEFLATE, compressed)
                } else {
                    (METHOD_STORE, raw.clone())
                }
            } else {
                (METHOD_STORE, raw.clone())
            };

            if payload.len() > u32::MAX as usize {
                return Err(OpcError::Zip64Unsupported(format!("条目 {name} 压缩尺寸超过 4GB，需要 ZIP64 支持")));
            }

            if out.len() > u32::MAX as usize {
                return Err(OpcError::Zip64Unsupported("本地数据段累计偏移超过 4GB，需要 ZIP64 支持".into()));
            }
            let offset = out.len() as u32;

            // ---- 本地文件头
            out.extend_from_slice(&SIG_LOCAL.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&DOS_TIME.to_le_bytes());
            out.extend_from_slice(&DOS_DATE.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(&payload);

            // ---- 中央目录项
            central.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // version made by
            central.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central.extend_from_slice(&0u16.to_le_bytes()); // flags
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&DOS_TIME.to_le_bytes());
            central.extend_from_slice(&DOS_DATE.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            central.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra
            central.extend_from_slice(&0u16.to_le_bytes()); // comment
            central.extend_from_slice(&0u16.to_le_bytes()); // disk start
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        if central.len() > u32::MAX as usize || out.len() > u32::MAX as usize {
            return Err(OpcError::Zip64Unsupported("中央目录尺寸或数据总偏移超过 4GB，需要 ZIP64 支持".into()));
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);

        // ---- EOCD
        out.extend_from_slice(&SIG_EOCD.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        out.extend_from_slice(&0u16.to_le_bytes()); // disk with cd
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        Ok(out)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ZipArchive {
    pub entries: Vec<ZipEntry>,
}

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| OpcError::BadZip(format!("在偏移 {at} 读 u16 越界")))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| OpcError::BadZip(format!("在偏移 {at} 读 u32 越界")))
}

impl ZipArchive {
    /// 默认安全解析模式：任何条目 CRC 不匹配直接拒绝返回 Err(OpcError::CrcMismatch)
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Self::read_internal(bytes, true)
    }

    /// 宽松诊断模式：保留损坏条目并在 ZipEntry 标记 crc_ok: false，供体检与诊断器读取损坏详情
    pub fn read_lenient(bytes: &[u8]) -> Result<Self> {
        Self::read_internal(bytes, false)
    }

    fn read_internal(bytes: &[u8], strict_crc: bool) -> Result<Self> {
        if bytes.len() < 22 {
            return Err(OpcError::BadZip(format!("长度 {} 不足以容纳 EOCD", bytes.len())));
        }

        // 先认"根本不是 ZIP"的两种常见情况，给出能指导下一步的错误，
        // 而不是让人对着"找不到 EOCD"猜。
        //   OLE2（D0 CF 11 E0 A1 B1 1A E1）= 旧版二进制 .doc/.xls/.ppt 复合文件；
        //   实测 WPS 12.0 的 SaveAs(path, 16) 会写出这种文件，即使扩展名是 .docx。
        if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
            return Err(OpcError::NotAZip(
                "这是 OLE2 复合文件（旧版二进制 .doc/.xls/.ppt），不是 OOXML 包；\
                 请用 OOXML 格式另存（WPS 的 SaveAs 格式常量 16 会落成 OLE2，需用 12）"
                    .into(),
            ));
        }
        if !bytes.starts_with(b"PK") {
            let head: String = bytes
                .iter()
                .take(4)
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            return Err(OpcError::NotAZip(format!("文件头不是 PK（实际 {head}）")));
        }

        // 从尾部向前找 EOCD（注释最长 65535，故搜索窗口 = 22 + 65535）
        let window = bytes.len().saturating_sub(22 + 65_535);
        let mut eocd = None;
        let mut i = bytes.len() - 22;
        loop {
            if u32_at(bytes, i)? == SIG_EOCD {
                eocd = Some(i);
                break;
            }
            if i == window {
                break;
            }
            i -= 1;
        }
        let eocd = eocd.ok_or_else(|| OpcError::BadZip("找不到 EOCD 记录".into()))?;

        let count = u16_at(bytes, eocd + 10)? as usize;
        let cd_size = u32_at(bytes, eocd + 12)? as usize;
        let cd_offset = u32_at(bytes, eocd + 16)? as usize;
        if count == u16::MAX as usize || cd_offset == u32::MAX as usize || cd_size == u32::MAX as usize {
            return Err(OpcError::Zip64Unsupported("条目数或中央目录偏移/长度使用了 ZIP64 哨兵值".into()));
        }

        // RT-01: 解压资源安全配额 (防 ZIP 炸弹与无界解压耗尽系统内存)
        const MAX_ENTRY_UNCOMP_BYTES: usize = 64 * 1024 * 1024; // 单条目上限 64 MB
        const MAX_TOTAL_UNCOMP_BYTES: usize = 256 * 1024 * 1024; // 总解压上限 256 MB
        const MAX_COMPRESSION_RATIO: usize = 100; // 最大压缩比 100:1 (高于此阈值视为炸弹)

        let mut total_uncompressed_accum: usize = 0;

        let mut entries = Vec::with_capacity(count);
        let mut p = cd_offset;
        for idx in 0..count {
            if u32_at(bytes, p)? != SIG_CENTRAL {
                return Err(OpcError::BadZip(format!("第 {idx} 个中央目录项签名不对（偏移 {p}）")));
            }
            let method = u16_at(bytes, p + 10)?;
            let crc = u32_at(bytes, p + 16)?;
            let comp_size = u32_at(bytes, p + 20)? as usize;
            let uncomp_size = u32_at(bytes, p + 24)? as usize;
            let name_len = u16_at(bytes, p + 28)? as usize;
            let extra_len = u16_at(bytes, p + 30)? as usize;
            let comment_len = u16_at(bytes, p + 32)? as usize;
            let local_off = u32_at(bytes, p + 42)? as usize;
            let name = String::from_utf8_lossy(
                bytes
                    .get(p + 46..p + 46 + name_len)
                    .ok_or_else(|| OpcError::BadZip("中央目录项名字越界".into()))?,
            )
            .into_owned();

            if comp_size == u32::MAX as usize || uncomp_size == u32::MAX as usize || local_off == u32::MAX as usize {
                return Err(OpcError::Zip64Unsupported(format!("条目 {name} 使用了 ZIP64 尺寸或偏移")));
            }

            // RT-01: 校验条目尺寸硬限制
            if uncomp_size > MAX_ENTRY_UNCOMP_BYTES {
                return Err(OpcError::ResourceLimitExceeded(format!(
                    "条目 {name} 声明解压尺寸 {uncomp_size} 字节超过单条目配额 {MAX_ENTRY_UNCOMP_BYTES} 字节"
                )));
            }
            if comp_size > 0 && (uncomp_size / comp_size) > MAX_COMPRESSION_RATIO && uncomp_size > 1024 * 1024 {
                return Err(OpcError::ResourceLimitExceeded(format!(
                    "条目 {name} 压缩比超过 {MAX_COMPRESSION_RATIO}:1 安全阈值 (疑似 ZIP 炸弹)"
                )));
            }

            total_uncompressed_accum += uncomp_size;
            if total_uncompressed_accum > MAX_TOTAL_UNCOMP_BYTES {
                return Err(OpcError::ResourceLimitExceeded(format!(
                    "压缩包累计解压尺寸超过配额 {MAX_TOTAL_UNCOMP_BYTES} 字节"
                )));
            }

            // 本地头：只为拿到数据起点（尺寸一律以中央目录为准）
            if u32_at(bytes, local_off)? != SIG_LOCAL {
                return Err(OpcError::BadZip(format!("条目 {name} 的本地头签名不对")));
            }
            let l_name_len = u16_at(bytes, local_off + 26)? as usize;
            let l_extra_len = u16_at(bytes, local_off + 28)? as usize;
            let data_start = local_off + 30 + l_name_len + l_extra_len;
            let data_end = data_start + comp_size;
            let raw = bytes
                .get(data_start..data_end)
                .ok_or_else(|| OpcError::BadZip(format!("条目 {name} 的数据段越界")))?;

            let data = match method {
                METHOD_STORE => raw.to_vec(),
                METHOD_DEFLATE => {
                    use std::io::Read;
                    let mut dec = flate2::read::DeflateDecoder::new(raw);
                    let mut out = Vec::with_capacity(uncomp_size.min(1024 * 1024));
                    // 限制最大读取长度，防止实际解压长度超过声明值
                    let mut limited = (&mut dec).take((uncomp_size + 1) as u64);
                    limited.read_to_end(&mut out)
                        .map_err(|e| OpcError::Inflate { name: name.clone(), source: e })?;
                    if out.len() != uncomp_size {
                        return Err(OpcError::BadZip(format!(
                            "条目 {name} 实际解压尺寸 {} 与中央目录声明值 {uncomp_size} 不一致",
                            out.len()
                        )));
                    }
                    out
                }
                other => return Err(OpcError::UnsupportedMethod(other)),
            };

            let actual = crc32(&data);
            if strict_crc && actual != crc {
                return Err(OpcError::CrcMismatch {
                    name,
                    expected: crc,
                    actual,
                });
            }
            entries.push(ZipEntry {
                name,
                method,
                crc32: crc,
                compressed_size: comp_size as u32,
                uncompressed_size: uncomp_size as u32,
                data,
                crc_ok: actual == crc,
            });

            p += 46 + name_len + extra_len + comment_len;
        }

        Ok(Self { entries })
    }

    pub fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.name == name).map(|e| e.data.as_slice())
    }

    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// 逐条 CRC 结果，供上层写成体检报告。
    pub fn crc_report(&self) -> Vec<(String, bool)> {
        self.entries.iter().map(|e| (e.name.clone(), e.crc_ok)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrip() {
        let mut w = ZipWriter::new();
        w.add("[Content_Types].xml", b"<Types/>".to_vec());
        w.add("word/document.xml", "<w:document>正文</w:document>".as_bytes().to_vec());
        let bytes = w.finish().unwrap();

        let a = ZipArchive::read(&bytes).unwrap();
        assert_eq!(a.names(), vec!["[Content_Types].xml", "word/document.xml"]);
        assert_eq!(a.get("word/document.xml").unwrap(), "<w:document>正文</w:document>".as_bytes());
        assert!(a.crc_report().iter().all(|(_, ok)| *ok));
    }

    #[test]
    fn output_is_deterministic() {
        let build = || {
            let mut w = ZipWriter::new();
            w.add("a.xml", b"hello hello hello hello".to_vec());
            w.finish().unwrap()
        };
        assert_eq!(build(), build(), "同样输入必须产出逐字节相同的包");
    }

    #[test]
    fn store_only_mode_is_readable() {
        let mut w = ZipWriter::store_only();
        w.add("a.xml", b"abcd".to_vec());
        let bytes = w.finish().unwrap();
        let a = ZipArchive::read(&bytes).unwrap();
        assert_eq!(a.entry("a.xml").unwrap().method, METHOD_STORE);
        assert_eq!(a.get("a.xml").unwrap(), b"abcd");
    }

    #[test]
    fn corrupt_data_byte_is_detected_not_silently_accepted() {
        // 用 store 模式，数据段位置可精确算出：本地头 30 字节 + 文件名
        let name = "a.xml";
        let payload = b"0123456789".repeat(20);
        let mut w = ZipWriter::store_only();
        w.add(name, payload.clone());
        let mut bytes = w.finish().unwrap();

        let data_start = 30 + name.len();
        bytes[data_start] ^= 0xFF; // 只破坏内容，不动头部与 CRC 字段

        // 默认严格解析路径必须直接拒绝损坏数据，返回 CrcMismatch 错误
        let strict_res = ZipArchive::read(&bytes);
        assert!(matches!(strict_res, Err(OpcError::CrcMismatch { .. })), "默认读取必须拒绝 CRC 损坏包");

        // 诊断模式允许读取损坏条目并标记 crc_ok: false，供体检器提取诊断事实
        let a = ZipArchive::read_lenient(&bytes).expect("诊断模式应当能读出条目并保留部分事实");
        let e = a.entry(name).unwrap();
        assert_ne!(e.data, payload, "内容确实被改动了");
        assert!(!e.crc_ok, "CRC 必须报错，不能静默接受");
        assert!(a.crc_report().iter().any(|(_, ok)| !ok));
    }

    #[test]
    fn corrupt_deflate_stream_does_not_masquerade_as_valid() {
        let mut w = ZipWriter::new();
        w.add("a.xml", b"0123456789".repeat(200)); // 足够长，确定会走 deflate
        let bytes = w.finish().unwrap();
        let raw = ZipArchive::read(&bytes).unwrap();
        assert_eq!(raw.entry("a.xml").unwrap().method, METHOD_DEFLATE, "长内容应被压缩");

        let mut broken = bytes.clone();
        let data_start = 30 + "a.xml".len();
        broken[data_start + 4] ^= 0xFF; // 破坏 deflate 流内部

        let strict_err = ZipArchive::read(&broken).is_err();
        assert!(strict_err, "默认读取破坏的 deflate 流必须报错拒绝");

        let lenient_ok = match ZipArchive::read_lenient(&broken) {
            Err(_) => false,
            Ok(a) => a.entry("a.xml").map(|e| e.crc_ok).unwrap_or(true),
        };
        assert!(!lenient_ok, "破坏 deflate 流后即使宽松读取也不能报告为内容完好");
    }

    #[test]
    fn ole2_binary_container_is_named_explicitly() {
        // 实测：WPS 12.0 的 SaveAs(path, 16) 会写出 OLE2 二进制（扩展名却是 .docx）。
        // 这种输入必须得到"这是旧版二进制格式"的明确结论，而不是含糊的结构错误。
        let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        bytes.extend(std::iter::repeat(0u8).take(64));
        let err = ZipArchive::read(&bytes).unwrap_err();
        assert!(matches!(err, OpcError::NotAZip(_)), "得到 {err:?}");
        assert!(err.to_string().contains("OLE2"), "{err}");
    }

    #[test]
    fn truncated_input_is_an_error() {
        let mut w = ZipWriter::new();
        w.add("a.xml", b"x".repeat(100));
        let bytes = w.finish().unwrap();
        assert!(ZipArchive::read(&bytes[..10]).is_err());
    }

    #[test]
    fn test_red_team_rt01_decompression_quota_enforced() {
        // 构造一个中央目录声明超大解压尺寸 (80 MB > 64 MB 上限) 的恶意包
        let mut w = ZipWriter::store_only();
        w.add("huge.xml", b"small payload".to_vec());
        let mut bytes = w.finish().unwrap();

        // 找到中央目录中 uncompressed_size 字段 (偏移 24)，篡改为 80 MB
        // EOCD 在最后 22 字节
        let eocd_pos = bytes.len() - 22;
        let cd_offset = u32::from_le_bytes([bytes[eocd_pos+16], bytes[eocd_pos+17], bytes[eocd_pos+18], bytes[eocd_pos+19]]) as usize;
        let uncomp_size_pos = cd_offset + 24;
        let fake_size: u32 = 80 * 1024 * 1024;
        bytes[uncomp_size_pos..uncomp_size_pos+4].copy_from_slice(&fake_size.to_le_bytes());

        let res = ZipArchive::read(&bytes);
        assert!(res.is_err(), "声明 80MB 的解压必须被 ResourceLimitExceeded 拒绝");
        let err = res.unwrap_err();
        assert!(matches!(err, OpcError::ResourceLimitExceeded(_)), "得到错误: {err:?}");
    }
}
