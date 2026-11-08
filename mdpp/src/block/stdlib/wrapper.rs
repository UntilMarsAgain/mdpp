//! 两个基础包装块：`::surface` 与 `::element`。
//!
//! - `::surface` 是**只负责样式值的盒子**：颜色、字色、高度、圆角、紧凑模式都经过
//!   校验；动态颜色与字色对比不可能由纯文本模板算出来，所以这一层留在 Rust。
//!   class **由调用者自己写**：写了就用调用者的那一串，不再额外塞 `surface`，
//!   也不自动生成 `surface--*`。高度、圆角、mini 落成行内样式，类名只归模板管。
//!   `tag=auto` 按正文形状选 `p` / `div`，给 `banner` 一类块用。
//!   这里的 `text=` 是**字色**（`auto` 或十六进制），不是正文替换；扫描器对
//!   `surface` 不走 `text=` 那条正文捷径。
//! - `::element` 是 `::class` 的替代品：可以选标签、挂 class、`role`、`aria-*` 与
//!   `data-mdpp-*`，但不开放 `style` / `id` / 任意属性。模板需要结构时用它，不需要
//!   为了一个类名再写一个 `::class`。
//!
//! 两者都只包内容，不改变内容本身的 markdown 解析。

use std::collections::HashSet;

use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use super::{hex_why, normalize_hex, size_rule, text_on};
use crate::block::fill;
use crate::registry::Block;

/// `::surface` 可用的标签。样式盒子是块级容器，`span` / `p` 留给行内用法。
const SURFACE_TAGS: &[&str] = &["div", "span", "section", "aside", "p"];

/// `::element` 可用的标签。刻意不给 `script` / `style` / `iframe` / `form`，
/// 也不给 `a` / `img` / `video` 这类需要单独校验 URL 属性的标签。
const ELEMENT_TAGS: &[&str] = &[
    "div",
    "span",
    "section",
    "aside",
    "article",
    "header",
    "footer",
    "nav",
    "main",
    "figure",
    "figcaption",
    "blockquote",
    "button",
    "details",
    "summary",
    "dl",
    "dt",
    "dd",
    "time",
    "mark",
    "small",
    "strong",
    "em",
    "cite",
    "q",
    "s",
    "del",
    "ins",
    "kbd",
    "samp",
    "var",
    "sub",
    "sup",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "ul",
    "ol",
    "li",
    "p",
    "pre",
    "code",
];

/// `::surface color=#0055a4 text=auto rounded=on height=120px mini=off class=... [tag=auto] [inline=on]`
///
/// `class=` 写了就按原样使用（不写默认 `surface`）；`surface` 不再追加
/// `surface--sized` / `surface--square` / `surface--mini`，这些效果改为行内样式。
/// `tag=auto` 按正文形状选标签：单段落 `p`，多段/含块 `div`。
pub(super) fn render_surface(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：::surface 要包裹点东西");
        return;
    }

    // class 归调用者。写了就按原样用；没写才落到默认的 `surface`。
    // 这里不追加 `surface--*`：修饰类与「有没有某个参数」属于模板的 `::if`，
    // `surface` 只把参数换算成经过校验的行内样式。
    let class = block
        .param("class")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("surface")
        .to_string();
    let mut style = String::new();

    let color = block
        .param("color")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(color) = color {
        let Some(hex) = normalize_hex(color) else {
            render_problem(block, fmt, &hex_why("color", None));
            return;
        };
        push_decl(&mut style, &format!("background: {hex};"));
        match block
            .param("text")
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            None => push_decl(&mut style, &format!("color: {};", text_on(&hex))),
            Some(text) if text.eq_ignore_ascii_case("auto") => {
                push_decl(&mut style, &format!("color: {};", text_on(&hex)))
            }
            Some(text) => {
                let Some(fg) = normalize_hex(text) else {
                    render_problem(block, fmt, &hex_why("text", None));
                    return;
                };
                push_decl(&mut style, &format!("color: {fg};"));
            }
        }
    } else if let Some(text) = block
        .param("text")
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("auto"))
    {
        let Some(fg) = normalize_hex(text) else {
            render_problem(block, fmt, &hex_why("text", None));
            return;
        };
        push_decl(&mut style, &format!("color: {fg};"));
    }

    if let Some(height) = block
        .param("height")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some(rule) = size_rule("min-height", height) else {
            render_problem(
                block,
                fmt,
                "height= 只认数字加单位（px / % / em / rem / vh / vw），例如 height=120px",
            );
            return;
        };
        push_decl(&mut style, &rule);
        // 原来由 `.surface--sized` 负责的居中，现在直接落成样式
        push_decl(
            &mut style,
            "display: flex; align-items: center; justify-content: center;",
        );
    }

    match on_off(block.param("rounded"), true) {
        Ok(true) => {}
        Ok(false) => push_decl(&mut style, "border-radius: 0;"),
        Err(()) => {
            render_problem(block, fmt, "rounded= 只认 on / off");
            return;
        }
    }
    match on_off(block.param("mini"), false) {
        Ok(true) => push_decl(
            &mut style,
            "font-size: 0.9em; padding: 0.35rem 0.7rem; margin: 0.3em 0; \
             line-height: 1.4; border-radius: 0.2rem;",
        ),
        Ok(false) => {}
        Err(()) => {
            render_problem(block, fmt, "mini= 只认 on / off");
            return;
        }
    }

    if let Err(why) = only_params(
        block,
        &[
            "color", "text", "rounded", "height", "mini", "class", "tag", "inline",
        ],
    ) {
        render_problem(block, fmt, &why);
        return;
    }
    let auto = block.param("tag").map(str::trim) == Some("auto");
    let (tag, inline) = if auto {
        auto_tag(node)
    } else {
        let tag = match tag_of(block, SURFACE_TAGS) {
            Ok(tag) => tag,
            Err(why) => {
                render_problem(block, fmt, &why);
                return;
            }
        };
        (tag, fill::flag(block, "inline"))
    };

    let mut attrs: Vec<(&str, String)> = vec![("class", class)];
    if !style.is_empty() {
        attrs.push(("style", style));
    }
    fmt.cr();
    fmt.open(tag, &attrs);
    fmt.cr();
    contents(node, fmt, inline);
    fmt.cr();
    fmt.close(tag);
    fmt.cr();
}

