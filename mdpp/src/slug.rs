//! 标题锚点。
//!
//! 生成的 `id` 是**未编码的原文**，而 `[跳转](#表格)` 那样的 `href` 是 URL 编码过的
//! —— 两者对不上，取元素之前要 `decodeURIComponent`。

/// 由标题文字生成锚点。
///
/// 字母数字（汉字算）原样保留，其余归并为 `-`，去掉首尾多余的 `-`。
pub fn heading_id(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_dash = true;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::heading_id;

    #[test]
    fn cjk_is_kept_and_the_rest_is_folded() {
        assert_eq!(heading_id("文本与行内元素"), "文本与行内元素");
        assert_eq!(heading_id("Hello World"), "hello-world");
        assert_eq!(heading_id("H1 Test"), "h1-test");
        // 连续的非字母数字（空格、斜杠）只归并成一个 '-'，首尾不留 '-'
        assert_eq!(heading_id("  Code / Notes  "), "code-notes");
        assert_eq!(heading_id("!!!"), "");
    }
}
