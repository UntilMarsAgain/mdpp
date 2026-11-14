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

use super::super::dispatch::render_inline_problem;
use crate::block::typecheck;
use crate::registry::Block;

/// 把 `render_nested` 的输出收成一段行内 HTML。
///
/// 单段落是 markdown 的常态；`<p>…</p>` 的外壳对 `<th>` / `<span>` 既不合法
/// 也不好看，所以剥掉。多段直接拒绝，不再猜作者想保留哪一段。
fn inline_html(text: &str) -> Result<String, String> {
    let snapshot = crate::env::snapshot();
    let rendered = crate::engine::render_nested(text);
    match typecheck::check_inline(&rendered) {
        Ok(html) => Ok(html),
        Err(why) => {
            // 行内检查失败时，正文里已经登记的样式/脚本也要跟着丢掉；
            // 否则错误框旁边还留着半截渲染的副作用。
            if let Some(snapshot) = snapshot {
                crate::env::restore(snapshot);
            }
            Err(format!("`::inline` 只接受单段行内内容：{why}"))
        }
    }
}

/// `::inline` —— 行内渲染，不产生自己的元素。
pub(super) fn render_inline(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    if !block.params.is_empty() {
        render_inline_problem(block, fmt, "`::inline` 不接受参数：正文就是行内内容");
        return;
    }
    let text = block.body.trim();
    if text.is_empty() {
        render_inline_problem(block, fmt, "内容是空的：::inline 要写点字");
        return;
    }
    match inline_html(text) {
        Ok(html) => fmt.text_raw(&html),
        Err(why) => render_inline_problem(block, fmt, &why),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::RenderContext;
    use crate::block::template::{TemplatePage, TemplateSource};
    use crate::engine::Mdpp;

    #[derive(Debug, Default)]
    struct Pages(HashMap<String, TemplatePage>);

    impl TemplateSource for Pages {
        fn page(&self, name: &str) -> Option<TemplatePage> {
            self.0.get(name).cloned()
        }
    }

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    fn render_with_template(name: &str, source: &str, src: &str) -> String {
        let mut pages = Pages::default();
        pages
            .0
            .insert(name.to_string(), TemplatePage::Ready(source.to_string()));
        let context = RenderContext::new().with_templates(Arc::new(pages));
        Mdpp::new().render_with(src, &context)
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

    #[test]
    fn inline_accepts_a_template_that_outputs_inline_html() {
        let span = "::element tag=span inline=on\n  行内**内容**";
        let html = render_with_template("span", span, "::inline\n  ::span\n");
        assert!(html.contains("<span>"), "{html}");
        assert!(html.contains("<strong>内容</strong>"), "{html}");
        assert!(!html.contains("template--problem"), "{html}");
    }

    #[test]
    fn inline_refuses_a_block_template_and_rolls_back_its_styles() {
        let block = "::css\n  .rollback { color: red; }\n::element tag=div\n  块内容";
        let html = render_with_template("block", block, "::inline\n  ::block\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(!html.contains("<div>"), "{html}");
        assert!(!html.contains("rollback"), "{html}");
    }
}