/// `::element tag=div|auto class=... role=... aria-*=... data-mdpp-*=... [inline=on]`
pub(super) fn render_element(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：::element 要包裹点东西");
        return;
    }

    let auto = block.param("tag").map(str::trim) == Some("auto");
    let (tag, inline) = if auto {
        auto_tag(node)
    } else {
        let tag = match tag_of(block, ELEMENT_TAGS) {
            Ok(tag) => tag,
            Err(why) => {
                render_problem(block, fmt, &why);
                return;
            }
        };
        (tag, fill::flag(block, "inline"))
    };

    let mut attrs: Vec<(&str, String)> = Vec::new();
    let mut seen = HashSet::new();
    for (key, value) in &block.params {
        match key.as_str() {
            "tag" | "inline" => {}
            "class" => {
                if !value.trim().is_empty() {
                    attrs.push(("class", value.trim().to_string()));
                }
            }
            "role" => attrs.push(("role", value.clone())),
            key if key.starts_with("aria-") || key.starts_with("data-mdpp-") => {
                attrs.push((key, value.clone()));
            }
            _ => {
                render_problem(
                    block,
                    fmt,
                    &format!(
                        "`::element` 不认参数 `{key}`：只允许 tag / class / inline / role / aria-* / data-mdpp-*"
                    ),
                );
                return;
            }
        }
        if !seen.insert(key.as_str()) {
            render_problem(block, fmt, &format!("`::element` 的参数 `{key}` 写了两次"));
            return;
        }
    }

    fmt.cr();
    fmt.open(tag, &attrs);
    fmt.cr();
    contents(node, fmt, inline);
    fmt.cr();
    fmt.close(tag);
    fmt.cr();
}

/// `tag=auto`：单段落用 `<p>` 并摊平行内内容，多段/含块用 `<div>`。
///
/// 这是 `banner` 一类「一行是横条、多段是容器」的块留下的结构判断；
/// 模板看不到解析后的节点，所以放在包装原语里。
fn auto_tag(node: &Node) -> (&'static str, bool) {
    if matches!(node.children.as_slice(), [only] if only.is::<Paragraph>()) {
        ("p", true)
    } else {
        ("div", false)
    }
}

/// `inline=on` 时把单段正文直接写进行内，避免 `<p>` 里再套 `<p>`。
fn contents(node: &Node, fmt: &mut dyn Renderer, inline: bool) {
    if inline
        && let [only] = node.children.as_slice()
        && only.is::<Paragraph>()
    {
        fmt.contents(&only.children);
    } else {
        fmt.contents(&node.children);
    }
}

