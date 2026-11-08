//! 随库发布的标准库模板。
//!
//! 这里放的是**库自己的**模板，不是项目里的 `templates/`，也不是文档站用的模板。
//! 文件在编译期由 [`include_dir!`] 整个嵌进二进制，运行时不碰文件系统。
//!
//! 目前从原 Rust 实用块迁出来的是 `small` / `title` / `quote` / `signature` /
//! `flatlist` / `aside` / `note` / `tip` / `warning` / `danger` / `error`；
//! `problem` 是模板内部共用的诊断框。以后可迁移的基础块也放这里，外围标准库和
//! 项目模板都通过同样的 [`TemplateSource`] 接口覆盖。
//!
//! 默认的 [`RenderContext`](crate::RenderContext) 已经带上这份来源；调用方用
//! [`RenderContext::with_templates`](crate::RenderContext::with_templates) 叠加自己的
//! 模板时，调用方优先，这里是兜底。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use include_dir::{Dir, include_dir};

use crate::block::template::{TemplatePage, TemplatePages, TemplateSource};

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
        assert!(html.contains(r#"<div class="small note">"#), "{html}");
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
        assert!(quote.contains(r#"<blockquote class="quote">"#), "{quote}");
        assert!(quote.contains("quote__origin"), "{quote}");
        assert!(quote.contains("— 某人"), "{quote}");

        let signature = mdpp.render("::signature\n  甲  \n  2026 年秋\n");
        assert!(
            signature.contains(r#"<div class="signature">"#),
            "{signature}"
        );
        assert!(signature.contains("2026 年秋"), "{signature}");

        let flatlist = mdpp.render("::flatlist\n  * 甲\n  * 乙\n");
        assert!(
            flatlist.contains(r#"<div class="flatlist hlist">"#),
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
        let plain = crate::Mdpp::new().render("::quote\n  只引用\n");
        assert!(plain.contains("<blockquote"), "{plain}");
        assert!(!plain.contains("quote__origin"), "{plain}");

        let nested = crate::Mdpp::new().render("::quote\n  ::quote origin=内层\n    文字\n");
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
        assert!(html.contains(r#"<aside class="aside">"#), "{html}");
        assert!(html.contains("aside__title"), "{html}");
        assert!(html.contains("<strong>门</strong>"), "{html}");
        assert!(html.contains("<strong>强调</strong>"), "{html}");

        let center = crate::Mdpp::new().render("::aside align=center\n  居中\n");
        assert!(
            center.contains(r#"<aside class="aside aside--center">"#),
            "{center}"
        );
        let left = crate::Mdpp::new().render("::aside align=left\n  左侧\n");
        assert!(
            left.contains(r#"<aside class="aside aside--left">"#),
            "{left}"
        );

        let bad = crate::Mdpp::new().render("::aside align=maybe\n  内容\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(bad.contains("align="), "{bad}");
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
                html.contains(&format!("callout callout--{class}")),
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
