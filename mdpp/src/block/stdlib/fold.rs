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

    let mut detail_class = "fold".to_string();
    let mut summary_class = "fold--custom".to_string();
    let mut summary_style = String::new();
    if let Some(color) = block.param("color") {
        let Some(hex) = normalize_hex(color) else {
            render_problem(block, fmt, &hex_why("color", None));
            return;
        };
        summary_style.push_str(&format!("background: {hex}; color: {};", text_on(&hex)));
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

    let mut detail_attrs: Vec<(&str, String)> = vec![("class", detail_class)];
    if open {
        detail_attrs.push(("open", String::new()));
    }

    let mut summary_attrs: Vec<(&str, String)> = vec![("class", summary_class)];
    if !summary_style.is_empty() {
        summary_attrs.push(("style", summary_style));
    }

    fmt.cr();
    fmt.open("details", &detail_attrs);
    fmt.cr();
    fmt.open("summary", &summary_attrs);
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
        assert!(html.contains(r#"<summary class="fold--custom">草稿</summary>"#), "{html}");
        assert!(html.contains("这是折起来的内容"), "{html}");
    }

    /// 标题可省略，缺省「更多」
    #[test]
    fn fold_default_title() {
        let html = render("::fold\n  没写标题\n");
        assert!(html.contains(r#"<summary class="fold--custom">更多</summary>"#), "{html}");
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

    /// `color=` 只认十六进制，乱写的拒掉；color 只影响标题栏（summary），不影响内部内容
    #[test]
    fn fold_color_validated() {
        let colored = render("::fold color=#0055a4 title=深蓝\n  正文\n");
        // color 写在 summary 的 style 上
        assert!(
            colored.contains(r#"<summary class="fold--custom" style="background: #0055a4; color: #f5f5f5;">"#),
            "{colored}"
        );
        assert!(colored.contains("fold--custom"), "{colored}");
        // 正文内容不在 style 属性里，不会被 color 污染
        assert!(colored.contains("正文"), "{colored}");

        let bad = render("::fold color=red title=乱写\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
        // 错误时整块拒掉，不渲染半截
        assert!(!bad.contains(r#"class="fold""#), "{bad}");
    }

    /// `color=` 只影响标题栏，不影响折叠内容区
    #[test]
    fn fold_color_only_affects_title() {
        let html = render("::fold color=#0055a4 title=标题\n  这里是正文内容\n");
        // summary 上有背景色
        assert!(
            html.contains(r#"<summary class="fold--custom" style="background: #0055a4; color: #f5f5f5;">"#),
            "标题栏应有背景色和字色：{html}"
        );
        // 内容区在 </summary> 之后，不应有 inline style
        let after_summary = &html[html.find("</summary>").expect("summary 应该关闭")..];
        assert!(!after_summary.contains("style="), "内容区不应有 inline style：{after_summary}");
        // 内容本身正常
        assert!(after_summary.contains("这里是正文内容"), "内容应正常显示：{after_summary}");
    }
}
