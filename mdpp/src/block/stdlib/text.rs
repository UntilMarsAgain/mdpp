//! 只写文字的块：`aside` / `banner` / `fields`，以及原样文本原语 `text`；
//! `quote` / `title` / `signature` 已迁到标准库模板，见 [`crate::stdlib`]。
//!
//! 除 `fields` 与 `text` 外，内容都**照常按 markdown 解析** —— 子节点已经挂在
//! `node.children` 上，渲染器只管 `fmt.contents(&node.children)`，所以块里能写链接、
//! 列表、内部链接，也能再嵌一个块。要「不解析」的块在 [`super::embeds`] 与
//! [`render_text`]。
//!
//! `fields` 是本组唯一的例外：内容是**逐行拆出来**的属性表，值用原文 —— 它是
//! 「查参数」用的，一行一项最清楚。
//!
//! 参数只管**外框**长什么样：位置（`align`）、颜色（`color=`）、高度（`height=`）、
//! 圆角。

use super::super::dispatch::render_problem;
use super::{hex_why, normalize_hex, size_rule, text_on};
use crate::registry::Block;
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::{Node, Renderer};

/// `::text` —— 原样文本原语：正文**不按 markdown 解析**，HTML 记号也按字面转义。
///
/// 它不产生自己的元素，只把文字写出去；需要外层结构时由模板用 `::element` 包住。
/// 这是给「标题、标签、字段值」这类必须保持字面的内容用的，不适合写正文。
pub(super) fn render_text(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    if !block.params.is_empty() {
        render_problem(block, fmt, "`::text` 不接受参数：正文会按字面输出");
        return;
    }

    let text = block.body.trim();
    if text.is_empty() {
        render_problem(block, fmt, "内容是空的：::text 要写点字");
        return;
    }

    fmt.cr();
    fmt.text(text);
    fmt.cr();
}

/// `::aside title="十四门桥"` —— 右侧的信息栏，**内容照常按 markdown 渲染**。
///
/// 用 `node.children`（已经解析好的内容）而不是原文：信息栏里也要能写链接、强调、列表。
/// 它是一条**浮动**的栏：正文会绕着它走，这是信息栏该有的样子；窄屏上由样式取消浮动。
///
/// `align=` 决定位置：
/// - 不写（或 `right`）：右侧浮动，正文绕着走（默认）
/// - `center`：居中小框，不浮动，宽度随内容，正文不绕 —— 适合做维基式的「居中信息框」
/// - `left`：左侧浮动
pub(super) fn render_aside(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let mut class = "aside".to_string();
    let align = block.param("align").map(str::trim).map(str::to_lowercase);
    match align.as_deref() {
        None | Some("right") => {}
        Some("center") => class.push_str(" aside--center"),
        Some("left") => class.push_str(" aside--left"),
        Some(_) => {
            render_problem(block, fmt, "align= 只认 right / center / left");
            return;
        }
    }

    fmt.cr();
    fmt.open("aside", &[("class", class)]);
    if let Some(title) = block.param("title") {
        fmt.open("p", &[("class", "aside__title".to_string())]);
        fmt.text(title);
        fmt.close("p");
    }
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close("aside");
    fmt.cr();
}

/// `::fields` —— 左右两栏的属性表：每行 `标签 | 值`。
///
/// 值用**原文**（不解析 markdown）。没有 `|` 的行把整行当标签、值留空 ——
/// 宁可少一栏，也不丢作者写下的字。
pub(super) fn render_fields(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    fmt.cr();
    fmt.open("table", &[("class", "fields".to_string())]);
    fmt.open("tbody", &[]);
    for line in block.body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (label, value) = match line.split_once('|') {
            Some((label, value)) => (label.trim(), value.trim()),
            None => (line, ""),
        };
        fmt.cr();
        fmt.open("tr", &[]);
        fmt.open("th", &[]);
        fmt.text(label);
        fmt.close("th");
        fmt.open("td", &[]);
        fmt.text(value);
        fmt.close("td");
        fmt.close("tr");
    }
    fmt.cr();
    fmt.close("tbody");
    fmt.close("table");
    fmt.cr();
}

