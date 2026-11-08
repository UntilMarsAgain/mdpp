//! 随库发布的标准库模板。
//!
//! 这里放的是**库自己的**模板，不是项目里的 `templates/`，也不是文档站用的模板。
//! 文件在编译期由 [`include_dir!`] 整个嵌进二进制，运行时不碰文件系统。
//!
//! 目前从原 Rust 实用块迁出来的是 `small` / `title` / `quote` / `signature` /
//! `flatlist`。以后可迁移的基础块也放这里，外围标准库和项目模板都通过同样的
//! [`TemplateSource`] 接口覆盖。
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
