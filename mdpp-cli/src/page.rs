//! 整页外壳：**这个二进制**的活，不是库的。
//!
//! ## 为什么在这儿
//!
//! 库给的是 HTML **片段** —— 那段 HTML 往哪去、外面套什么样式、要不要接一段脚本
//! 让它能点，都是**界面**那一层的事。所以外壳、默认样式表怎么内联、默认脚本怎么
//! 内联，都住在 `mdpp-cli` 这边；库那边只留 [`mdpp::assets::DEFAULT_CSS`]（它样式
//! 表的对象是库自己的输出，而 `::css` 注入的 CSS 要用 `var(--mdpp-…)` 取配色，
//! 变量名与那份文件是同一件事）。
//!
//! ## 两样都内联
//!
//! 内联而不是外链，是为了让 `--full` 产出的那一个文件**自己就能看** —— 拷走、
//! 发给别人、丢进网盘，都还是那样。这正是「完整文档」的意思。
//!
//! 代价是每个页面都带着那 ~13 KB 样式与 ~2 KB 脚本，而页面多半用不上每一行。
//! 为此单开一个「不内联」开关不值当：想要自己控制外壳的人，本来就在写自己的外壳
//! —— 或者用 `--css` / `--js` 附上自己那份，那两份会接在默认的**后面**。