/// `::banner color=#0055a4` —— 一条方框标题带（信息栏里那种居中的标题条）。
///
/// 文案来自块内容（一行），或由 `text=` 给出；两者**都按 markdown 渲染**，
/// 所以加粗、链接、内部链接在这里同样有效。
/// `color=` 给底色（只认 `#rgb` / `#rrggbb`），字色由 [`text_on`] 按亮度定，
/// 免得深底配深字。
/// 名字取 `banner` 而不是「方框标题」之类：它是一个**横条**，越短越不容易与别的块混淆。
pub(super) fn render_banner(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：标题带要写一行字，或用 text=… 给");
        return;
    }

    // 颜色只认十六进制：这个值会写进 style 属性，宽松了就等于允许塞任意声明。
    // 另立一个 banner--tinted 类：横条里那点行内代码的芯子底色要按「这条横条
    // 有没有自己的底色」来定（见 mdpp.css），靠 style 属性去猜既绕也不可靠。
    let mut class = "banner".to_string();
    let mut style = String::new();
    if let Some(color) = block.param("color") {
        let Some(hex) = normalize_hex(color) else {
            render_problem(block, fmt, &hex_why("color", None));
            return;
        };
        style.push_str(&format!("background: {hex}; color: {};", text_on(&hex)));
        class.push_str(" banner--tinted");
    }

    // 高度给的是**最小**高度：字大了往下长，不会被裁掉；给了它就顺带把内容在条里摆正
    if let Some(height) = block.param("height") {
        let Some(rule) = size_rule("min-height", height) else {
            render_problem(
                block,
                fmt,
                "height= 只认数字加单位（px / % / em / rem / vh / vw），例如 height=120px",
            );
            return;
        };
        style.push_str(&rule);
        class.push_str(" banner--sized");
    }

    // 圆角默认开着；要直角就写 rounded=off
    let rounded = block
        .param("rounded")
        .map(|value| value.trim().to_lowercase());
    match rounded.as_deref() {
        None | Some("" | "on" | "true" | "yes") => {}
        Some("off" | "false" | "no" | "0") => class.push_str(" banner--square"),
        Some(_) => {
            render_problem(block, fmt, "rounded= 只认 off（要直角）；不写就是圆角");
            return;
        }
    }

    // mini 模式：不占整行，变成行内块，适合嵌进正文或 ::aside 里当分节标题
    let mini = block.param("mini").map(str::trim).map(str::to_lowercase);
    match mini.as_deref() {
        Some("on" | "true" | "yes" | "") => class.push_str(" banner--mini"),
        None | Some("off" | "false" | "no" | "0") => {}
        Some(_) => {
            render_problem(block, fmt, "mini= 只认 on / off");
            return;
        }
    }

    let mut attrs: Vec<(&str, String)> = vec![("class", class)];
    if !style.is_empty() {
        attrs.push(("style", style));
    }

    fmt.cr();
    inline_or_blocks_in(node, &attrs, fmt);
    fmt.cr();
}

/// 「一行内容」的块（横条）里的东西该按行内还是按块渲染
enum InlineOrBlocks<'a> {
    /// 一个段落：渲染它里面的行内内容
    Inline(&'a [Node]),
    /// 别的：按块渲染
    Blocks,
}

/// 只有一个段落时按行内渲染 —— 这是「一条横条 / 一个大标题」该有的样子：
/// 它们本身不是容器，里面再套一个块（`<p>` 里套 `<p>`）就散了
fn inline_or_blocks<'a>(node: &'a Node) -> InlineOrBlocks<'a> {
    match node.children.as_slice() {
        [only] if only.is::<Paragraph>() => InlineOrBlocks::Inline(&only.children),
        _ => InlineOrBlocks::Blocks,
    }
}

