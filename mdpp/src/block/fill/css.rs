//! 内容 CSS 的净化。
//!
//! `::css` 现在把样式交给整页收集器，不再套 `.mdpp-content`。这里只做两件
//! 必须做的事：丢掉 `@import`（不让正文把页面带去外部取样式），以及去掉
//! `</style`（不让一段 CSS 提前闭合样式块）。

/// CSS 里不许出现 `</style`：它会提前闭合样式块，把后面的内容顶到页面上。
///
/// 同时丢掉 `@import`：样式可以影响整页，但不应让页面在打开时去外部拉资源。
pub fn sanitize_css(css: &str) -> String {
    let css = drop_imports(css);
    let mut out = String::with_capacity(css.len());
    let mut rest = css.as_str();
    while let Some(position) = find_ci(rest, "</style") {
        out.push_str(&rest[..position]);
        // 连它后面到 `>` 的部分一起去掉
        let tail = &rest[position..];
        let end = find_ci(tail, ">").map(|i| i + 1).unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// 丢掉 `@import …;`（大小写不敏感）。
fn drop_imports(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(position) = find_ci(rest, "@import") {
        out.push_str(&rest[..position]);
        let tail = &rest[position..];
        match tail.find(';') {
            Some(end) => rest = &tail[end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// 找一段子串（仅 ASCII 大小写不敏感），返回的字节偏移可以直接切原文。
///
/// 不能对整串做 Unicode 小写化再拿偏移切原文：`İ`/`ẞ` 这类字符小写化后会变
/// 字节数，偏移就落到别处去了 —— 轻则 panic，重则把 `</style` 漏过去。
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::{drop_imports, sanitize_css};

    #[test]
    fn closes_the_style_tag() {
        assert_eq!(sanitize_css("a</style>b"), "ab");
        assert_eq!(sanitize_css("a</STYLE>b"), "ab");
        assert_eq!(sanitize_css("a</style"), "a");
    }

    #[test]
    fn keeps_offsets_valid_around_non_ascii() {
        // 小写化会改字节数的字符不能挪动 `</style` 的位置：修复前这里会把闭合
        // 标签原样输出，脚本于是落在 `<style>` 外面。
        let css = "İİİİİİİİ</style>x><script>alert(1)</script>";
        let out = sanitize_css(css);
        // 去掉的只有闭合标签本身；剩下的 `<script>` 字样留在 `<style>` 里是死文本。
        assert!(!out.contains("</style"), "输出：{out}");
        assert!(out.contains("<script>"), "输出：{out}");
    }

    #[test]
    fn strips_import_rules() {
        assert_eq!(drop_imports("a@import url(x.css);b"), "ab");
        assert_eq!(drop_imports("@IMPORT url(x.css);"), "");
        assert_eq!(drop_imports("a@import url(x.css)"), "a");
        assert_eq!(drop_imports("a@importx;b"), "ab");
    }
}
