//! 输入上限。
//!
//! 一篇文档、一页正文都是整份读进内存再解析的；没有上限时，一个误放的
//! 大文件（日志、数据库导出）会让进程直接把内存吃光。这个数不是语法限制，
//! 只是一个「显然不像一页文档」的闸门：真需要更大的页，改这一处即可。
//!
//! 输出不设上限 —— 产物大小由正文决定，限制它只会把问题推到更难解释的地方。

use std::io::Read;
use std::path::Path;

/// 单份输入的字节上限（32 MiB）。
pub const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

/// 读一个文本文件，超过上限就拒绝。
pub fn read_text(path: &Path) -> Result<String, String> {
    let meta =
        std::fs::metadata(path).map_err(|error| format!("读不了 {}：{error}", path.display()))?;
    if meta.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "{} 太大了（{} 字节，单份输入上限 {} 字节）",
            path.display(),
            meta.len(),
            MAX_SOURCE_BYTES
        ));
    }
    std::fs::read_to_string(path).map_err(|error| format!("读不了 {}：{error}", path.display()))
}

/// 从标准输入读，超过上限就拒绝。
pub fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("读标准输入失败：{error}"))?;
    if text.len() as u64 > MAX_SOURCE_BYTES {
        return Err(format!(
            "标准输入太大了（超过单份输入上限 {} 字节）",
            MAX_SOURCE_BYTES
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 边界内正常读；超过上限拒绝，且报错里带着路径
    #[test]
    fn a_file_over_the_limit_is_refused() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let small = dir.path().join("small.mdpp");
        std::fs::write(&small, "小").expect("写得下");
        assert_eq!(read_text(&small).expect("读得动"), "小");

        let big = dir.path().join("big.mdpp");
        let file = std::fs::File::create(&big).expect("建得出");
        file.set_len(MAX_SOURCE_BYTES + 1).expect("撑得大");
        let error = read_text(&big).expect_err("该拒绝");
        assert!(error.contains("big.mdpp"), "{error}");
        assert!(error.contains("上限"), "{error}");
    }
}
