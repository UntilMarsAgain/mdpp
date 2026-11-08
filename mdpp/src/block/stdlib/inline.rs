//! 行内渲染原语：`::inline`。
//!
//! 它把正文按 markdown 解析，但**只取单段行内内容**，不产生外层 `<p>` 或别的
//! 容器。适合在模板里渲染标题、标签、组名这种必须落在行内的值：
//!
//! ```mdpp
//! ::inline
//!   {{@item.label}}
//! ```
//!
//! 链接走的是同一个 `[[…]]` 解析器；正文里若是多段、列表或块级结构，会报用法
//! 问题 —— 那些东西塞不进一个 `<span>` / `<th>` 里。

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use crate::registry::Block;

/// 把 `render_nested` 的输出收成一段行内 HTML。
///
/// 单段落是 markdown 的常态；`<p>…</p>` 的外壳对 `<th>` / `<span>` 既不合法
/// 也不好看，所以剥掉。多段直接拒绝，不再猜作者想保留哪一段。
fn inline_html(text: &str) -> Result<String, String> {
    let rendered = crate::engine::render_nested(text);
    let trimmed = rendered.trim();
    let Some(inner) = trimmed
        .strip_prefix("<p>")
        .and_then(|rest| rest.strip_suffix("</p>"))
    else {
        return Err("`::inline` 只接受单段行内内容，不能写列表、标题或多个段落".to_string());
    };
    if inner.contains("</p>") {
        return Err("`::inline` 只接受单段行内内容，不能写多个段落".to_string());
    }
    Ok(inner.to_string())
}

/// `::inline` —— 行内渲染，不产生自己的元素。
pub(super) fn render_inline(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    if !block.params.is_empty() {
        render_problem(block, fmt, "`::inline` 不接受参数：正文就是行内内容");
        return;
    }
    let text = block.body.trim();
    if text.is_empty() {
        render_problem(block, fmt, "内容是空的：::inline 要写点字");
        return;
    }
    match inline_html(text) {
        Ok(html) => fmt.text_raw(&html),
        Err(why) => render_problem(block, fmt, &why),
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 单段正文只输出行内内容，不留下 `<p>`
    #[test]
    fn inline_strips_the_single_paragraph() {
        let html = render("::inline\n  这是**重点**与 [[链接]]\n");
        assert!(html.contains("<strong>重点</strong>"), "{html}");
        assert!(html.contains("链接"), "{html}");
        assert!(!html.contains("<p>"), "{html}");
        assert!(!html.contains("::inline"), "{html}");
    }

    /// 多段和块级正文不放行
    #[test]
    fn inline_refuses_block_content_and_parameters() {
        for source in [
            "::inline\n  第一段\n\n  第二段\n",
            "::inline\n  * 列表\n",
            "::inline class=foo\n  正文\n",
            "::inline\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{source:?}: {html}");
        }
    }
}
