//! 有上限的文件读取：调用方声明可接受的最大字节数，超过即报错。
//!
//! `std::fs::read` / `read_to_string` 会把整个文件载入内存；规则包、密钥、
//! 自定义规则等输入都应有明确上限，读取阶段就按上限截断，而不是读完再判断。

use std::io::Read;
use std::path::Path;

fn too_large(path: &Path, limit: u64) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::FileTooLarge,
        format!("{} 超过 {limit} 字节上限", path.display()),
    )
}

/// 最多读取 `limit` 字节；文件更大时返回 `FileTooLarge`，不把超出部分载入内存。
pub fn read_bytes_limited(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    // 元数据预检只为在明显超限时免去读取；结论以实际读取字节数为准（读取期间文件仍可能增长）。
    if file.metadata()?.len() > limit {
        return Err(too_large(path, limit));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(too_large(path, limit));
    }
    Ok(bytes)
}

/// [`read_bytes_limited`] 的文本版本；非 UTF-8 内容返回 `InvalidData`。
pub fn read_to_string_limited(path: &Path, limit: u64) -> std::io::Result<String> {
    let bytes = read_bytes_limited(path, limit)?;
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_within_limit_and_rejects_oversize() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.txt");
        std::fs::write(&path, b"hello").unwrap();

        assert_eq!(read_bytes_limited(&path, 5).unwrap(), b"hello");
        assert_eq!(read_to_string_limited(&path, 5).unwrap(), "hello");

        let error = read_bytes_limited(&path, 4).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::FileTooLarge);
        let error = read_to_string_limited(&path, 4).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::FileTooLarge);
    }

    #[test]
    fn rejects_non_utf8_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("binary.dat");
        std::fs::write(&path, [0xff, 0xfe]).unwrap();

        let error = read_to_string_limited(&path, 16).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
