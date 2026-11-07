//! 分节类：`tabs` / `theme` / `columns` —— 按 `[标签]` 分节的三个块。
//!
//! 它们与其他块的根本差别是**不读块正文**：分节发生在扫描时（见
//! [`crate::block::scanner`]），各节已经各自解析成子节点、挂在一个 [`Section`] 下面。
//! 渲染器只做一件事：把各节按自己的方式摆出来。
//!
//! 差别也只在于**怎么摆**：选项卡是一排按钮加若干面板，切换靠一段脚本
//! （`mdpp-cli/assets/mdpp.js` —— 那是界面那一层的事，渲染器只吐结构）；深浅色主题是
//! 把两节都渲染进去、显示哪一节由样式按 `data-mdpp-theme` 决定（切主题不必重新渲染）；
//! 分栏是把各节**一起摆出来**，横着排，窄屏由样式堆成一竖列。

use super::super::dispatch::{enter_theme_slot, no_leading_content, render_problem, sections};
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
    if let Err(why) = no_leading_content(&sections, "选项卡") {
        render_problem(block, fmt, &why);
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
            // 就得靠这个字段给每一节起名字（见 `mdpp-cli/assets/mdpp.css` 的
            // `@media print` 那一节）。缺了它，打印出来就是几段没有标题的正文。
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
///
/// ## ::css / ::js / ::html js 的选择性注入
///
/// `::theme` 节里嵌套的 `::css`、`::js`、`::html js` 不会直接写进 `<style>`/`<script>`，
/// 而是被放进带 `data-theme` 的 `.theme-asset` 占位容器里，由页面脚本按当前主题
/// 选择性地真正注入到文档中（创建 `<style>` / `<script>` 标签）。这样浅色节的
/// 脚本不会在深色下执行，反之亦然。
///
/// 页面加载时自动注入；系统 `prefers-color-scheme` 切换时自动重新注入；下游库若
/// 重写了 `<html data-mdpp-theme>`，可调用 `window.mdppThemeInject()` 手动触发重注入。
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
        // 进入 theme slot：::css / ::js / ::html js 会检测到这个标记，
        // 输出带 data-theme 的占位容器，不直接写 <style>/<script>。
        let _guard = enter_theme_slot(slot);
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

/// `::columns` —— 水平分栏：块里用 `[标签]` 分节，一节就是一栏，几栏一起摆出来。
///
/// ```text
/// ::columns
///   [写法]
///   用 ::quote 的写法……
///   [效果]
///   渲染出来的样子……
/// ```
///
/// 与 `::tabs` 的根本差别是**几栏同时可见**：选项卡一次只露一节，分栏是拿来并排比对的
/// （写法与效果、两种变体、参数与默认值）。每节各自是一份小文档，栏里照样能写段落、
/// 表格、别的块。
///
/// `[标签]` 默认当**栏头**（一行小字压在栏顶）；纯摆版面不想见字就写 `labels=off`。
/// 摆不下时样式会自动堆成一竖列 —— 那是 CSS 的事，渲染器不分宽窄屏。
/// 与 `::tabs` 同一个规矩：`[标签]` 要与本节的其他行**左对齐**。
pub(super) fn render_columns(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let sections = sections(node);
    if sections.is_empty() {
        render_problem(block, fmt, "分栏是空的：每一节要以 [标签] 开头");
        return;
    }
    if let Err(why) = no_leading_content(&sections, "分栏") {
        render_problem(block, fmt, &why);
        return;
    }

    // 栏头默认露出来；要纯版面就 labels=off（与 banner 的 rounded=off 同一套取值）
    let labels = block
        .param("labels")
        .map(|value| value.trim().to_lowercase());
    let headless = match labels.as_deref() {
        None | Some("" | "on" | "true" | "yes") => false,
        Some("off" | "false" | "no" | "0") => true,
        Some(_) => {
            render_problem(block, fmt, "labels= 只认 off（不写栏头）；不写就是写");
            return;
        }
    };

    fmt.cr();
    fmt.open(
        "div",
        &[(
            "class",
            if headless {
                "columns columns--nolabels".to_string()
            } else {
                "columns".to_string()
            },
        )],
    );
    for (label, content) in &sections {
        fmt.cr();
        fmt.open("div", &[("class", "columns__col".to_string())]);
        if !headless {
            fmt.cr();
            // 栏目名是**这一栏的标签**，不是正文里的一个标题 —— 与 `::tabs` 那边用
            // `<button>` 同一个道理：这里是分栏，不是文档的章节，插一个 `hN` 进去
            // 会把整页的标题层级打乱（跳级、或与正文标题抢位置）。「标题的样子」
            // 由样式在 `.columns__head` 上给
            fmt.open("div", &[("class", "columns__head".to_string())]);
            fmt.text(label);
            fmt.close("div");
        }
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

    /// `::columns`：每节一栏、同时可见，默认把 [标签] 当栏头
    #[test]
    fn columns_render_one_column_per_section_with_a_head() {
        let html = render("::columns\n  [写法]\n  这边写\n  [效果]\n  那边看\n");
        assert_eq!(html.matches("class=\"columns__col\"").count(), 2, "{html}");
        assert!(html.contains("columns__head"), "{html}");
        assert!(html.contains(">写法<") && html.contains(">效果<"), "{html}");
        // 分栏不是选项卡：不藏任何一节，也没有可点的标签
        assert!(!html.contains("tabs__tab"), "{html}");
        assert!(!html.contains("hidden"), "{html}");

        // 一栏里照样是完整的一份小文档
        let rich = render("::columns\n  [一]\n  **加粗**\n");
        assert!(rich.contains("<strong>加粗</strong>"), "{rich}");

        // labels=off：纯摆版面，栏头不露
        let bare = render("::columns labels=off\n  [一]\n  内容\n");
        assert!(bare.contains("columns--nolabels"), "{bare}");
        assert!(!bare.contains("columns__head"), "{bare}");

        let stray = render("::columns\n  没写标签\n  [一]\n  正文\n");
        assert!(stray.contains("template--problem"), "{stray}");
        let empty = render("::columns\n");
        assert!(empty.contains("template--problem"), "{empty}");
        let bad = render("::columns labels=大概\n  [一]\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
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

    /// `::theme` 里的 `::css` 不直接写 `<style>`，而是输出 theme-asset 占位容器
    #[test]
    fn theme_css_becomes_asset_container() {
        let html = render(
            "::theme\n  [light]\n  ::css\n    .a { color: red; }\n  [dark]\n  ::css\n    .a { color: blue; }\n",
        );
        assert!(html.contains("theme-asset theme-css"), "{html}");
        assert!(html.contains("data-theme=\"light\""), "{html}");
        assert!(html.contains("data-theme=\"dark\""), "{html}");
        assert!(
            !html.contains("<style>"),
            "theme 内部不该直接输出 <style>：{html}"
        );
        assert!(html.contains(".a { color: red; }"), "{html}");
        assert!(html.contains(".a { color: blue; }"), "{html}");
    }

    /// `::theme` 里的 `::js` 不直接写 `<script>`，而是输出 theme-asset 占位容器
    #[test]
    fn theme_js_becomes_asset_container() {
        let html = render(
            "::theme\n  [light]\n  ::js\n    console.log('light');\n  [dark]\n  ::js\n    console.log('dark');\n",
        );
        assert!(html.contains("theme-asset theme-js"), "{html}");
        assert!(
            !html.contains("<script>"),
            "theme 内部不该直接输出 <script>：{html}"
        );
        assert!(html.contains("console.log('light');"), "{html}");
        assert!(html.contains("console.log('dark');"), "{html}");
    }

    /// `::theme` 里的 `::html js` 提取脚本到 theme-js 容器，其余 HTML 正常输出
    #[test]
    fn theme_html_js_extracts_script() {
        let html = render(
            "::theme\n  [light]\n  ::html js\n    <script>alert('light')</script>\n  [dark]\n  ::html js\n    <script>alert('dark')</script>\n",
        );
        assert!(html.contains("theme-asset theme-js"), "{html}");
        assert!(html.contains("alert('light')"), "{html}");
        assert!(html.contains("alert('dark')"), "{html}");
        assert!(
            !html.contains("<script>"),
            "theme 内部不该直接输出 <script>：{html}"
        );
    }

    /// `::theme` 外面的 `::css` 仍然正常输出 `<style>`，不受 theme slot 影响
    #[test]
    fn css_outside_theme_still_outputs_style() {
        let html = render("::css\n  .a { color: red; }\n");
        assert!(html.contains("<style>"), "{html}");
        assert!(
            !html.contains("theme-asset"),
            "theme 外不该输出 theme-asset：{html}"
        );
    }

    /// `::theme` 内部的资源不进入站点资源清单 —— 它们由主题脚本动态注入
    #[test]
    fn theme_assets_are_not_collected() {
        use crate::{Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::theme\n  [light]\n  ::css\n    .a { }\n  ::js\n    console.log(1);\n  [dark]\n  ::css\n    .b { }\n",
            &RenderContext::new(),
            "assets",
        );
        assert!(
            rendered.assets.is_empty(),
            "theme 内部资源不该进清单：{:?}",
            rendered.assets
        );
        assert!(rendered.html.contains("theme-asset"), "{}", rendered.html);
        // 没有外链（不生成 `<link>` / `<script src>`）
        assert!(!rendered.html.contains("<link"), "{}", rendered.html);
        assert!(!rendered.html.contains("<script src="), "{}", rendered.html);
    }
}
