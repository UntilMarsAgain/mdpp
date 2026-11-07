//! CSS 类容器：`::class`。
//!
//! 把内容包进一个带指定类名的 `<div>` 里，方便用 `::css` 再给它定样子。
//! 内容**照常按 markdown 解析**。
//!
//! ```text
//! ::class class=我的类
//!   这里的内容会被包进 `<div class="我的类">`。
//!
//!   可以写 **markdown**、[[内部链接]]，也能再嵌块。
//! ```

use super::super::dispatch::render_problem;
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::class` —— 内容包进带指定类名的 `<div>` 里。
pub(super) fn render_class(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let class = match block.param("class") {
        Some(c) if !c.trim().is_empty() => c.trim().to_string(),
        _ => {
            render_problem(block, fmt, "class= 缺失：写 ::class class=我的类");
            return;
        }
    };

    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：::class 要包裹点东西");
        return;
    }

    fmt.cr();
    fmt.open("div", &[("class", class)]);
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

    /// 内容被包进带指定类名的 div 里
    #[test]
    fn class_wraps_content_in_div_with_class() {
        let html = render("::class class=我的类\n  这里是内容\n");
        assert!(html.contains(r#"<div class="我的类">"#), "{html}");
        assert!(html.contains("这里是内容"), "{html}");
        assert!(html.contains("</div>"), "{html}");
    }

    /// 内容照常按 markdown 解析
    #[test]
    fn class_body_is_markdown() {
        let html = render("::class class=foo\n  这里有**加粗**与[[链接]]\n");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
        assert!(html.contains("wikilink"), "{html}");
    }

    /// class= 缺失时报错
    #[test]
    fn missing_class_is_problem() {
        let html = render("::class\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("class="), "{html}");
    }

    /// class= 空串也报错
    #[test]
    fn empty_class_is_problem() {
        let html = render("::class class=\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
    }

    /// 空内容报错
    #[test]
    fn empty_content_is_problem() {
        let html = render("::class class=foo\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("是空的"), "{html}");
    }
}
