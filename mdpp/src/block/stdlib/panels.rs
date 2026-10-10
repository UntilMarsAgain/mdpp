//! 分节类：`tabs` / `theme` —— 全库唯二按 `[标签]` 分节的块。
//!
//! 这两个与其他块的根本差别：**它们不读块正文**。分节发生在扫描时（见
//! [`crate::block::scanner`]），各节已经各自解析成子节点、挂在一个 [`Section`]
//! 下面挂到本块的 `children` 上 —— 一节是一篇独立的小文档，所以渲染器只做一件事：
//! 把各节按自己的方式摆出来。
//!
//! 两者的差别也只在于**怎么摆**：选项卡是一排按钮加若干面板，**切换靠一段脚本**
//! （`mdpp-cli/assets/mdpp.js` —— 那是界面那一层的事，渲染器不该管，所以这里只
//! 吐结构）；深浅色主题是把两节都渲染进去、显示哪一节由样式按
//! `data-mdpp-theme` 决定（切主题不必重新渲染正文）。

use super::super::dispatch::{render_problem, sections};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::tabs` —— 选项卡：块里用 `[标签]` 分节，一节就是一个面板。
///
/// ```text
/// ::tabs
///   [北岸]
///   北岸的走法……
///   [南岸]
///   南岸的走法……
/// ```
///
/// 这里只产出**结构**（一排标签 + 各面板），切换由使用方接上 —— 渲染器往输出里塞不了
/// `onclick`，塞了也过不了 CSP，而同一个页面往往有多处注入，行为该由一处管。
/// 第一节默认选中。标签是显示用的字，原样保留（`[API]` 不会变成 `[api]`）。
///
/// `[标签]` 与本节的其他行**左对齐**：写得更深就当成里层内容了（嵌套的选项卡靠这条分得开）。
pub(super) fn render_tabs(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let sections = sections(node);
    if sections.is_empty() {
        render_problem(block, fmt, "选项卡是空的：每一节要以 [标签] 开头");
        return;
    }
    if sections.iter().any(|(label, _)| label.is_empty()) {
        render_problem(
            block,
            fmt,
            "有内容写在第一个 [标签] 之前：选项卡的每一节都要以 [标签] 开头",
        );
        return;
    }

    fmt.cr();
    fmt.open("div", &[("class", "tabs".to_string())]);
    fmt.cr();
    fmt.open(
        "div",
        &[
            ("class", "tabs__bar".to_string()),
            ("role", "tablist".to_string()),
        ],
    );
    for (index, (label, _)) in sections.iter().enumerate() {
        let selected = index == 0;
        // 用真按钮：Tab 键走得到、回车就切（切换的那段脚本认 `role="tablist"`）
        let attrs: Vec<(&str, String)> = vec![
            (
                "class",
                if selected {
                    "tabs__tab tabs__tab--on".to_string()
                } else {
                    "tabs__tab".to_string()
                },
            ),
            ("type", "button".to_string()),
            ("role", "tab".to_string()),
            ("data-tab", index.to_string()),
            (
                "aria-selected",
                if selected { "true" } else { "false" }.to_string(),
            ),
        ];
        fmt.open("button", &attrs);
        fmt.text(label);
        fmt.close("button");
    }
    fmt.cr();
    fmt.close("div");
    fmt.cr();

    for (index, (label, content)) in sections.iter().enumerate() {
        let mut attrs: Vec<(&str, String)> = vec![
            ("class", "tabs__panel".to_string()),
            ("role", "tabpanel".to_string()),
            ("data-panel", index.to_string()),
            // 屏幕上这一面是藏着的，名字已经写在标签上了；打印时全展开，
            // 就得靠这个字段给每一节起名字（见 mdpp.css 里那段 `@media print`）。
            // 缺了它，打印出来就是几段没有标题的正文。
            ("data-label", (*label).to_string()),
        ];
        if index != 0 {
            attrs.push(("hidden", "hidden".to_string()));
        }
        fmt.open("div", &attrs);
        fmt.cr();
        fmt.contents(&content.children);
        fmt.cr();
        fmt.close("div");
    }
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// `::theme` —— 同一段内容，深浅色各给一版。
///
/// ```text
/// ::theme
///   [light]
///   浅色下显示这一段
///   [dark]
///   深色下显示这一段
/// ```
///
/// **两节都会渲染进 HTML**，显示哪一节由样式按当前主题决定（`<html data-mdpp-theme="…">`
/// 是主题的落点）。这样切主题不必重新渲染正文 —— 就像别的样式一样，当场就变。
///
/// 写在第一个标签**之前**的内容两边都显示（`[both]` 同义）：前言、说明常常要这样写。
/// 与 `::tabs` 同一个规矩：`[标签]` 要与本节的其他行**左对齐**。
pub(super) fn render_theme(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let sections = sections(node);
    if sections.is_empty() {
        render_problem(block, fmt, "要分节：每一节以 [light] 或 [dark] 开头");
        return;
    }

    // 先整块认一遍再渲染：标签写错了就整块报出来，免得渲染半截
    let mut parts: Vec<(&str, &Node)> = Vec::new();
    for (label, content) in &sections {
        let slot = match label.to_lowercase().as_str() {
            "" | "both" | "all" => "",
            "light" => "light",
            "dark" => "dark",
            _ => {
                let why = format!(
                    "认不出这一节的标签 [{label}]：只认 [light] 与 [dark] \
                     （写在最前面的内容两边都显示）"
                );
                render_problem(block, fmt, &why);
                return;
            }
        };
        parts.push((slot, content));
    }

    fmt.cr();
    fmt.open("div", &[("class", "theme".to_string())]);
    for (slot, content) in parts {
        let class = match slot {
            "light" => "theme__part theme__part--light",
            "dark" => "theme__part theme__part--dark",
            _ => "theme__part",
        };
        fmt.cr();
        fmt.open("div", &[("class", class.to_string())]);
        fmt.cr();
        fmt.contents(&content.children);
        fmt.cr();
        fmt.close("div");
    }
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

    /// `::theme`：两节都渲染出来，由样式按主题决定显示哪一节
    #[test]
    fn theme_renders_both_versions_and_lets_the_stylesheet_pick() {
        let html = render("::theme\n  [light]\n  浅色下看这个\n  [dark]\n  深色下看这个\n");
        assert!(html.contains("theme__part--light"), "{html}");
        assert!(html.contains("theme__part--dark"), "{html}");
        assert!(
            html.contains("浅色下看这个") && html.contains("深色下看这个"),
            "{html}"
        );

        // 标签之前的内容两边都显示
        let both = render("::theme\n  两种主题都显示\n  [dark]\n  只有深色\n");
        assert!(both.contains("class=\"theme__part\""), "{both}");

        let bad = render("::theme\n  [blue]\n  没有这个主题\n");
        assert!(bad.contains("template--problem"), "{bad}");
    }

    /// `::tabs`：这里给结构与内容（第一节选中），切换留给使用方
    #[test]
    fn tabs_render_a_bar_and_one_panel_each() {
        let html = render("::tabs\n  [北岸]\n  走北路\n  [南岸]\n  走南路\n");
        assert_eq!(html.matches("class=\"tabs__tab").count(), 2, "{html}");
        assert!(html.contains("tabs__tab--on"), "{html}");
        assert!(html.contains("走北路") && html.contains("走南路"), "{html}");
        // 没选中的面板先藏着，点标签才出来
        assert!(html.contains("hidden"), "{html}");

        // 一节里的正文按 markdown 渲染
        let rich = render("::tabs\n  [一]\n  **加粗**\n");
        assert!(rich.contains("<strong>加粗</strong>"), "{rich}");

        let stray = render("::tabs\n  没写标签的内容\n  [一]\n  正文\n");
        assert!(stray.contains("template--problem"), "{stray}");
    }

    /// 一节里还能再嵌一个选项卡（各节是各自解析的小文档）
    #[test]
    fn tabs_nest() {
        let nested = render("::tabs\n  [甲]\n  ::tabs\n    [内]\n    内文\n  [乙]\n  乙文\n");
        assert_eq!(nested.matches("class=\"tabs\"").count(), 2, "{nested}");
        assert!(
            nested.contains("内文") && nested.contains("乙文"),
            "{nested}"
        );
    }

    /// 标签是显示用的字，原样保留 —— `[API]` 不该变成 `[api]`
    #[test]
    fn tab_labels_keep_their_case() {
        let html = render("::tabs\n  [API]\n  内容\n");
        assert!(html.contains(">API</button>"), "{html}");
    }
}