/// 把内容摆进 `attrs` 指定的那两个元素之一：**一行就套 `<p>`，写成块了就套 `<div>`**。
///
/// 横条里的内容该按行内还是按块渲染，只在这里定一份：
///
/// - 单段时只取它的**行内内容**摊平放进来 —— 横条与标题自己不是容器，里面再套一个
///   `<p>` 就散了；
/// - 写成好几段、或者塞了别的块：那已经不是一条横条/一个标题了，但内容不能丢，
///   于是换 `<div>` 照块渲染 —— 块塞进 `<p>` 是无效 HTML，浏览器会自己拆，
///   拆成什么样就不由我们说了。
///
/// 开合那两个元素外面的两次 `fmt.cr()` 由调用方给
fn inline_or_blocks_in(node: &Node, attrs: &[(&str, String)], fmt: &mut dyn Renderer) {
    match inline_or_blocks(node) {
        InlineOrBlocks::Inline(inline) => {
            fmt.open("p", attrs);
            fmt.contents(inline);
            fmt.close("p");
        }
        InlineOrBlocks::Blocks => {
            fmt.open("div", attrs);
            fmt.cr();
            fmt.contents(&node.children);
            fmt.close("div");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 标题带里的内容按 markdown 渲染：加粗、内部链接都算数
    #[test]
    fn banner_renders_markdown_in_its_body() {
        let html = render("::banner\n  这是**加粗**与[[目标]]\n");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
        assert!(html.contains("wikilink"), "{html}");
        // 一条横条：不该在横条里再套一个段落
        assert!(!html.contains("<p><p>"), "{html}");
        assert_eq!(html.matches("class=\"banner\"").count(), 1, "{html}");
    }

    /// `text=` 同样按 markdown 渲染
    #[test]
    fn banner_renders_markdown_in_its_text_parameter() {
        let html = render("::banner text=\"**加粗**的标题\"\n");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
    }

    /// `::banner` 的高度与圆角：高度是**最小**高度，圆角默认开着
    #[test]
    fn banner_takes_a_height_and_a_square_corner() {
        let sized = render("::banner text=\"标题\" height=120px\n");
        assert!(sized.contains("min-height: 120px"), "{sized}");
        assert!(sized.contains("banner--sized"), "{sized}");
        assert!(
            !sized.contains("banner--square"),
            "不写 rounded= 就是圆角：{sized}"
        );

        let square = render("::banner text=\"标题\" rounded=off\n");
        assert!(square.contains("banner--square"), "{square}");

        let bad = render("::banner text=\"标题\" height=很久\n");
        assert!(bad.contains("template--problem"), "{bad}");
        let bad = render("::banner text=\"标题\" rounded=也许\n");
        assert!(bad.contains("template--problem"), "{bad}");
    }

    /// 信息栏的内容照常按 markdown 解析 —— 改成输出原文就丢了强调与链接
    #[test]
    fn aside_renders_its_body_as_markdown() {
        let html = render("::aside title=\"十四门桥\"\n  正文里的**强调**\n");
        assert!(html.contains(r#"<aside class="aside">"#), "{html}");
        assert!(html.contains("aside__title"), "{html}");
        assert!(
            html.contains("<strong>强调</strong>"),
            "信息栏的内容仍按 markdown 解析：{html}"
        );
    }

    /// 逐行拆成标签与值；没有 `|` 的行整行当标签、值留空 —— 一个字都不丢
    #[test]
    fn fields_split_label_and_value() {
        let html = render("::fields\n  地址 | 福建省某村\n  分类 | 古建筑\n");
        assert!(html.contains(r#"<table class="fields">"#), "{html}");
        assert!(html.contains("<th>地址</th>"), "{html}");
        assert!(html.contains("<td>福建省某村</td>"), "{html}");

        // 没有 `|` 的行：整行当标签、值留空 —— 宁可少一栏，也不丢作者写下的字
        let one = render("::fields\n  只有标签\n");
        assert!(one.contains("<th>只有标签</th>"), "{one}");
    }

    /// `::fields` 的值用原文：那里的 markdown 记号不该被解析
    #[test]
    fn fields_do_not_parse_markdown_in_their_values() {
        let html = render("::fields\n  记号 | *星号* 与 $a_1$\n");
        assert!(html.contains("*星号* 与 $a_1$"), "{html}");
        assert!(!html.contains("<em>"), "{html}");
    }

    /// 横条的文案两种写法都认（块里一行，或 `text=`）；都没有时报用法问题，不画空带
    #[test]
    fn banner_takes_the_body_or_the_text_param() {
        let html = render("::banner\n  福建省文物保护单位\n");
        assert!(
            html.contains(r#"<p class="banner">福建省文物保护单位</p>"#),
            "{html}"
        );
        let param = render("::banner text=另一种写法\n");
        assert!(param.contains("另一种写法"), "{param}");
        // 空内容要给提示，而不是画一条空带
        let empty = render("::banner\n");
        assert!(empty.contains("template--problem"), "{empty}");
    }

    /// `color=` 给底色、字色按亮度另配（深底配深字是最常见的自挖坑）；
    /// `#abc` 这种简写也认，乱写的直接拒 —— 这个值会进 style 属性，宽松不得
    #[test]
    fn banner_color_sets_background_and_readable_text() {
        // 深蓝底 → 浅色字
        let dark = render("::banner color=#0055a4\n  深蓝底\n");
        assert!(dark.contains("background: #0055a4"), "{dark}");
        assert!(dark.contains("color: #f5f5f5"), "{dark}");

        // 浅黄底 → 深色字（深底配深字是最常见的自挖坑）
        let light = render("::banner color=#ffe680\n  浅黄底\n");
        assert!(light.contains("color: #101010"), "{light}");

        // `#abc` 这种简写也认，并规范成六位
        let short = render("::banner color=#0af\n  简写\n");
        assert!(short.contains("background: #00aaff"), "{short}");

        // 乱写的颜色：不猜，直接说清楚（值会进 style 属性，不能宽松）
        let bad = render("::banner color=red\n  乱写\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(!bad.contains("style="), "不该拼出 style 属性：{bad}");
    }

    /// `::aside align=center`：不浮动，居中小框，宽度随内容
    #[test]
    fn aside_center_is_inline_block() {
        let html = render("::aside title=中心 align=center\n  居中信息框\n");
        assert!(
            html.contains(r#"<aside class="aside aside--center">"#),
            "{html}"
        );
        assert!(html.contains("居中信息框"), "{html}");
    }

    /// `::aside align=left`：左浮动
    #[test]
    fn aside_left_floats_left() {
        let html = render("::aside title=左边 align=left\n  左侧信息框\n");
        assert!(
            html.contains(r#"<aside class="aside aside--left">"#),
            "{html}"
        );
    }

    /// `::aside align=` 认不出的值报错
    #[test]
    fn aside_align_rejects_bad_value() {
        let html = render("::aside align=maybe\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("align="), "{html}");
    }

    /// `::banner mini`：变成行内块，不占整行
    #[test]
    fn banner_mini_is_inline_block() {
        let html = render("::banner mini\n  小横条\n");
        assert!(
            html.contains(r#"<p class="banner banner--mini">小横条</p>"#),
            "{html}"
        );
        let param = render("::banner mini=on text=另一种写法\n");
        assert!(param.contains("banner--mini"), "{param}");
    }

    /// `::banner mini=off`：不开迷你，仍是普通横条
    #[test]
    fn banner_mini_off_is_normal() {
        let html = render("::banner mini=off\n  普通横条\n");
        assert!(html.contains(r#"<p class="banner">普通横条</p>"#), "{html}");
        assert!(!html.contains("banner--mini"), "{html}");
    }

    /// `::banner mini=` 认不出的值报错
    #[test]
    fn banner_mini_rejects_bad_value() {
        let html = render("::banner mini=也许\n  内容\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("mini="), "{html}");
    }

    /// `::text` 只做 HTML 转义，不解析 markdown
    #[test]
    fn text_is_literal_and_escaped() {
        let html = render("::text\n  <b>**不解析**</b>\n");
        assert!(html.contains("&lt;b&gt;**不解析**&lt;/b&gt;"), "{html}");
        assert!(!html.contains("<strong>"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
    }

    /// `::text` 可以放进 `::element`，由外层提供结构
    #[test]
    fn text_can_sit_inside_an_element() {
        let html = render("::element tag=p\n  ::text\n    原样 <tag>\n");
        assert!(html.contains("<p>"), "{html}");
        assert!(html.contains("原样 &lt;tag&gt;"), "{html}");
    }

    /// 空正文与参数都算用法问题
    #[test]
    fn text_rejects_empty_content_and_parameters() {
        let empty = render("::text\n");
        assert!(empty.contains("template--problem"), "{empty}");

        let param = render("::text class=foo\n  内容\n");
        assert!(param.contains("template--problem"), "{param}");
    }
}