/// 解析 `tag=`；没有就用 `div`，不在白名单里就报错。
fn tag_of<'a>(block: &'a Block, allowed: &[&str]) -> Result<&'a str, String> {
    let tag = block
        .param("tag")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("div");
    if allowed.contains(&tag) {
        Ok(tag)
    } else {
        Err(format!("tag= 只认 {}，写成了 `{tag}`", allowed.join(" / ")))
    }
}

/// 往 style 串里接一条声明，中间留一个空格；空串时不留前导空格。
fn push_decl(style: &mut String, declaration: &str) {
    if !style.is_empty() {
        style.push(' ');
    }
    style.push_str(declaration);
}

/// `on` / `off` 一类的开关；`None` 用默认值。
fn on_off(value: Option<&str>, default: bool) -> Result<bool, ()> {
    match value.map(str::trim).map(str::to_lowercase).as_deref() {
        // 裸 flag 现在是 `key=on`；显式 `key=` / 模板可选参数缺失才是空值，
        // 空值按默认值处理，不能当成 true —— 否则模板里的 `mini={{?@mini}}`
        // 会在没传 mini 时把它打开。
        None | Some("") => Ok(default),
        Some("on") | Some("true") | Some("yes") | Some("1") => Ok(true),
        Some("off") | Some("false") | Some("no") | Some("0") => Ok(false),
        Some(_) => Err(()),
    }
}