/// 一个能直接看的完整 HTML 文档：正文 + 内联样式 + 内联脚本。
///
/// - `title` 写进 `<title>`；
/// - `theme` 给 `Some("light" | "dark")` 时设 `data-mdpp-theme`，`::theme`
///   那两节就由它挑（见样式表里那段）；
/// - `extra_css` / `extra_js` 是调用方用 `--css` / `--js` 附上的那些，
///   接在默认的**后面** —— 同优先级时后者胜出，所以「改一下」不必重写默认那份。
///
/// 出来的那份**能看也能用**：样式在，选项卡切得动，打印时折叠的也都展开。
pub fn full_document(
    title: &str,
    body: &str,
    theme: Option<&str>,
    extra_css: &[String],
    extra_js: &[String],
) -> String {
    // 属性值进 HTML 属性要转义；标题那一处尤其要紧 —— 它常常是文件名
    let title = escape_attribute(title);
    let theme_attr = match theme {
        Some(theme) => format!(" data-mdpp-theme=\"{}\"", escape_attribute(theme)),
        None => String::new(),
    };
    // 脚本里出现 `</script` 会提前闭合标签，后面的字会漏成正文。
    // 用户附带的那几份更要处理 —— 它们是任���写来的。
    let js = escape_script_end(mdpp::assets::DEFAULT_JS);
    let js = std::iter::once(js)
        .chain(extra_js.iter().map(|s| escape_script_end(s)))
        .collect::<Vec<_>>()
        .join("\n");
    // 同理，样式里出现 `</style` 会提前闭合
    let css = escape_style_end(mdpp::assets::DEFAULT_CSS);
    let css = std::iter::once(css)
        .chain(extra_css.iter().map(|s| escape_style_end(s)))
        .collect::<Vec<_>>()
        .join("\n");

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
         <script>\n{js}\n</script>\n\
         </body>\n\
         </html>\n",
        theme_attr = theme_attr,
        title = title,
        css = css,
        body = body,
        js = js,
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

/// 去掉可能提前闭合标签的 `</script` —— 大小写都算。
///
/// 只删闭合标签本身：这是本仓库自己的静态文件，写进去不会有别的意图，所以
/// 不必学那些「转义成 `<\/script`」的花样（那是给**用户写的**脚本准备的）。
fn escape_script_end(text: &str) -> String {
    escape_close_tag(text, "</script")
}

/// 去掉可能提前闭合标签的 `</style`
fn escape_style_end(text: &str) -> String {
    escape_close_tag(text, "</style")
}

/// 去掉 `needle`（连它后面那个 `>` 一起），大小写不敏感
fn escape_close_tag(text: &str, needle: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut cursor = 0usize;
    while let Some(at) = lower[cursor..].find(needle) {
        let start = cursor + at;
        let end = rest[start..]
            .find('>')
            .map(|at| start + at + 1)
            .unwrap_or(text.len());
        out.push_str(&rest[..start]);
        rest = &rest[end..];
        cursor = end;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_document_carries_the_stylesheet_the_script_and_the_body() {
        let html = full_document("某篇笔记", "<p>正文</p>", None, &[], &[]);
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains(".mdpp-content"), "样式没内联进来");
        assert!(html.contains("tabs__tab--on"), "脚本没内联进来");
        assert!(html.contains("<p>正文</p>"), "{html}");
    }

    /// 「能看」与「能用」是两件事 —— 少了脚本，选项卡点不动，页面仍然「看着对」。
    /// 这条钉的是**脚本在**，钉不住它「有效」；后者靠 `::tabs` 的按钮带
    /// `data-mdpp-block` 之类的约定 —— 而那也只是形状上的线索。
    #[test]
    fn the_script_is_there_so_the_page_is_usable() {
        let html = full_document("t", "<div class=\"tabs\"></div>", None, &[], &[]);
        assert!(html.contains("DOMContentLoaded"), "{html}");
        assert!(html.contains("tabs__tab--on"), "{html}");
    }

    #[test]
    fn a_theme_lands_on_the_html_element() {
        let html = full_document("t", "x", Some("dark"), &[], &[]);
        assert!(
            html.contains(r#"<html lang="zh-Hans" data-mdpp-theme="dark">"#),
            "{html}"
        );
    }

    /// 标题常常是文件名，里面带 `&` 与引号很常见 —— 不转义就进不了属性
    #[test]
    fn the_title_is_escaped_into_the_attribute() {
        let html = full_document(r#"a"b&c<d"#, "x", None, &[], &[]);
        assert!(
            html.contains("<title>a&quot;b&amp;c&lt;d</title>"),
            "{html}"
        );
        assert!(!html.contains("<d"), "{html}");
    }

    #[test]
    fn a_hostile_theme_value_cannot_break_out_of_the_attribute() {
        let html = full_document("t", "x", Some("dark\" onload=\"alert(1)"), &[], &[]);
        assert!(!html.contains("onload=\"alert"), "{html}");
        assert!(html.contains("&quot;"), "{html}");
    }

    /// 脚本里出现 `</script` 会提前收尾，后面的字漏成正文
    #[test]
    fn neither_the_script_nor_the_style_can_close_its_own_tag() {
        assert_eq!(escape_script_end("a</script>b"), "ab");
        assert_eq!(escape_script_end("a</SCRIPT >b"), "ab");
        assert_eq!(escape_style_end("a</style>b"), "ab");
        // 没有就不动
        assert_eq!(escape_script_end("plain"), "plain");
        // 只去掉闭合标签，别的原样
        assert_eq!(escape_script_end("x</script>y"), "xy");
    }

    /// 附带的那些接在默认的**后面** —— 同优先级时后者胜出，
    /// 所以「改一下」不必把默认那份重写一遍
    #[test]
    fn the_extra_files_come_after_the_defaults() {
        let html = full_document(
            "t",
            "x",
            None,
            &["/* 我自己的 */".into()],
            &["// 我自己的".into()],
        );
        let default_css = html.find(".mdpp-content").expect("默认样式");
        let extra_css = html.find("/* 我自己的 */").expect("附带的样式");
        let default_js = html.find("tabs__tab--on").expect("默认脚本");
        let extra_js = html.find("// 我自己的").expect("附带的脚本");
        assert!(default_css < extra_css, "附带的样式该在默认之后：{html}");
        assert!(default_js < extra_js, "附带的脚本该在默认之后：{html}");
    }

    /// 附带文件里出现闭合标签也不能提前收尾 —— 那是**用户写的**，比默认那份危险
    #[test]
    fn the_extra_files_cannot_close_their_own_tags() {
        let html = full_document(
            "t",
            "x",
            None,
            &["a{}</style><p>顶出来了</p>".into()],
            &[r#"var s = "</script>";"#.into()],
        );
        assert_eq!(html.matches("</style>").count(), 1, "{html}");
        assert!(!html.contains(r#"</script>";"#), "{html}");
    }
}
