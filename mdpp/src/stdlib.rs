//! 随库发布的标准库模板。
//!
//! 这里放的是**库自己的**模板，不是项目里的 `templates/`，也不是文档站用的模板。
//! 文件在编译期由 [`include_dir!`] 整个嵌进二进制，运行时不碰文件系统。
//!
//! 目前从原 Rust 实用块迁出来的是 `small` / `title` / `quote` / `signature` /
//! `flatlist` / `banner` / `aside` / `note` / `tip` / `warning` / `danger` / `error` /
//! `columns` / `fields` / `fold` / `theme` / `tabs` / `indent` / `navbox`；`callout` 是提示框共用的底模板。
//! `problem`、`guard` 等需要参与渲染过程的能力收在 Rust 侧。以后可迁移的基础块也放这里，外围标准库和项目模板都通过同样的
//! [`TemplateSource`] 接口覆盖。
//!
//! 默认的 [`RenderContext`](crate::RenderContext) 已经带上这份来源；调用方用
//! [`RenderContext::with_templates`](crate::RenderContext::with_templates) 叠加自己的
//! 模板时，调用方优先，这里是兜底。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use include_dir::{Dir, include_dir};

use crate::block::template::{TemplatePage, TemplatePages, TemplateSource};

/// 测试断言只关心正文；模板自带的 `::css` 现在会出现在片段开头。
#[cfg(test)]
fn body(html: &str) -> String {
    let mut rest = html;
    let mut out = String::new();
    while let Some(start) = rest.find("<style>") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("</style>") else {
            out.push_str(&rest[start..]);
            return out;
        };
        rest = &rest[start + end + "</style>".len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod navbox_tests;

/// 编译期嵌入的模板目录（`mdpp/stdlib/`）。
static STDLIB: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/stdlib");

#[derive(Debug, Default)]
struct Pages(HashMap<String, TemplatePage>);

impl TemplateSource for Pages {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        self.0.get(name).cloned()
    }
}

/// 标准库模板源。只读一次，之后各处取同一份 [`Arc`]。
pub fn pages() -> TemplatePages {
    static PAGES: OnceLock<TemplatePages> = OnceLock::new();
    Arc::clone(PAGES.get_or_init(|| Arc::new(embedded_pages())))
}

fn embedded_pages() -> Pages {
    let mut files: Vec<_> = STDLIB.files().collect();
    files.sort_by_key(|file| file.path());

    let mut pages = Pages::default();
    for file in files {
        if !file
            .path()
            .extension()
            .is_some_and(|extension| extension == "mdpp")
        {
            continue;
        }
        let Some(stem) = file
            .path()
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        let Some(text) = file.contents_utf8() else {
            continue;
        };
        pages
            .0
            .insert(stem.to_lowercase(), TemplatePage::Ready(text.to_string()));
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准库真的被嵌进来了，而且 `small` 走的是模板而不是 Rust 块
    #[test]
    fn the_standard_library_has_small() {
        let pages = pages();
        let page = pages.page("small").expect("标准库该有 small");
        assert!(matches!(page, TemplatePage::Ready(_)));
        assert!(pages.page("not-there").is_none());
    }

    /// 默认 `RenderContext` 已经带上标准库：`::small` 不用调用方再递模板
    #[test]
    fn the_default_context_uses_the_standard_library() {
        let html = crate::Mdpp::new().render("::small class=note\n  小字 **加粗**\n");
        assert!(html.contains(r#"<div class="mdpp-small note">"#), "{html}");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
    }

    /// 从 Rust 迁出来的基础块都能通过标准库模板工作
    #[test]
    fn migrated_basic_blocks_use_the_standard_library() {
        let mdpp = crate::Mdpp::new();

        let title = mdpp.render("::title\n  卷首的题名\n");
        assert!(title.contains("title-block"), "{title}");
        assert!(title.contains("卷首的题名"), "{title}");

        let quote = mdpp.render("::quote origin=某人\n  引用内容\n");
        assert!(
            quote.contains(r#"<blockquote class="mdpp-quote">"#),
            "{quote}"
        );
        assert!(quote.contains("quote__origin"), "{quote}");
        assert!(quote.contains("— 某人"), "{quote}");

        let signature = mdpp.render("::signature\n  甲  \n  2026 年秋\n");
        assert!(
            signature.contains(r#"<div class="mdpp-signature">"#),
            "{signature}"
        );
        assert!(signature.contains("2026 年秋"), "{signature}");

        let flatlist = mdpp.render("::flatlist\n  * 甲\n  * 乙\n");
        assert!(
            flatlist.contains(r#"<div class="mdpp-flatlist mdpp-hlist">"#),
            "{flatlist}"
        );
        assert_eq!(flatlist.matches("<li>").count(), 2, "{flatlist}");
    }

    /// `title` 的 `text=` 当正文、`color=` 当字色；可选参数缺失不该报错
    #[test]
    fn title_uses_text_and_optional_color() {
        let html = crate::Mdpp::new().render("::title text=\"**重**的标题\" color=#123456\n");
        assert!(html.contains("color: #123456"), "{html}");
        assert!(html.contains("<strong>重</strong>"), "{html}");
        assert!(!html.contains("模板参数"), "{html}");

        let bad = crate::Mdpp::new().render("::title text=x color=红色\n");
        assert!(bad.contains("template--problem"), "{bad}");
    }

    /// `quote` 没写 `origin` 就不输出署名；嵌套时内容落在**内层**引用里
    #[test]
    fn quote_omits_an_absent_origin_and_keeps_nesting() {
        let plain = body(&crate::Mdpp::new().render("::quote\n  只引用\n"));
        assert!(plain.contains("<blockquote"), "{plain}");
        assert!(!plain.contains("quote__origin"), "{plain}");

        let nested = body(&crate::Mdpp::new().render("::quote\n  ::quote origin=内层\n    文字\n"));
        assert_eq!(nested.matches("<blockquote").count(), 2, "{nested}");
        let before = &nested[..nested.find("文字").expect("应当渲染出内容")];
        let depth = before.matches("<blockquote").count() - before.matches("</blockquote>").count();
        assert_eq!(depth, 2, "内容应当嵌在内层 blockquote 里：{nested}");
    }

    /// `aside` 的标题与正文都按 markdown 解析，`align` 用 `::if` 切类名
    #[test]
    fn aside_parses_title_and_body_as_markdown() {
        let html =
            crate::Mdpp::new().render("::aside title=\"十四**门**桥\"\n  正文里的**强调**\n");
        assert!(html.contains(r#"<aside class="mdpp-aside">"#), "{html}");
        assert!(html.contains("aside__title"), "{html}");
        assert!(html.contains("<strong>门</strong>"), "{html}");
        assert!(html.contains("<strong>强调</strong>"), "{html}");

        let center = crate::Mdpp::new().render("::aside align=center\n  居中\n");
        assert!(
            center.contains(r#"<aside class="mdpp-aside mdpp-aside--center">"#),
            "{center}"
        );
        let left = crate::Mdpp::new().render("::aside align=left\n  左侧\n");
        assert!(
            left.contains(r#"<aside class="mdpp-aside mdpp-aside--left">"#),
            "{left}"
        );

        let bad = crate::Mdpp::new().render("::aside align=maybe\n  内容\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(bad.contains("align="), "{bad}");
    }

    /// `banner` 单段落用 `<p>`，多段用 `<div>`；文案可来自正文或 `text=`
    #[test]
    fn banner_uses_tag_auto_and_takes_text_or_body() {
        let body = crate::Mdpp::new().render("::banner\n  福建省文物保护单位\n");
        assert!(body.contains(r#"<p class="mdpp-banner">"#), "{body}");
        assert!(!body.contains("<p><p>"), "{body}");

        let text = crate::Mdpp::new().render("::banner text=\"**加粗**的标题\"\n");
        assert!(text.contains("<strong>加粗</strong>"), "{text}");
        assert!(text.contains(r#"<p class="mdpp-banner">"#), "{text}");

        let multi = crate::Mdpp::new().render("::banner\n  第一段\n\n  第二段\n");
        assert!(multi.contains(r#"<div class="mdpp-banner">"#), "{multi}");
        assert!(multi.contains("<p>第二段</p>"), "{multi}");
    }

    /// `banner` 的高度、圆角、mini 与底色都由 `surface` 落成行内样式
    #[test]
    fn banner_style_values_go_through_surface() {
        let sized = crate::Mdpp::new().render("::banner text=标题 height=120px\n");
        assert!(sized.contains("min-height: 120px"), "{sized}");
        assert!(
            sized.contains("display: flex; align-items: center; justify-content: center;"),
            "{sized}"
        );

        let square = crate::Mdpp::new().render("::banner text=标题 rounded=off\n");
        assert!(square.contains("border-radius: 0;"), "{square}");

        let mini = crate::Mdpp::new().render("::banner mini\n  小横条\n");
        assert!(mini.contains("font-size: 0.9em;"), "{mini}");

        let tinted = crate::Mdpp::new().render("::banner color=#0055a4\n  深蓝底\n");
        assert!(tinted.contains("banner--tinted"), "{tinted}");
        assert!(tinted.contains("background: #0055a4"), "{tinted}");
        assert!(tinted.contains("color: #f5f5f5"), "{tinted}");

        let bad = crate::Mdpp::new().render("::banner text=标题 rounded=也许\n");
        assert!(bad.contains("template--problem"), "{bad}");

        let empty = crate::Mdpp::new().render("::banner\n");
        assert!(empty.contains("template--problem"), "{empty}");
    }

    /// 五种提示框的类名、记号与默认标题都从标准库模板来
    #[test]
    fn callouts_use_the_standard_library_templates() {
        for (name, class, mark, title) in [
            ("note", "note", "ℹ", "说明"),
            ("tip", "tip", "★", "建议"),
            ("warning", "warning", "⚠", "注意"),
            ("danger", "danger", "▲", "危险"),
            ("error", "error", "✕", "错误"),
        ] {
            let html = crate::Mdpp::new().render(&format!("::{name}\n  正文一行\n"));
            assert!(
                html.contains(&format!("mdpp-callout mdpp-callout--{class}")),
                "{name}: {html}"
            );
            assert!(html.contains(mark), "{name} 的记号丢了: {html}");
            assert!(html.contains(title), "{name} 的默认标题丢了: {html}");
            assert!(html.contains("aria-hidden=\"true\""), "{name}: {html}");
        }
    }

    /// 提示框标题保持字面；正文按块解析；空正文仍然报出来
    #[test]
    fn callout_title_is_literal_and_empty_body_is_reported() {
        let html = crate::Mdpp::new().render("::note title=**不是粗体**\n  第一段\n\n  - 甲\n");
        assert!(html.contains("**不是粗体**"), "{html}");
        assert!(!html.contains("<strong>不是粗体</strong>"), "{html}");
        assert!(html.contains("<li>甲</li>"), "{html}");

        let empty = crate::Mdpp::new().render("::danger\n");
        assert!(empty.contains("template--problem"), "{empty}");
        assert!(empty.contains("内容是空的"), "{empty}");
    }

    /// 提示框的自定义底色仍走 `surface` 的校验与字色对比
    #[test]
    fn callout_color_is_validated_by_surface() {
        let html = crate::Mdpp::new().render("::note color=#0055a4\n  正文\n");
        assert!(html.contains("background: #0055a4"), "{html}");
        assert!(html.contains("color: #f5f5f5"), "{html}");

        let bad = crate::Mdpp::new().render("::note color=red\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(!bad.contains("style="), "{bad}");
    }

    /// `fold` 现在是 `disclose` 上面的一层模板：默认标题、open、color 都还在
    #[test]
    fn fold_uses_the_disclose_primitive() {
        let default = crate::Mdpp::new().render("::fold\n  折起来的内容\n");
        assert!(
            default.contains(r#"<details class="mdpp-fold">"#),
            "{default}"
        );
        assert!(
            default.contains(r#"<summary class="mdpp-fold--custom">更多</summary>"#),
            "{default}"
        );
        assert!(default.contains("折起来的内容"), "{default}");

        let open = crate::Mdpp::new().render("::fold open=on title=草稿\n  **加粗**\n");
        assert!(
            open.contains("<details class=\"mdpp-fold\" open=\"\">"),
            "{open}"
        );
        assert!(open.contains("<strong>加粗</strong>"), "{open}");

        let colored = crate::Mdpp::new().render("::fold color=#0055a4 title=深蓝\n  正文\n");
        assert!(
            colored.contains(
                r#"<summary class="mdpp-fold--custom" style="background: #0055a4; color: #f5f5f5;">"#
            ),
            "{colored}"
        );

        let bad = crate::Mdpp::new().render("::fold color=red title=乱写\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(
            !bad.contains("<details"),
            "坏颜色不该留下半个 details：{bad}"
        );

        let empty = crate::Mdpp::new().render("::fold\n");
        assert!(empty.contains("template--problem"), "{empty}");
        assert!(empty.contains("折起来的那一块"), "{empty}");

        // 正文仍是 Markdown 模式：一行 `[注]` 只是普通文字，不是新的一节
        let bracket = crate::Mdpp::new().render("::fold\n  [注] 不是标签\n");
        assert!(bracket.contains("[注] 不是标签"), "{bracket}");
        assert!(!bracket.contains("template--problem"), "{bracket}");
    }

    /// `theme` / `tabs` 现在是 `variants` 上面的一层模板
    #[test]
    fn theme_and_tabs_use_the_variants_primitive() {
        let tabs =
            crate::Mdpp::new().render("::tabs\n  [北岸]\n    走北路\n  [南岸]\n    走南路\n");
        assert!(tabs.contains(r#"class="mdpp-tabs__bar""#), "{tabs}");
        assert!(
            tabs.contains(r#"class="mdpp-tabs__tab mdpp-tabs__tab--on""#),
            "{tabs}"
        );
        assert!(tabs.contains(r#"class="mdpp-tabs__panel""#), "{tabs}");
        assert!(tabs.contains("data-mdpp-option=\"0\""), "{tabs}");
        assert!(tabs.contains("hidden"), "{tabs}");

        let theme = crate::Mdpp::new().render("::theme\n  [light]\n    浅色\n  [dark]\n    深色\n");
        assert!(theme.contains(r#"class="mdpp-theme""#), "{theme}");
        assert!(
            theme.contains("mdpp-theme__part mdpp-theme__part--light"),
            "{theme}"
        );
        assert!(
            theme.contains("mdpp-theme__part mdpp-theme__part--dark"),
            "{theme}"
        );
        assert!(theme.contains("data-mdpp-control=\"theme\""), "{theme}");

        let bad = crate::Mdpp::new().render("::theme\n  [blue]\n    没有这个主题\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(
            !bad.contains("class=\"mdpp-theme\""),
            "坏标签不该留下半个容器：{bad}"
        );
    }

    /// `indent` 是 `surface indent=` 上面的一层模板：比例回到 CSS 的 `.mdpp-indent`
    #[test]
    fn indent_uses_surface_and_the_css_variable() {
        let html = crate::Mdpp::new().render("::indent indent=2\n  这里有**加粗**\n");
        assert!(html.contains(r#"class="mdpp-indent""#), "{html}");
        assert!(html.contains("--mdpp-indent: 2;"), "{html}");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");

        let boxed = crate::Mdpp::new().render("::indent indent=1 class=box\n  内容\n");
        assert!(boxed.contains(r#"class="mdpp-indent box""#), "{boxed}");
        assert!(boxed.contains("--mdpp-indent: 1;"), "{boxed}");

        let missing = crate::Mdpp::new().render("::indent\n  内容\n");
        assert!(missing.contains("template--problem"), "{missing}");
        assert!(missing.contains("indent= 缺失"), "{missing}");

        let bad = crate::Mdpp::new().render("::indent indent=也许\n  内容\n");
        assert!(bad.contains("template--problem"), "{bad}");

        let empty = crate::Mdpp::new().render("::indent indent=1\n");
        assert!(empty.contains("template--problem"), "{empty}");
    }

    /// `fields` 是 `rows max=2` 上面的一层表格模板
    #[test]
    fn fields_use_the_rows_template() {
        let html = crate::Mdpp::new()
            .render("::fields\n  地址 | 福建省某村\n  分类 | 古建筑\n  备注\n  公式 | 甲|乙\n\n");
        assert!(html.contains(r#"<table class="mdpp-fields">"#), "{html}");
        assert!(
            html.contains("地址") && html.contains("福建省某村"),
            "{html}"
        );
        assert!(html.contains("备注"), "{html}");
        assert_eq!(html.matches("<td>").count(), 4, "{html}");
        // max=2：第一道 | 之后的内容都留在值里
        assert!(html.contains("甲|乙"), "{html}");
        assert!(!html.contains("template--problem"), "{html}");

        let classed = crate::Mdpp::new().render("::fields class=attrs\n  甲 | 乙\n");
        assert!(
            classed.contains(r#"class="mdpp-fields attrs""#),
            "{classed}"
        );

        let empty = crate::Mdpp::new().render("::fields\n");
        assert!(empty.contains(r#"<table class="mdpp-fields">"#), "{empty}");
        assert!(!empty.contains("template--problem"), "{empty}");
    }

    /// `columns` 现在是 `each` 上面的一层模板：栏头由 `labels=off` 关掉
    #[test]
    fn columns_use_the_each_template() {
        let html =
            crate::Mdpp::new().render("::columns\n  [写法]\n    这边写\n  [效果]\n    那边看\n");
        assert_eq!(
            html.matches("class=\"mdpp-columns__col\"").count(),
            2,
            "{html}"
        );
        assert!(html.contains("columns__head"), "{html}");
        assert!(html.contains("写法") && html.contains("效果"), "{html}");
        assert!(html.contains("这边写") && html.contains("那边看"), "{html}");

        let rich = crate::Mdpp::new().render("::columns\n  [一]\n    **加粗**\n");
        assert!(rich.contains("<strong>加粗</strong>"), "{rich}");

        let bare = body(&crate::Mdpp::new().render("::columns labels=off\n  [一]\n    内容\n"));
        assert!(!bare.contains("columns__head"), "{bare}");
        assert!(bare.contains("内容"), "{bare}");

        let empty = crate::Mdpp::new().render("::columns\n");
        assert!(empty.contains("template--problem"), "{empty}");
        assert!(empty.contains("分栏是空的"), "{empty}");

        let stray = crate::Mdpp::new().render("::columns\n  没写标签\n  [一]\n    正文\n");
        assert!(stray.contains("template--problem"), "{stray}");
    }

    /// 提示框可以用 `collapsible` 变成 `disclose`；不写仍是原来的静态形状
    #[test]
    fn callouts_can_fold_with_disclose() {
        let plain = crate::Mdpp::new().render("::note\n  普通提示\n");
        assert!(!plain.contains("<details"), "{plain}");
        assert!(
            plain.contains(r#"<p class="mdpp-callout__title">"#),
            "{plain}"
        );

        let open = crate::Mdpp::new().render("::note collapsible\n  可折叠提示\n");
        assert!(
            open.contains(r#"<details class="mdpp-callout__details" open="">"#),
            "{open}"
        );
        assert!(
            open.contains(r#"<summary class="mdpp-callout__title">"#),
            "{open}"
        );
        assert!(open.contains("可折叠提示"), "{open}");

        let closed = crate::Mdpp::new().render("::tip collapsible=closed title=提示\n  内容\n");
        assert!(closed.contains("<details"), "{closed}");
        assert!(!closed.contains(" open=\"\""), "{closed}");
        assert!(closed.contains(">提示</summary>"), "{closed}");

        let off = crate::Mdpp::new().render("::warning collapsible=off\n  还是静态\n");
        assert!(!off.contains("<details"), "{off}");
        assert!(off.contains(r#"<p class="mdpp-callout__title">"#), "{off}");

        let bad = crate::Mdpp::new().render("::danger collapsible=maybe\n  内容\n");
        assert!(bad.contains("template--problem"), "{bad}");

        // 折叠分支也只有一份正文：标题 id 不该因为另一条分支被数两遍
        let heading = crate::Mdpp::new().render("::note collapsible\n  # 标题\n");
        assert_eq!(
            heading.matches(r#"id="标题""#).count(),
            1,
            "折叠提示框里的标题不该被数两遍：{heading}"
        );
    }

    /// 组件样式由模板自己的 `::css` 提供，不依赖 `mdpp.css`
    #[test]
    fn template_component_css_is_collected_with_the_template() {
        let html = crate::Mdpp::new().render("::callout tone=warning\n  内容\n");
        assert!(html.contains(".mdpp-callout--warning"), "{html}");
        assert!(html.contains("mdpp-callout--warning"), "{html}");
        assert!(!body(&html).contains("<style>"), "{html}");
    }

    /// 调用方模板叠在标准库上面，同名时调用方赢
    #[test]
    fn caller_templates_override_the_standard_library() {
        use crate::block::template::TemplateSource;
        use std::collections::HashMap;
        use std::sync::Arc;

        #[derive(Debug, Default)]
        struct Pages(HashMap<String, TemplatePage>);
        impl TemplateSource for Pages {
            fn page(&self, name: &str) -> Option<TemplatePage> {
                self.0.get(name).cloned()
            }
        }

        let mut pages = Pages::default();
        pages
            .0
            .insert("small".into(), TemplatePage::Ready("OVERRIDE".into()));
        let context = crate::RenderContext::new().with_templates(Arc::new(pages));
        let html = crate::Mdpp::new().render_with("::small\n  正文\n", &context);
        assert!(html.contains("OVERRIDE"), "{html}");
    }
}
