//! `::guard`：把一段内容包成一次“要么完整成功，要么只留一处诊断”的渲染。
//!
//! 它不是条件块，也不检查某条具体规则；它只做一件事：
//!
//! 1. 把子节点渲染进临时缓冲；
//! 2. 如果这段内容写出了任何问题框，丢弃缓冲和副作用，只写一个诊断；
//! 3. 否则把缓冲原样写回。
//!
//! 这样模板可以把“校验 + 输出”放在同一个 guard 里，失败时不会留下半个表格。
//! 问题计数与资源也要一起回滚，否则页面上只剩一个框，`--stats` 却还记着里面
//! 失败过的每一次，或者坏分支里登记的 CSS 仍然被带出去。
//!
//! guard 不支持参数：它没有自己的条件。需要条件时，把条件写成 guard 里的块，
//! 让那个块自己报告问题。

use markdown_it::{Node, Renderer};

use crate::block::dispatch;
use crate::registry::Block;
use crate::runtime::capture::Capture;

/// 渲染 `::guard`。
pub(super) fn render_guard(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if !block.params.is_empty() {
        dispatch::render_problem(block, fmt, "`::guard` 不接受参数");
        return;
    }

    let Some(snapshot) = crate::env::snapshot() else {
        // 不在渲染环境里时没有可回滚的状态；保持透明容器语义。
        fmt.contents(&node.children);
        return;
    };

    // 环境里可能已经有外层的问题说明；先摘下它，才能拿到 guard 自己这段
    // 内容里的第一份诊断。
    let outer_detail = crate::env::problem_detail();
    crate::env::clear_problem_detail();

    let mut capture = Capture::default();
    capture.contents(&node.children);
    let html = capture.into_string();

    if crate::env::problems_since(&snapshot) == 0 {
        crate::env::set_problem_detail(outer_detail);
        fmt.text_raw(&html);
        return;
    }

    let detail = crate::env::problem_detail();
    crate::env::restore(snapshot);

    let (badge, class, name, why) = match detail {
        Some(detail) => (
            detail.badge,
            detail.class,
            detail.name,
            if detail.why.is_empty() {
                "guard 里的内容写出了问题框".to_string()
            } else {
                detail.why
            },
        ),
        None => (
            "被 guard 包裹的内容有问题".to_string(),
            "mdpp-template--problem".to_string(),
            "guard".to_string(),
            "guard 里的内容写出了问题框".to_string(),
        ),
    };

    // 只保留问题框，不保留触发它的半成品内容；统计也只记一个。
    if class.ends_with("--unknown") {
        crate::env::unknown_problem();
    } else {
        crate::env::misused_problem();
    }
    let shown = Block {
        name,
        params: Vec::new(),
        body: String::new(),
    };
    dispatch::problem_box(&shown, fmt, &badge, &class, &why, |_| {});
}

#[cfg(test)]
mod tests {
    use crate::engine::Rendered;
    use crate::{Mdpp, RenderContext};

    fn report(source: &str) -> Rendered {
        Mdpp::new().render_report(source, &RenderContext::new())
    }

    #[test]
    fn a_clean_guard_keeps_its_body() {
        let rendered = report("::guard\n  正文一行\n");
        assert!(rendered.html.contains("正文一行"), "{}", rendered.html);
        assert_eq!(rendered.problems, Default::default(), "{rendered:?}");
    }

    #[test]
    fn a_problem_inside_a_guard_discards_its_siblings() {
        let rendered = report("::guard\n  前文\n  ::problem name=示例 why=坏掉了\n  后文\n");
        assert!(!rendered.html.contains("前文"), "{}", rendered.html);
        assert!(!rendered.html.contains("后文"), "{}", rendered.html);
        assert!(rendered.html.contains("坏掉了"), "{}", rendered.html);
        assert!(rendered.html.contains("示例"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    }

    #[test]
    fn nested_guards_collapse_to_one_problem() {
        let rendered = report(
            "::guard\n  外层前\n  ::guard\n    内层前\n    ::problem name=内层 why=坏\n    内层后\n  外层后\n",
        );
        assert!(!rendered.html.contains("外层"), "{}", rendered.html);
        assert!(!rendered.html.contains("内层前"), "{}", rendered.html);
        assert!(
            rendered.html.contains("mdpp-template--problem"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    }

    #[test]
    fn a_failed_guard_rolls_back_assets() {
        let rendered = report("::guard\n  ::css\n    .x { color: red; }\n  ::problem why=坏掉了\n");
        assert!(!rendered.html.contains("<style"), "{}", rendered.html);
        assert!(rendered.styles.is_empty(), "{rendered:?}");
        assert!(rendered.assets.is_empty(), "{rendered:?}");
    }

    #[test]
    fn a_failed_guard_rolls_back_scripts() {
        let rendered = report("::guard\n  ::js\n    console.log('bad')\n  ::problem why=坏掉了\n");
        assert!(!rendered.html.contains("<script"), "{}", rendered.html);
        assert!(rendered.scripts.is_empty(), "{rendered:?}");
    }

    #[test]
    fn guard_does_not_accept_params() {
        let rendered = report("::guard why=坏\n  正文\n");
        assert!(rendered.html.contains("不接受参数"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    }

    #[test]
    fn a_guard_keeps_its_own_diagnostic_after_an_earlier_problem() {
        let rendered = report(
            "::problem name=外层 why=外层的问题\n::guard\n  ::problem name=内层 why=内层的问题\n",
        );
        assert!(
            rendered.html.contains("内层"),
            "guard 不该复用外层的问题说明：{}",
            rendered.html
        );
        assert_eq!(rendered.problems.misused, 2, "{rendered:?}");
    }
}
