//! 诊断框原语：`::problem`。
//!
//! 它只回答一件事：**这里有一处用法错误**。因此它不属于展示模板，而是像
//! `::text` 一样的语义原语 —— 写出框的同时，把这次错误记进
//! [`Rendered::problems`](crate::Rendered::problems)。
//!
//! 从前它是 `problem.mdpp`：标准库模板调用它，页面看起来有框，但渲染报告里
//! 一个数都不加。于是 `::aside align=maybe` 明明报了错，`--stats` 却说
//! 「误用 0 个」。现在框由 Rust 直接写，计数与写出是同一处，模板层不再需要
//! 一个只长得像问题框的普通模板。
//!
//! 正文不解析、不回显：这里只放 `name=` 与 `why=` 两句话；带内容的错误由
//! 分发那边的 [`crate::block::dispatch::render_problem`] 负责。

use markdown_it::{Node, Renderer};

use crate::block::dispatch;
use crate::registry::Block;

/// 写出一个 `::problem` 框，并记一次「误用」。
///
/// 参数不认识或重复时交给分发那边：那里同样会写出问题框并计数，但会顺带
/// 把解析到的参数与正文回显出来，方便作者改。两条路都是直接写 HTML，不会
/// 再回到 `::problem`，因此没有递归。
pub(super) fn render_problem(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let mut name = None;
    let mut why = None;
    for (key, value) in &block.params {
        match key.as_str() {
            "name" if name.is_none() => name = Some(value.as_str()),
            "why" if why.is_none() => why = Some(value.as_str()),
            "name" | "why" => {
                dispatch::render_problem(block, fmt, "`name=` 与 `why=` 各只能写一次");
                return;
            }
            _ => {
                dispatch::render_problem(block, fmt, "`::problem` 只认 `name=` 与 `why=`");
                return;
            }
        }
    }

    // 头里只显示 name，不把 name=/why= 再当参数回显一遍；这与旧模板的输出一致。
    let shown = Block {
        name: name.unwrap_or_default().to_string(),
        params: Vec::new(),
        body: String::new(),
    };
    crate::env::misused_problem();
    dispatch::problem_box(
        &shown,
        fmt,
        "块的用法有问题",
        "template--problem",
        why.unwrap_or_default(),
        ignore_extra,
    );
}

fn ignore_extra(_: &mut dyn Renderer) {}

#[cfg(test)]
mod tests {
    use crate::engine::Rendered;
    use crate::{Mdpp, RenderContext};

    fn report(source: &str) -> Rendered {
        Mdpp::new().render_report(source, &RenderContext::new())
    }

    /// 有效的问题框既出现在页面上，也出现在渲染报告里
    #[test]
    fn a_problem_box_is_counted() {
        let rendered = report("::problem name=aside why=\"align= 写错了\"\n");
        assert!(
            rendered.html.contains("template--problem"),
            "{}",
            rendered.html
        );
        assert!(rendered.html.contains("aside"), "{}", rendered.html);
        assert!(rendered.html.contains("align= 写错了"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
        assert_eq!(rendered.problems.unknown, 0, "{rendered:?}");
    }

    /// 标准库模板里的诊断也走同一个计数，不再只输出一个长得像的框
    #[test]
    fn a_problem_from_a_standard_library_template_is_counted() {
        let rendered = report("::aside align=maybe\n  正文\n");
        assert!(
            rendered.html.contains("template--problem"),
            "{}",
            rendered.html
        );
        assert!(rendered.html.contains("align= 只认"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    }

    /// `::problem` 只写两句话；正文不解析，也不回显
    #[test]
    fn a_problem_box_does_not_echo_its_body() {
        let rendered = report("::problem name=x why=y\n  这段不该出现\n");
        assert!(!rendered.html.contains("这段不该出现"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    }

    /// 参数写错时交给分发框，仍然只记一次，不会因为再走一次 `::problem` 而递归
    #[test]
    fn malformed_problem_params_are_reported_once() {
        let rendered = report("::problem name=a name=b\n  正文\n");
        assert!(
            rendered.html.contains("template--problem"),
            "{}",
            rendered.html
        );
        assert!(rendered.html.contains("各只能写一次"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    }
}
