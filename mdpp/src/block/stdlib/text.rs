//! 原样文本原语：`text`。
//!
//! 它不解析 markdown，也不产生自己的元素；需要外层结构时由模板用 `::element`
//! 包住。`fields` 已迁到标准库模板，表格行由 [`super::rows`] 提供。

use super::super::dispatch::render_problem;
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::text` —— 原样文本原语：正文**不按 markdown 解析**，HTML 记号也按字面转义。
///
/// 它不产生自己的元素，只把文字写出去；需要外层结构时由模板用 `::element` 包住。
/// 这是给「标题、标签、字段值」这类必须保持字面的内容用的，不适合写正文。
pub(super) fn render_text(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    if !block.params.is_empty() {
        render_problem(block, fmt, "`::text` 不接受参数：正文会按字面输出");
        return;
    }

    let text = block.body.trim();
    if text.is_empty() {
        render_problem(block, fmt, "内容是空的：::text 要写点字");
        return;
    }

    fmt.cr();
    fmt.text(text);
    fmt.cr();
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// `::text` 只做 HTML 转义，不解析 markdown
    #[test]
    fn text_is_literal_and_escaped() {
        let html = render("::text\n  <b>**不解析**</b>\n");
        assert!(html.contains("&lt;b&gt;**不解析**&lt;/b&gt;"), "{html}");
        assert!(!html.contains("<strong>"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
    }

    /// `::text` 可以放进 `::element`，由外层提供结构
    #[test]
    fn text_can_sit_inside_an_element() {
        let html = render("::element tag=p\n  ::text\n    原样 <tag>\n");
        assert!(html.contains("<p>"), "{html}");
        assert!(html.contains("原样 &lt;tag&gt;"), "{html}");
    }

    /// 空正文与参数都算用法问题
    #[test]
    fn text_rejects_empty_content_and_parameters() {
        let empty = render("::text\n");
        assert!(empty.contains("template--problem"), "{empty}");

        let param = render("::text class=foo\n  内容\n");
        assert!(param.contains("template--problem"), "{param}");
    }
}
