//! 缩进容器：`::indent`。
//!
//! 把内容包进一个带 `margin-left` 的 `<div>` 里，缩进等级由 `indent=` 给出，
//! 每级 1.6em。内容**照常按 markdown 解析**。
//!
//! ```text
//! ::indent indent=2
//!   这段会被缩进 3.2em。
//!
//!   可以写 **markdown**、[[内部链接]]，也能再嵌块。
//! ```

use super::super::dispatch::render_problem;
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::indent` —— 内容包进带缩进的 `<div>` 里。
pub(super) fn render_indent(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let indent = match block.param("indent") {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => {
            render_problem(
                block,
                fmt,
                "indent= 缺失：写 ::indent indent=2",
            );
            return;
        }
    };

    let value: f64 = match indent.parse() {
        Ok(v) => v,
        Err(_) => {
            render_problem(
                block,
                fmt,
                "indent= 只认数字，例如 indent=2（缩进 3.2em）",
            );
            return;
        }
    };

    if node.children.is_empty() {
        render_problem(
            block,
            fmt,
            "内容是空的：::indent 要包裹点东西",
        );
        return;
    }

    let em = value * 1.6;
    fmt.cr();
    fmt.open("div", &[("style", format!("margin-left: {}em;", em))]);
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 内容被包进带 margin-left 的 div 里，indent × 1.6em
    #[test]
    fn indent_wraps_content_with_margin_left() {
        let html = render("::indent indent=2\n  这里是内容\n");
        assert!(html.contains("margin-left: 3.2em;"), "{html}");
        assert!(html.contains("这里是内容"), "{html}");
        assert!(html.contains("</div>"), "{html}");
    }

    /// indent=0 时 margin-left 为 0em
    #[test]
    fn indent_zero_is_zero_em() {
        let html = render("::indent indent=0\n  内容\n");
        assert!(html.contains("margin-left: 0em;"), "{html}");
    }

    /// 内容照常按 markdown 解析
    #[test]
    fn indent_body_is_markdown() {
        let html = render("::indent indent=1\n  这里有**加粗**与[[链接]]\n");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
        assert!(html.contains("wikilink"), "{html}");
    }

    /// indent= 缺失时报错
    #[test]
    fn missing_indent_is_problem() {
        let html = render("::indent\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("indent="), "{html}");
    }

    /// indent= 空串也报错
    #[test]
    fn empty_indent_is_problem() {
        let html = render("::indent indent=\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
    }

    /// indent= 只认数字
    #[test]
    fn indent_rejects_non_number() {
        let html = render("::indent indent=也许\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("indent="), "{html}");
    }

    /// 空内容报错
    #[test]
    fn empty_content_is_problem() {
        let html = render("::indent indent=1\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("是空的"), "{html}");
    }
}
