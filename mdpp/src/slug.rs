//! 标题的 id：给页内跳转用。
//!
//! 保留字母数字（中日韩字符也算），其余字符归并为 `-`，并去掉首尾多余的 `-`。

/// 由标题文字生成一个 id。
///
/// 刻意不用 markdown-it 自带的 `simple_slugify_fn`：它的文档明确写着
/// "added for testing and demonstration purposes only"。
///
/// 中文标题会被原样保留（`ch.is_alphanumeric()` 对汉字为真），所以
/// `[跳转](#表格)` 里的 `href` 是 URL 编码过的，而标题上的 `id` 是未编码的原文 ——
/// 两者对不上，**取元素之前要 `decodeURIComponent`**。这件事值得单说，因为它
/// 很容易被当成 bug。
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