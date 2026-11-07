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

use super::super::dispatch::{no_leading_content, render_problem, sections};
use crate::registry::Block;
use crate::variants::{ALWAYS, Control, Group};
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

    // 选项值用下标：`0`、`1`……与旧版 `data-tab` 的取值一致，脚本不必换算。
    let mut group = Group::new("tabs", Control::Choice);
    for (index, (label, _)) in sections.iter().enumerate() {
        group = group.option(index.to_string(), *label);
    }

    let mut container = vec![("class", "tabs".to_string())];
    container.extend(group.container_attrs());
    fmt.cr();
    fmt.open("div", &container);
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
        let value = index.to_string();
        // 用真按钮：Tab 键走得到、回车就切（切换的那段脚本认 `role="tablist"`）
        let mut attrs: Vec<(&str, String)> = vec![
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
            // `data-tab` 是旧版留给自定义脚本的钩子；新代码认 `data-mdpp-option`。
            ("data-tab", value.clone()),
            (
                "aria-selected",
                if selected { "true" } else { "false" }.to_string(),
            ),
        ];
        attrs.extend(group.option_attrs(&value));
        fmt.open("button", &attrs);
        fmt.text(label);
        fmt.close("button");
    }
    fmt.cr();
    fmt.close("div");
    fmt.cr();

    for (index, (label, content)) in sections.iter().enumerate() {
        let value = index.to_string();
        let mut attrs: Vec<(&str, String)> = vec![
            ("class", "tabs__panel".to_string()),
            ("role", "tabpanel".to_string()),
            // 旧版属性；新代码认 `data-mdpp-panel`。
            ("data-panel", value.clone()),
            // 屏幕上这一面是藏着的，名字已经写在标签上了；打印时全展开，
            // 就得靠这个字段给每一节起名字（见 `mdpp-cli/assets/mdpp.css` 的
            // `@media print` 那一节）。缺了它，打印出来就是几段没有标题的正文。
            ("data-label", (*label).to_string()),
        ];
        attrs.extend(group.panel_attrs(&value));
        fmt.open("div", &attrs);
        fmt.cr();
        // 这一节里产生的 ::css / ::js 绑到 tabs 组的下标上，切换时才挂载。
        let _scope = group.enter(&value);
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
/// `::theme` 节里嵌套的 `::css`、`::js`、`::html js` 会绑定到这个主题组的
/// `light` / `dark` 选项上，由页面脚本按当前主题选择性挂载。这样浅色节的脚本
/// 不会在深色下执行，反之亦然。
///
/// 页面加载时自动挂载；系统 `prefers-color-scheme` 切换时自动跟随；下游库若
/// 重写了 `<html data-mdpp-theme>`，可调用 `window.mdppVariants.refresh()` 手动刷新。
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
            "" | "both" | "all" => ALWAYS,
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

    let group = Group::new("theme", Control::Theme)
        .option("light", "light")
        .option("dark", "dark");

    let mut container = vec![("class", "theme".to_string())];
    container.extend(group.container_attrs());
    fmt.cr();
    fmt.open("div", &container);
    for (slot, content) in parts {
        let class = match slot {
            "light" => "theme__part theme__part--light",
            "dark" => "theme__part theme__part--dark",
            _ => "theme__part",
        };
        // 这一节里产生的资源绑到 theme 组对应的选项上；`*` 表示两边都挂。
        let _guard = group.enter(slot);
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

        // 前缀里的资源用 `*`，两边都挂载
        let both_css = render("::theme\n  ::css\n    .both { }\n  [dark]\n  只有深色\n");
        assert!(both_css.contains("data-mdpp-value=\"*\""), "{both_css}");

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

    /// `::theme` 里的 `::css` 不直接写 `<style>`，而是输出通用惰性占位符
    #[test]
    fn theme_css_becomes_a_deferred_asset() {
        let html = render(
            "::theme\n  [light]\n  ::css\n    .a { color: red; }\n  [dark]\n  ::css\n    .a { color: blue; }\n",
        );
        assert!(html.contains("data-mdpp-asset=\"css\""), "{html}");
        assert!(html.contains("data-mdpp-group=\"theme-1\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"light\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"dark\""), "{html}");
        assert!(html.contains("data-mdpp-version=\"0.1.0\""), "{html}");
        assert!(
            !html.contains("<style>"),
            "theme 内部不该直接输出 <style>：{html}"
        );
        assert!(html.contains(".a { color: red; }"), "{html}");
        assert!(html.contains(".a { color: blue; }"), "{html}");
    }

    /// `::theme` 里的 `::js` 同样变成惰性资源，并保留组/选项
    #[test]
    fn theme_js_becomes_a_deferred_asset() {
        let html = render(
            "::theme\n  [light]\n  ::js\n    console.log('light');\n  [dark]\n  ::js\n    console.log('dark');\n",
        );
        assert!(html.contains("data-mdpp-asset=\"js\""), "{html}");
        assert!(html.contains("data-mdpp-group=\"theme-1\""), "{html}");
        assert!(
            !html.contains("<script>"),
            "theme 内部不该直接输出 <script>：{html}"
        );
        assert!(html.contains("console.log('light');"), "{html}");
        assert!(html.contains("console.log('dark');"), "{html}");
    }

    /// `::theme` 里的 `::html js` 提取脚本到惰性资源，其余 HTML 正常输出
    #[test]
    fn theme_html_js_extracts_script() {
        let html = render(
            "::theme\n  [light]\n  ::html js\n    <script>alert('light')</script>\n  [dark]\n  ::html js\n    <script>alert('dark')</script>\n",
        );
        assert!(html.contains("data-mdpp-asset=\"js\""), "{html}");
        assert!(html.contains("alert('light')"), "{html}");
        assert!(html.contains("alert('dark')"), "{html}");
        assert!(
            !html.contains("<script>"),
            "theme 内部不该直接输出 <script>：{html}"
        );
    }

    /// `::theme` 外面的 `::css` 仍然正常输出 `<style>`，不受变体作用域影响
    #[test]
    fn css_outside_theme_still_outputs_style() {
        let html = render("::css\n  .a { color: red; }\n");
        assert!(html.contains("<style>"), "{html}");
        assert!(
            !html.contains("data-mdpp-asset"),
            "theme 外不该输出惰性资源：{html}"
        );
    }

    /// 站点模式下 theme 的资源也进清单，只是页面里先给静态链接占位符
    #[test]
    fn theme_assets_are_collected_as_static_files() {
        use crate::{Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::theme\n  [light]\n  ::css\n    .a { }\n  ::js\n    console.log(1);\n  [dark]\n  ::css\n    .b { }\n",
            &RenderContext::new(),
            "assets",
        );
        assert_eq!(rendered.assets.len(), 3, "{:?}", rendered.assets);
        assert!(
            rendered.html.contains("data-mdpp-asset=\"css\""),
            "{}",
            rendered.html
        );
        assert!(
            rendered.html.contains("data-mdpp-href=\"assets/mdpp-"),
            "{}",
            rendered.html
        );
        // 占位符不是已经挂上的标签
        assert!(!rendered.html.contains("<link"), "{}", rendered.html);
        assert!(!rendered.html.contains("<script src="), "{}", rendered.html);
    }

    /// `::tabs` 里的资源也按面板选项绑定，未选中的不会直接执行
    #[test]
    fn tabs_defer_their_assets() {
        let html = render(
            "::tabs\n  [北岸]\n  ::css\n    .north { color: red; }\n  ::js\n    console.log('north');\n  [南岸]\n  ::css\n    .south { color: blue; }\n",
        );
        assert!(html.contains("data-mdpp-group=\"tabs-1\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"0\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"1\""), "{html}");
        assert!(html.contains("data-mdpp-asset=\"css\""), "{html}");
        assert!(html.contains("data-mdpp-asset=\"js\""), "{html}");
        assert!(!html.contains("<style>"), "{html}");
        assert!(!html.contains("<script>"), "{html}");
    }

    /// `::js once` 把一次性策略写进占位符
    #[test]
    fn tabs_can_mark_a_script_once() {
        let html = render("::tabs\n  [一]\n  ::js once\n    console.log(1);\n");
        assert!(html.contains("data-mdpp-once=\"once\""), "{html}");
    }
}
