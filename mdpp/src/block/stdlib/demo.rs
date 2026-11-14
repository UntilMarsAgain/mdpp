//! `::demo` —— 语法演示：正文渲染出的 HTML，直接摆成源码。
//!
//! 语法文档的典型需求 —— 这一段这样写出来，对应的 HTML 是什么。本块把正文当
//! markdown 走一遍**整个流水线**，拿回结果 HTML，然后**只**把那份 HTML 包进
//! `::code lang=html` 那件壳里展示 —— 渲染结果本身不再摆第二遍。
//!
//! 源码那一半**复用** ::code 的渲染器：转义与那两个 class（`template-code` /
//! `language-html`）在**一处**定死 —— `::code` 的样子改了，这里跟着改，不会漂。
//!
//! 正文是**原始档**（引擎不预解析）：渲染发生在**本块的渲染器里**，走的是当次渲染
//! 的那一份解析器、上下文与注册表 —— 与 `::html` 里的 `<markdown>` 同一条路，
//! 所以正文里的 `[[内部链接]]`、模板页、嵌套的 `::` 块照常算数。
//!
//! 为什么是内置块而不是模板页：模板页的填空是**纯文本替换**（填完整页重新解析），
//! 没有「先把正文渲染成 HTML、再把那份 HTML 当源码摆出来」这一档变换 —— 这个
//! 变换只能由一个真正的块自己做。

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use super::embeds::render_code;
use crate::engine::render_nested;
use crate::registry::Block;

/// `::demo` —— 正文渲染出的 HTML，以源码的样子摆在页面上。
pub(super) fn render_demo(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    // 空正文是作者写漏了：摆个说明框，别输出一个空壳
    if block.body.trim().is_empty() {
        render_problem(block, fmt, "内容是空的：::demo 要演示点东西");
        return;
    }

    // 渲染：正文当 markdown 走一遍整个流水线，拿回结果 HTML
    let html = render_nested(&block.body);

    // 只摆源码：那份 HTML 包进 `::code lang=html` 的壳 —— 结构与转义都交给
    // ::code 自己的渲染器
    let code = Block {
        name: "code".to_string(),
        params: vec![("lang".to_string(), "html".to_string())],
        body: html.trim().to_string(),
    };
    render_code(&code, node, fmt);
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 实体序列用拼接拼出来：直接写死的话，这套写文件的工具会把它还原成记号本身
    fn ent(mid: &str) -> String {
        format!("&{mid}")
    }

    /// 只输出源码：渲染结果本身**不再**摆第二遍
    #[test]
    fn shows_only_the_html_source_not_a_rendered_copy() {
        let html = render("::demo\n  **加粗** 里有 H_2_O\n");
        // 源码壳在
        assert!(
            html.contains(r#"<pre class="mdpp-template-code"><code class="language-html">"#),
            "{html}"
        );
        // 壳里是**转义后**的 HTML
        let lt = ent("lt;");
        let gt = ent("gt;");
        assert!(html.contains(format!("{lt}strong{gt}").as_str()), "{html}");
        assert!(
            html.contains(format!("{lt}sub{gt}2{lt}/sub{gt}").as_str()),
            "{html}"
        );
        // 渲染结果**只**以源码的样子出现，没有再渲染一遍
        assert!(!html.contains("<strong>加粗</strong>"), "{html}");
        assert!(!html.contains("<sub>2</sub>"), "{html}");
    }

    /// 正文走**整个**流水线：嵌套的 `::` 块也当真，但同样只出现在源码里
    #[test]
    fn the_body_goes_through_the_whole_pipeline() {
        let html = render("::demo\n  ::note\n    框里的话\n");
        // 嵌套块没有直接渲染
        assert!(!html.contains("<div class=\"mdpp-callout"), "{html}");
        // 它的结构在壳里看得见（引号被同一个转义规则收编成实体）
        let q = ent("quot;");
        let tag = format!(
            "{l}div class={q}mdpp-callout mdpp-callout--note{q}{r}",
            l = ent("lt;"),
            r = ent("gt;"),
        );
        assert!(html.contains(&tag), "{html}");
    }

    /// 空正文：摆个说明框，不输出空壳
    #[test]
    fn an_empty_body_says_what_to_do_rather_than_silently_shows_nothing() {
        let html = render("::demo\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("::demo 要演示点东西"), "{html}");
    }
}
