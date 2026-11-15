//! 站点模式的侧边目录。
//!
//! 标题不在这里扫：库的 [`Mdpp`](mdpp::Mdpp) 在渲染时按最终 HTML 收好了
//! `Rendered.outline`，锚点去重、嵌套展开、`::each` / `::rows` 里的标题都已经
//! 在里面。这里只做两件事：筛出 `h2` / `h3`，拼成两份结构。
//!
//! 不挑 `h1`：站点页面正文的第一行通常就是 `# 页面标题`，它已经进 `<title>`、
//! 也摆在文章最上面，再收进目录只是把同一句话说两遍。只写了 `h1` 的页面因此
//! 没有侧边目录 —— 那本来就是「这一页叫什么」，不是「这一页有几节」。
//!
//! 链接全是 `#锚点`：页面内跳转，不经过站点根，于是整站挂在哪个子路径下、
//! 配没配 `site_url` 都不影响它。
//!
//! ## 为什么是两份结构
//!
//! 宽屏是一列常驻的 `<aside>`，窄屏收成 `<details>`/`<summary>`。两者由 CSS 在
//! 同一个断点切换，**不依赖 JS** —— `details` 本来就点得开，禁掉脚本也能用。
//! 同一份链接因此出现过两次，但同一时刻只有一份 `display` 不是 `none`：读屏
//! 软件与 Tab 键都只会碰到当前那一份。

use mdpp::Heading;

use crate::render::page::escape;

/// 正文的大纲 → 侧边目录的 HTML。没有可链接的标题就返回空串 —— 空目录不该占地方。
pub(super) fn sidebar(outline: &[Heading]) -> String {
    let entries: Vec<&Heading> = outline
        .iter()
        .filter(|heading| matches!(heading.level, 2 | 3))
        .filter(|heading| {
            heading
                .id
                .as_deref()
                .is_some_and(|id| !id.trim().is_empty())
        })
        .filter(|heading| !heading.text.trim().is_empty())
        .collect();
    if entries.is_empty() {
        return String::new();
    }

    let list = list(&entries);
    // 宽屏那一列与窄屏那个折叠入口各是一份结构。两份都在 CSS 里按同一个断点
    // 切换显示，于是没有 JS 也能在两种宽度下都成立。
    format!(
        "<aside class=\"mdpp-toc mdpp-toc--wide\">\n\
         <nav class=\"mdpp-toc__nav\" aria-label=\"本页目录\">\n\
         {list}\n\
         </nav>\n\
         </aside>\n\
         <details class=\"mdpp-toc mdpp-toc--narrow\">\n\
         <summary class=\"mdpp-toc__summary\">本页目录</summary>\n\
         <nav class=\"mdpp-toc__nav\" aria-label=\"本页目录\">\n\
         {list}\n\
         </nav>\n\
         </details>"
    )
}

/// 一份清单。宽屏与窄屏两份都用它，内容因此不会长歪。
fn list(entries: &[&Heading]) -> String {
    let mut items = String::new();
    for heading in entries {
        let id = heading.id.as_deref().unwrap_or_default();
        let label = heading
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        items.push_str(&format!(
            "<li class=\"mdpp-toc__item mdpp-toc__item--h{}\"><a href=\"#{}\">{}</a></li>\n",
            heading.level,
            escape(id),
            escape(&label),
        ));
    }
    format!("<ol class=\"mdpp-toc__list\">\n{items}</ol>")
}

#[cfg(test)]
mod tests {
    use mdpp::Heading;

    use super::sidebar;

    fn heading(level: u8, id: &str, text: &str) -> Heading {
        Heading {
            level,
            text: text.to_string(),
            id: Some(id.to_string()),
        }
    }

    /// 只收 h2/h3，链接用大纲里的锚点；h1 是页标题，不再进目录。
    #[test]
    fn the_sidebar_lists_h2_and_h3_with_their_anchors() {
        let outline = vec![
            heading(1, "页面标题", "页面标题"),
            heading(2, "安装", "安装"),
            heading(3, "细节", "细节 a&b"),
        ];
        let html = sidebar(&outline);
        assert!(html.contains(r##"<a href="#安装">安装</a>"##), "{html}");
        assert!(
            html.contains(r##"<a href="#细节">细节 a&amp;b</a>"##),
            "{html}"
        );
        assert!(
            !html.contains("页面标题"),
            "h1 是页标题，不该再进目录：{html}"
        );
        // 宽屏一列 + 窄屏可展开入口，两份结构都在
        assert!(
            html.contains(r#"<aside class="mdpp-toc mdpp-toc--wide">"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<details class="mdpp-toc mdpp-toc--narrow">"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<summary class="mdpp-toc__summary">本页目录</summary>"#),
            "{html}"
        );
        // 两级各带自己的 class（缩进靠它）
        assert!(html.contains("mdpp-toc__item--h2"), "{html}");
        assert!(html.contains("mdpp-toc__item--h3"), "{html}");
    }

    /// 没有章节标题、没有锚点、文字为空，都不该留下一个空盒子。
    #[test]
    fn a_page_without_usable_headings_gets_no_sidebar() {
        assert!(sidebar(&[]).is_empty());
        assert!(
            sidebar(&[heading(1, "页", "页")]).is_empty(),
            "h1 是页标题，不是章节"
        );
        assert!(
            sidebar(&[Heading {
                level: 2,
                text: "没有锚点".to_string(),
                id: None,
            }])
            .is_empty(),
            "没有 id 就没有能跳过去的目标"
        );
        assert!(
            sidebar(&[Heading {
                level: 2,
                text: "   ".to_string(),
                id: Some("空".to_string()),
            }])
            .is_empty()
        );
    }

    /// 锚点与文字按属性转义，不信任大纲里的原文。
    #[test]
    fn the_sidebar_escapes_the_anchor_and_the_label() {
        let html = sidebar(&[heading(2, "A&B", "标题 \"引号\"")]);
        assert!(html.contains(r##"href="#A&amp;B""##), "{html}");
        assert!(html.contains("标题 &quot;引号&quot;"), "{html}");
    }
}
