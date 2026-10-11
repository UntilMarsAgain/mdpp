//! 整页外壳：**这个二进制**的活，不是库的。
//!
//! 库（`mdpp`）给的是 HTML **片段**，「外面长什么样、点一下会发生什么」一概不问。所以
//! 外壳、以及内联的那两样东西，都在 `mdpp-cli/assets/`：
//!
//! | 文件 | 是什么 |
//! |---|---|
//! | `base.css` | 配色变量 + 通用排版 + 出错时摆的那几个框 |
//! | `callouts.css` `text.css` `media.css` `panels.css` `embeds.css` | 按块分的那几组 |
//! | `print.css` | `@media print`：屏幕上折叠的，打印时都展开 |
//! | `mdpp.js` | **最简**的默认行为：点一下就切选项卡 |
//!
//! 许可与库一样是 MIT，**自取即可** —— 想改就复制一份去改。
//!
//! 两样都**内联**而不是外链，是为了让 `--full` 产出的那一个文件自己就能看：拷走、发
//! 给别人、丢进网盘，都还是那样。想自己控制外壳的人在写自己的外壳，或者用 `--css` /
//! `--js` 附自己那份，那两份接在默认的**后面**。
//!
//! `::css` 注入的 CSS 用 `var(--mdpp-accent)` 取配色，变量名与 `base.css` 是同一件事。

/// 默认样式表：`assets/` 下那几个按次序拼起来的。
///
/// 拼接在**编译期**完成（`concat!` 吃的是 `include_str!` 展开的字面量），运行时只是一次
/// 拷贝 —— 而在仓库里它是七个文件。
///
/// **次序有意义**：base 在前（它定义那组 `--mdpp-*` 变量，后面的都在用），print 在最后
/// （打印规则是覆盖，得压过前面的）。
pub const DEFAULT_CSS: &str = concat!(
    include_str!("../assets/base.css"),
    "\n",
    include_str!("../assets/callouts.css"),
    "\n",
    include_str!("../assets/text.css"),
    "\n",
    include_str!("../assets/media.css"),
    "\n",
    include_str!("../assets/panels.css"),
    "\n",
    include_str!("../assets/navbox.css"),
    "\n",
    include_str!("../assets/embeds.css"),
    "\n",
    include_str!("../assets/print.css"),
);

/// 默认脚本（[`assets/mdpp.js`](../assets/mdpp.js) 的内容）。
///
/// 只做「点一下就切选项卡」这一件事。方向键、动画、记住上次选中的那一栏这些属于各自
/// 的界面，由 `--js` 附自己那份进来。
pub const DEFAULT_JS: &str = include_str!("../assets/mdpp.js");

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
    // 属性值进 HTML 属性要转义；标题尤其要紧，它常常是文件名
    let title = escape_attribute(title);
    let theme_attr = match theme {
        Some(theme) => format!(" data-mdpp-theme=\"{}\"", escape_attribute(theme)),
        None => String::new(),
    };
    // 脚本里出现 `</script` 会提前闭合标签，后面的字会漏成正文。用户附带的那几份更要
    // 处理 —— 它们是任意写来的。
    let js = escape_script_end(DEFAULT_JS);
    let js = std::iter::once(js)
        .chain(extra_js.iter().map(|s| escape_script_end(s)))
        .collect::<Vec<_>>()
        .join("\n");
    // 同理，样式里出现 `</style` 会提前闭合
    let css = escape_style_end(DEFAULT_CSS);
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