/// 只允许列出来的参数，其余报错；`tag` 在调用方已经单独处理。
fn only_params(block: &Block, allowed: &[&str]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (key, _) in &block.params {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "`::surface` 不认参数 `{key}`：只允许 {}",
                allowed.join(" / ")
            ));
        }
        if !seen.insert(key.as_str()) {
            return Err(format!("`::surface` 的参数 `{key}` 写了两次"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// `::surface` 的底色、字色、尺寸与调用者写的 class 一起落出来
    #[test]
    fn surface_carries_validated_style() {
        let html = render(
            "::surface color=#0055a4 text=auto height=120px rounded=off class=card\n  正文\n",
        );
        // 写了 class 就用调用者的那一串，不再自动追加 surface / surface--*
        assert!(html.contains("class=\"card\""), "{html}");
        assert!(!html.contains("surface--"), "{html}");
        assert!(html.contains("background: #0055a4;"), "{html}");
        assert!(html.contains("color: #f5f5f5;"), "{html}");
        assert!(html.contains("min-height: 120px;"), "{html}");
        assert!(
            html.contains("display: flex; align-items: center; justify-content: center;"),
            "{html}"
        );
        assert!(html.contains("border-radius: 0;"), "{html}");
        // `text=` 是字色，不是正文替换：正文还得在
        assert!(html.contains("<p>正文</p>"), "{html}");
    }

    /// 没写 class 时才落到默认的 `surface`；类名不再由 surface 拼修饰类
    #[test]
    fn surface_defaults_to_the_surface_class() {
        let html = render("::surface\n  正文\n");
        assert!(html.contains("class=\"surface\""), "{html}");
    }

    /// `mini` 现在是行内样式，不再生成 `surface--mini`
    #[test]
    fn surface_mini_is_an_inline_style() {
        let html = render("::surface mini=on\n  正文\n");
        assert!(html.contains("font-size: 0.9em;"), "{html}");
        assert!(!html.contains("surface--mini"), "{html}");
    }

    /// `tag=p` 也允许：banner 一类后来可以把它当行内盒子用
    #[test]
    fn surface_can_use_a_paragraph_tag() {
        let html = render("::surface tag=p class=banner inline=on\n  正文\n");
        assert!(html.contains("<p class=\"banner\">"), "{html}");
        assert!(html.contains("正文"), "{html}");
        assert!(!html.contains("<p><p>"), "{html}");
    }

    /// `tag=auto`：单段落用 p，多段用 div —— 这是 banner 需要的结构判断
    #[test]
    fn auto_tag_follows_the_content_shape() {
        let single = render("::surface tag=auto class=banner\n  一行\n");
        assert!(single.contains("<p class=\"banner\">"), "{single}");
        assert!(!single.contains("<div class=\"banner\">"), "{single}");
        assert!(!single.contains("<p><p>"), "{single}");

        let multi = render("::surface tag=auto class=banner\n  第一段\n\n  第二段\n");
        assert!(multi.contains("<div class=\"banner\">"), "{multi}");
        assert!(multi.contains("<p>第一段</p>"), "{multi}");
        assert!(multi.contains("<p>第二段</p>"), "{multi}");

        let element = render("::element tag=auto class=banner\n  一行\n");
        assert!(element.contains("<p class=\"banner\">"), "{element}");
    }

    /// 裸 flag 已经解析成 `on`；显式空值才走默认值：
    /// `mini=` 不能把 mini 打开
    #[test]
    fn an_explicit_empty_flag_uses_the_default() {
        let bare = render("::surface mini\n  正文\n");
        assert!(bare.contains("font-size: 0.9em;"), "{bare}");

        let empty = render("::surface mini=\n  正文\n");
        assert!(!empty.contains("font-size: 0.9em;"), "{empty}");
    }

    /// 颜色和尺寸的坏值要报错，不写进 style
    #[test]
    fn surface_refuses_bad_style_values() {
        for source in [
            "::surface color=blue\n  正文\n",
            "::surface text=#12\n  正文\n",
            "::surface height=1em; background: red\n  正文\n",
            "::surface rounded=maybe\n  正文\n",
            "::surface color=#000 color=#fff\n  正文\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{source:?}：{html}");
        }
    }

    /// `::element` 输出选定的标签、class、role、aria 与 data-mdpp 属性
    #[test]
    fn element_carries_whitelisted_attributes() {
        let html = render(
            "::element tag=button class=\"tab\" role=tab aria-selected=true data-mdpp-option=0\n  甲\n",
        );
        assert!(
            html.contains("<button class=\"tab\" role=\"tab\""),
            "{html}"
        );
        assert!(html.contains("aria-selected=\"true\""), "{html}");
        assert!(html.contains("data-mdpp-option=\"0\""), "{html}");
    }

    /// `::element` 只开白名单：任意属性、style、id 与危险标签都拒绝
    #[test]
    fn element_refuses_unknown_attributes_and_tags() {
        for source in [
            "::element style=\"color:red\"\n  正文\n",
            "::element id=x\n  正文\n",
            "::element onclick=x\n  正文\n",
            "::element tag=script\n  正文\n",
            "::element tag=div class=a class=b\n  正文\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{source:?}：{html}");
        }
    }

    /// 结构标签与行内语义标签可以进 element；URL 类标签仍不在白名单里
    #[test]
    fn element_accepts_the_expanded_safe_tags() {
        let html = render(
            "::element tag=details\n  ::element tag=summary\n    标题\n  ::element tag=dl\n    ::element tag=dt\n      甲\n    ::element tag=dd\n      乙\n",
        );
        assert!(html.contains("<details>"), "{html}");
        assert!(html.contains("<summary>"), "{html}");
        assert!(html.contains("<dl>"), "{html}");
        assert!(html.contains("<dt>"), "{html}");
        assert!(html.contains("<dd>"), "{html}");

        for tag in ["script", "style", "iframe", "form", "a", "img"] {
            let html = render(&format!("::element tag={tag}\n  正文\n"));
            assert!(
                html.contains("template--problem"),
                "tag={tag} 不该放行：{html}"
            );
        }
    }

    /// `inline=on` 只把单段正文拆成行内，避免 `<p>` 套 `<p>`
    #[test]
    fn inline_unwraps_a_single_paragraph() {
        let element = render("::element tag=p inline=on\n  文字\n");
        assert!(element.contains("<p>"), "{element}");
        assert!(element.contains("文字"), "{element}");
        assert!(!element.contains("<p><p>"), "{element}");
        assert_eq!(element.matches("<p>").count(), 1, "{element}");

        let surface = render("::surface inline=on\n  文字\n");
        assert!(surface.contains("surface"), "{surface}");
        assert!(surface.contains("文字"), "{surface}");
    }

    /// 两者都保持内容按 markdown 解析
    #[test]
    fn wrappers_keep_markdown_content() {
        let surface = render("::surface\n  **粗**与[[链接]]\n");
        assert!(surface.contains("<strong>粗</strong>"), "{surface}");
        assert!(surface.contains("wikilink"), "{surface}");

        let element = render("::element class=foo\n  **粗**与[[链接]]\n");
        assert!(element.contains("<strong>粗</strong>"), "{element}");
        assert!(element.contains("wikilink"), "{element}");
    }
}
