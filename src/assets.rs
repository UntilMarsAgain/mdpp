//! 随库附带的东西：目前只有那份默认样式表。
//!
//! 样式**在库里**而不是在调用方那边，理由是 `::css` 注入的 CSS 要用
//! `var(--mdpp-accent)` 取到本库的配色 —— 变量名与这份文件是同一件事，
//! 拆到两个仓库里就会各自漂移。
//!
//! 要换一份样式：复制 [`DEFAULT_CSS`] 那一段文字改，调用方自己内联进页面。

/// 默认样式表（[`assets/mdpp.css`](../../assets/mdpp.css) 的内容）。
///
/// 内联进文档就能用：`--full` 那个命令行开关就是这么干的。
pub const DEFAULT_CSS: &str = include_str!("../assets/mdpp.css");

/// 一个能直接看的完整 HTML 文档：正文 + 内联样式。
///
/// - `title` 写进 `<title>` 与 `<h1>`；
/// - `theme` 给 `Some("light" | "dark")` 时设 `data-mdpp-theme`，
///   `::theme` 那两节就由它挑（见样式表里那段）。
///
/// 只给**片段**是常态（像 `cmark` 那样），要整页时才走这里 ——
/// 正文的样式该由使用方决定，而不是本库替它定。
pub fn full_document(title: &str, body: &str, theme: Option<&str>) -> String {
    // 属性值进 HTML 属性要转义；标题那一处尤其要紧 —— 它常常是文件名
    let title = escape_attribute(title);
    let theme_attr = match theme {
        Some(theme) => format!(" data-mdpp-theme=\"{}\"", escape_attribute(theme)),
        None => String::new(),
    };

    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"zh-Hans\"{theme_attr}>\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n\
         <style>\n{css}\n</style>\n\
         </head>\n\
         <body>\n\
         <article class=\"mdpp-content\">\n\
         {body}\n</article>\n\
         </body>\n\
         </html>\n",
        theme_attr = theme_attr,
        title = title,
        css = DEFAULT_CSS,
        body = body,
    )
}

/// HTML 属性值的转义
fn escape_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stylesheet_is_embedded_and_not_empty() {
        assert!(DEFAULT_CSS.len() > 500, "样式表该是内进来的：{}", DEFAULT_CSS.len());
        assert!(DEFAULT_CSS.contains(".mdpp-content"), "正文那一层得有样式");
    }

    /// 样式表里带变量 —— `::css` 注入的 CSS 靠 `var(--mdpp-…)` 取配色，
    /// 而变量名要与这份文件对得上
    #[test]
    fn the_palette_variables_the_callers_rely_on_are_all_here() {
        for name in [
            "--mdpp-accent",
            "--mdpp-text",
            "--mdpp-text-dim",
            "--mdpp-link",
            "--mdpp-border",
            "--mdpp-surface",
            "--mdpp-danger",
        ] {
            assert!(DEFAULT_CSS.contains(name), "少了 {name}");
        }
    }

    #[test]
    fn a_full_document_carries_the_stylesheet_and_the_body() {
        let html = full_document("某篇笔记", "<p>正文</p>", None);
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains("<style>"), "{html}");
        assert!(html.contains(".mdpp-content"), "{html}");
        assert!(html.contains("<p>正文</p>"), "{html}");
        // 样式表里本来就写着 data-mdpp-theme（那是 `::theme` 的落点），
        // 所以这里只能断言 html 标签上**没有**那个属性
        assert!(html.starts_with("<!DOCTYPE html>\n<html lang=\"zh-Hans\">"), "{html}");
    }

    #[test]
    fn a_theme_lands_on_the_html_element() {
        let html = full_document("t", "x", Some("dark"));
        assert!(html.contains(r#"<html lang="zh-Hans" data-mdpp-theme="dark">"#), "{html}");
    }

    /// 标题常常是文件名，里面带 `&` 与引号很常见 —— 不转义就进不了属性
    #[test]
    fn the_title_is_escaped_into_the_attribute() {
        let html = full_document(r#"a"b&c<d"#, "x", None);
        assert!(html.contains("<title>a&quot;b&amp;c&lt;d</title>"), "{html}");
        // `<` 不该原样出现在标签名附近
        assert!(!html.contains("<d"), "{html}");
    }

    #[test]
    fn a_hostile_theme_value_cannot_break_out_of_the_attribute() {
        let html = full_document("t", "x", Some("dark\" onload=\"alert(1)"));
        assert!(!html.contains("onload=\"alert"), "{html}");
        assert!(html.contains("&quot;"), "{html}");
    }
}