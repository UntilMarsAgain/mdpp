//! 平行列表：`::flatlist`。
//!
//! 把列表项摊成一行水平排列，与 qiuwenbaike 的 `Template:Flatlist` 对齐：
//! 根元素是一个带 `hlist` 类的 `<div>`，内容按 markdown 解析，列表项由
//! CSS 水平排列（见 `mdpp-cli/assets/mdpp.css` 的「平行列表」那一节），渲染器只吐结构，
//! 不吐 `hlist` / `horizontal` / `inline` 之类标记，也不塞 `onclick`。
//!
//! ```text
//! ::flatlist
//!   * [[拱桥]]
//!   * [[梁桥]]
//!   * [[悬索桥]]
//! ```

use super::super::dispatch::render_problem;
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::flatlist` —— 把列表项摊成一行水平排列。
pub(super) fn render_flatlist(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    if node.children.is_empty() {
        render_problem(
            block,
            fmt,
            "内容是空的：平行列表要写点列表项，或留空不如不用这块",
        );
        return;
    }

    fmt.cr();
    fmt.open("div", &[("class", "flatlist hlist".to_string())]);
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 根元素带 flatlist hlist 两个类，内容是普通 <ul><li>
    #[test]
    fn flatlist_renders_horizontal_list() {
        let html = render("::flatlist\n  * 甲\n  * 乙\n");
        assert!(html.contains(r#"<div class="flatlist hlist">"#), "{html}");
        assert_eq!(html.matches("<li>").count(), 2, "{html}");
        // 渲染器不额外吐 horizontal / inline 之类布局标记，水平由 CSS 负责
        //（“hlist”本身在 class 里，是 qiuwenbaike 模板约定，不算额外标记）
        for marker in ["horizontal", "inline"] {
            assert!(!html.contains(marker), "渲染器不该吐 {marker}：{html}");
        }
    }

    /// 空内容报错
    #[test]
    fn an_empty_flatlist_says_so() {
        let html = render("::flatlist\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("是空的"), "{html}");
    }
}