/// 去掉可能提前闭合标签的 `</script`，大小写都算。
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

    /// 导航框的开关也要接上 —— 与选项卡同一个分工
    #[test]
    fn the_script_binds_the_navbox_toggle_too() {
        assert!(DEFAULT_JS.contains(".navbox__toggle"), "{}", DEFAULT_JS);
        assert!(DEFAULT_JS.contains(".navbox__body"), "{}", DEFAULT_JS);
        // 按钮上那两个字由渲染器落在 data-label-* 上，脚本里不硬编码中文
        assert!(DEFAULT_JS.contains("data-label-show"), "{}", DEFAULT_JS);
        assert!(DEFAULT_JS.contains("data-label-hide"), "{}", DEFAULT_JS);
    }

    /// **按钮上的字要跟着状态换。** 少了这一步，按钮会一直停在「显示」上，
    /// 而内容已经展开了 —— 页面看着正常，而那个字在说谎。
    #[test]
    fn the_toggle_label_is_swapped_by_the_state_change() {
        let fold = DEFAULT_JS
            .split("function fold")
            .nth(1)
            .expect("脚本里该有 fold 那一步");
        let fold = &fold[..fold.find("\n  }").expect("fold 有个头")];
        assert!(
            fold.contains("wasCollapsed ? \"data-label-hide\" : \"data-label-show\""),
            "按钮的字没跟着状态换：\n{fold}"
        );
        assert!(fold.contains("textContent = next"), "\n{fold}");
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

    #[test]
    fn the_stylesheet_is_embedded_and_not_empty() {
        assert!(DEFAULT_CSS.len() > 500, "{}", DEFAULT_CSS.len());
        assert!(DEFAULT_CSS.contains(".mdpp-content"), "正文那一层得有样式");
    }

    /// 七个文件按次序拼成一份 —— 次序错了会静悄悄地看起来对：
    /// 变量那层必须在前（后面都在 `var(--mdpp-…)`），打印那层必须在最后（它是覆盖）。
    #[test]
    fn the_pieces_are_concatenated_in_the_right_order() {
        let base = DEFAULT_CSS.find(":root").expect("变量那层");
        let callout = DEFAULT_CSS.find(".callout {").expect("提示框");
        let tabs = DEFAULT_CSS.find(".tabs__bar").expect("选项卡");
        let print = DEFAULT_CSS.find("@media print").expect("打印");
        assert!(base < callout, "变量要在用它的那些之前");
        assert!(callout < tabs, "按注册次序");
        assert!(tabs < print, "打印规则是覆盖，要压过前面的");
    }

    /// 每个文件都得真的被拼进去 —— 漏一个 `include_str!` 是静默的：页面少几样样式，
    /// 而编译不报错。
    #[test]
    fn every_asset_file_reaches_the_stylesheet() {
        for (file, needle) in [
            ("base.css", "--mdpp-accent"),
            ("callouts.css", "callout--warning"),
            ("text.css", "title-block"),
            ("media.css", "figcaption"),
            ("panels.css", "theme__part--dark"),
            ("navbox.css", "navbox__group"),
            ("embeds.css", "math--display"),
            ("print.css", "attr(data-label)"),
        ] {
            assert!(
                DEFAULT_CSS.contains(needle),
                "{file} 没被拼进去（少 {needle}）"
            );
        }
    }

    /// `::css` 注入的 CSS 靠 `var(--mdpp-…)` 取配色，变量名要与 `base.css` 对得上
    #[test]
    fn the_palette_variables_are_all_there() {
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
    fn the_script_is_embedded_and_does_the_minimum() {
        assert!(DEFAULT_JS.contains("tabs__tab--on"), "{}", DEFAULT_JS);
        assert!(DEFAULT_JS.contains("DOMContentLoaded"), "{}", DEFAULT_JS);
        assert!(
            DEFAULT_JS.contains("addEventListener(\"click\""),
            "点击切换不能少"
        );
        // 默认脚本多带一样（方向键、动画、记忆…）就有人得自己去拆，
        // 而拆一段别人的行为比不写要费劲
        assert!(
            !DEFAULT_JS.contains("ArrowRight"),
            "方向键不该由官方工具来定"
        );
        assert!(
            !DEFAULT_JS.contains("localStorage"),
            "记住上次那栏不该由官方工具来定"
        );
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
