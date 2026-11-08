//! 两个基础包装块：`::surface` 与 `::element`。
//!
//! - `::surface` 是**带受控样式的盒子**：底色、字色、高度、圆角、紧凑模式都经过
//!   校验；动态颜色与字色对比不可能由纯文本模板算出来，所以这一层留在 Rust。
//!   这里的 `text=` 是**字色**（`auto` 或十六进制），不是正文替换；扫描器对
//!   `surface` 不走 `text=` 那条正文捷径。
//! - `::element` 是 `::class` 的替代品：可以选标签、挂 class、`role`、`aria-*` 与
//!   `data-mdpp-*`，但不开放 `style` / `id` / 任意属性。模板需要结构时用它，不需要
//!   为了一个类名再写一个 `::class`。
//!
//! 两者都只包内容，不改变内容本身的 markdown 解析。

use std::collections::HashSet;

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use super::{hex_why, normalize_hex, size_rule, text_on};
use crate::registry::Block;

/// `::surface` 可用的标签。样式盒子是块级容器，`span` 留给行内用法。
const SURFACE_TAGS: &[&str] = &["div", "span", "section", "aside"];

/// `::element` 可用的标签。刻意不给 `script` / `style` / `iframe` / `form`。
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
    "button",
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

/// `::surface color=#0055a4 text=auto rounded=on height=120px mini=off class=...`
pub(super) fn render_surface(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：::surface 要包裹点东西");
        return;
    }

    let mut class = "surface".to_string();
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
        style.push_str(&format!("background: {hex};"));
        match block
            .param("text")
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            None => style.push_str(&format!(" color: {};", text_on(&hex))),
            Some(text) if text.eq_ignore_ascii_case("auto") => {
                style.push_str(&format!(" color: {};", text_on(&hex)));
            }
            Some(text) => {
                let Some(fg) = normalize_hex(text) else {
                    render_problem(block, fmt, &hex_why("text", None));
                    return;
                };
                style.push_str(&format!(" color: {fg};"));
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
        style.push_str(&format!("color: {fg};"));
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
        style.push_str(&rule);
        class.push_str(" surface--sized");
    }

    match on_off(block.param("rounded"), true) {
        Ok(true) => {}
        Ok(false) => class.push_str(" surface--square"),
        Err(()) => {
            render_problem(block, fmt, "rounded= 只认 on / off");
            return;
        }
    }
    match on_off(block.param("mini"), false) {
        Ok(true) => class.push_str(" surface--mini"),
        Ok(false) => {}
        Err(()) => {
            render_problem(block, fmt, "mini= 只认 on / off");
            return;
        }
    }

    if let Some(extra) = block.param("class").map(str::trim) {
        if !extra.is_empty() {
            class.push(' ');
            class.push_str(extra);
        }
    }

    if let Err(why) = only_params(
        block,
        &["color", "text", "rounded", "height", "mini", "class", "tag"],
    ) {
        render_problem(block, fmt, &why);
        return;
    }
    let tag = match tag_of(block, SURFACE_TAGS) {
        Ok(tag) => tag,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    let mut attrs: Vec<(&str, String)> = vec![("class", class)];
    if !style.is_empty() {
        attrs.push(("style", style));
    }
    fmt.cr();
    fmt.open(tag, &attrs);
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close(tag);
    fmt.cr();
}

/// `::element tag=div class=... role=... aria-*=... data-mdpp-*=...`
pub(super) fn render_element(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：::element 要包裹点东西");
        return;
    }

    let tag = match tag_of(block, ELEMENT_TAGS) {
        Ok(tag) => tag,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    let mut attrs: Vec<(&str, String)> = Vec::new();
    let mut seen = HashSet::new();
    for (key, value) in &block.params {
        match key.as_str() {
            "tag" => {}
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
                        "`::element` 不认参数 `{key}`：只允许 tag / class / role / aria-* / data-mdpp-*"
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
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close(tag);
    fmt.cr();
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

/// `on` / `off` 一类的开关；`None` 用默认值。
fn on_off(value: Option<&str>, default: bool) -> Result<bool, ()> {
    match value.map(str::trim).map(str::to_lowercase).as_deref() {
        None => Ok(default),
        Some("") | Some("on") | Some("true") | Some("yes") | Some("1") => Ok(true),
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

    /// `::surface` 的底色、字色自动对比、尺寸与 class 一起落出来
    #[test]
    fn surface_carries_validated_style() {
        let html = render(
            "::surface color=#0055a4 text=auto height=120px rounded=off class=card\n  正文\n",
        );
        assert!(
            html.contains("class=\"surface surface--sized surface--square card\""),
            "{html}"
        );
        assert!(html.contains("background: #0055a4;"), "{html}");
        assert!(html.contains("color: #f5f5f5;"), "{html}");
        assert!(html.contains("min-height: 120px;"), "{html}");
        // `text=` 是字色，不是正文替换：正文还得在
        assert!(html.contains("<p>正文</p>"), "{html}");
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
