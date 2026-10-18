//! CRC32（IEEE 802.3，ZIP 用的那个多项式）。
//!
//! 自己实现而不是借 `flate2::Crc`：写入路径因此不依赖任何 crate，
//! 且算法有独立测试向量（"123456789" → 0xCBF43926）。

const POLY: u32 = 0xEDB8_8320;

fn table() -> &'static [u32; 256] {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        let mut i = 0usize;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { POLY ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    })
}

/// 计算一段字节的 CRC32。
pub fn crc32(data: &[u8]) -> u32 {
    let t = table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn known_vectors() {
        // ZIP/PNG 通用测试向量
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
    }

    #[test]
    fn utf8_payload_is_byte_exact() {
        // 中文必须按 UTF-8 字节算，不能先转 Unicode 码点
        let s = "表格";
        assert_eq!(crc32(s.as_bytes()), crc32(&[0xE8, 0xA1, 0xA8, 0xE6, 0xA0, 0xBC]));
    }
}
