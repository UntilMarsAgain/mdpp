//! 折叠模板：`::fold`。
//!
//! 默认折叠，只露出一行标题；点标题展开，再点折回去。
//! 用 HTML 原生的 `<details>` / `<summary>`，不依赖脚本 —— 没有脚本的环境
//! 里点标题照样能展开。
//!
//! 内容**照常按 markdown 解析**：里面可以写列表、链接、内部链接，也能再嵌块。
//!
//! 参数：
//! - `title=`：标题，缺省「更多」；
//! - `open=`：默认展开。认 `open` / `true` / `on` / `1`，其余当不写；
//! - `color=`：底色（只认 `#rgb` / `#rrggbb`），字色由 [`super::text_on`] 定。

use super::super::dispatch::render_problem;
use super::{hex_why, normalize_hex, text_on};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::fold title="……"` —— 默认折叠，只露出一行标题。
///
/// 用 `<details>` / `<summary>` 原生折叠：没有脚本也能点开。
pub(super) fn render_fold(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(
            block,
            fmt,
            "内容是空的：折起来的那一块要写点东西，或留空不如不用这块",
        );
        return;
    }

    let mut class = "fold".to_string();
    let mut style = String::new();
    if let Some(color) = block.param("color") {
        let Some(hex) = normalize_hex(color) else {
            render_problem(block, fmt, &hex_why("color", None));
            return;
        };
        style.push_str(&format!("background: {hex}; color: {};", text_on(&hex)));
        class.push_str(" fold--custom");
    }

    let title = block.param("title").unwrap_or("更多");

    let open = block
        .param("open")
        .map(|value| {
            matches!(
                value.trim().to_lowercase().as_str(),
                "open" | "true" | "on" | "1" | ""
            )
        })
        .unwrap_or(false);

    let mut attrs: Vec<(&str, String)> = vec![("class", class)];
    if open {
        attrs.push(("open", String::new()));
    }
    if !style.is_empty() {
        attrs.push(("style", style));
    }

    fmt.cr();
    fmt.open("details", &attrs);
    fmt.cr();
    fmt.open("summary", &[]);
    fmt.text(title);
    fmt.close("summary");
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close("details");
    fmt.cr();
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 默认折叠：有 `<details>`、`<summary>` 与标题，内容也在
    #[test]
    fn fold_renders_title_and_body() {
        let html = render("::fold title=草稿\n  这是折起来的内容\n");
        assert!(html.contains(r#"<details class="fold">"#), "{html}");
        assert!(html.contains("<summary>草稿</summary>"), "{html}");
        assert!(html.contains("这是折起来的内容"), "{html}");
    }

    /// 标题可省略，缺省「更多」
    #[test]
    fn fold_default_title() {
        let html = render("::fold\n  没写标题\n");
        assert!(html.contains("<summary>更多</summary>"), "{html}");
    }

    /// `open=` 默认展开：认 open / true / on / 1 —— 渲染出 `open=""` 这种 HTML 空值写法
    #[test]
    fn fold_open_starts_open() {
        for value in ["open", "true", "on", "1"] {
            let html = render(&format!("::fold open={value}\n  正文\n"));
            assert!(
                html.contains(r#"<details class="fold" open="">"#),
                "open={value}: {html}"
            );
        }
    }

    /// `open=` 的其它写法当不写：不加 open 属性
    #[test]
    fn fold_open_unknown_is_closed() {
        let html = render("::fold open=也许\n  正文\n");
        assert!(!html.contains(" open=\""), "{html}");
        assert!(!html.contains(" open "), "{html}");
    }

    /// `title=` 带空格要用引号，内容照常按 markdown 解析
    #[test]
    fn fold_body_is_markdown() {
        let html = render("::fold title=\"草稿箱\"\n  这里有**加粗**与[[链接]]\n");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
        assert!(html.contains("wikilink"), "{html}");
    }

    /// 空内容如实报错
    #[test]
    fn an_empty_fold_says_so() {
        let html = render("::fold\n");
        assert!(html.contains("内容是空的"), "{html}");
        assert!(html.contains("template--problem"), "{html}");
    }

    /// `color=` 只认十六进制，乱写的拒掉
    #[test]
    fn fold_color_validated() {
        let colored = render("::fold color=#0055a4 title=深蓝\n  正文\n");
        assert!(colored.contains("background: #0055a4"), "{colored}");
        assert!(colored.contains("color: #f5f5f5"), "{colored}");
        assert!(colored.contains("fold--custom"), "{colored}");

        let bad = render("::fold color=red title=乱写\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(!bad.contains("style="), "{bad}");
    }
}
