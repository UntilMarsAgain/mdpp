//! 页面 / 模板两种渲染语境的包含条件：`noinclude` / `includeonly`。
//!
//! - `::noinclude`：页面**直接渲染**时输出，作为模板被 `::名字` 引入时隐藏；
//! - `::includeonly`：反过来，只在作为模板引入时输出。
//!
//! 判据是当前有没有模板作用域（[`crate::context::template_vars`]）：模板页被
//! `::名字` 展开并渲染时一定有；页面顶层直接渲染时没有。嵌套模板、模板里再调
//! 模板都在作用域内，因此 `includeonly` 会一路可见。
//!
//! 两个块都是透明的 Markdown 容器，不产生自己的 DOM。

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use crate::context;
use crate::registry::Block;

/// `::noinclude` / `::includeonly` 的共用实现。
fn render_if_included(block: &Block, node: &Node, fmt: &mut dyn Renderer, wanted: bool) {
    if !block.params.is_empty() {
        render_problem(block, fmt, "`::noinclude` / `::includeonly` 不接受参数");
        return;
    }
    let included = context::template_vars().is_some();
    if included != wanted {
        return;
    }
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
}

/// `::noinclude` —— 只在页面直接渲染时输出。
pub(super) fn render_noinclude(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    render_if_included(block, node, fmt, false);
}

/// `::includeonly` —— 只在作为模板引入时输出。
pub(super) fn render_includeonly(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    render_if_included(block, node, fmt, true);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::block::template::{TemplatePage, TemplateSource};
    use crate::{Mdpp, RenderContext};

    #[derive(Debug, Default)]
    struct Pages(HashMap<String, TemplatePage>);

    impl TemplateSource for Pages {
        fn page(&self, name: &str) -> Option<TemplatePage> {
            self.0.get(name).cloned()
        }
    }

    const PAGE: &str = "::noinclude\n  页面独有\n::includeonly\n  模板独有\n::text\n  公共\n";

    fn render_included() -> String {
        let mut pages = Pages::default();
        pages
            .0
            .insert("卡片".into(), TemplatePage::Ready(PAGE.into()));
        let context = RenderContext::new().with_templates(Arc::new(pages));
        Mdpp::new().render_with("::卡片\n", &context)
    }

    /// 页面直接渲染：noinclude 在，includeonly 不在
    #[test]
    fn a_direct_page_keeps_noinclude_and_drops_includeonly() {
        let html = Mdpp::new().render(PAGE);
        assert!(html.contains("页面独有"), "{html}");
        assert!(!html.contains("模板独有"), "{html}");
        assert!(html.contains("公共"), "{html}");
    }

    /// 被模板引入：includeonly 在，noinclude 不在
    #[test]
    fn including_it_keeps_includeonly_and_drops_noinclude() {
        let html = render_included();
        assert!(!html.contains("页面独有"), "{html}");
        assert!(html.contains("模板独有"), "{html}");
        assert!(html.contains("公共"), "{html}");
    }

    /// 参数和未知用法要报出来，不能静默吞掉
    #[test]
    fn parameters_are_refused() {
        let html = Mdpp::new().render("::noinclude why=x\n  正文\n");
        assert!(html.contains("template--problem"), "{html}");
    }
}
