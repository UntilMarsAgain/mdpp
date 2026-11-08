//! 分栏：`::columns` —— 按 `[标签]` 分节，一节就是一栏。
//!
//! 与变体组的根本差别是**几栏同时可见**：分栏是拿来并排比对的（写法与效果、两种
//! 变体、参数与默认值），没有选中状态，也不生成按钮。每节各自是一份小文档，栏里
//! 照样能写段落、表格、别的块。
//!
//! `[标签]` 默认当**栏头**（一行小字压在栏顶）；纯摆版面不想见字就写 `labels=off`。
//! 摆不下时样式会自动堆成一竖列 —— 那是 CSS 的事，渲染器不分宽窄屏。
//! 与 `::tabs` 同一个规矩：`[标签]` 要与本节的其他行**左对齐**。

use super::super::dispatch::{no_leading_content, render_problem, sections};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::columns` —— 水平分栏。
pub(super) fn render_columns(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let sections = sections(node);
    if sections.is_empty() {
        render_problem(block, fmt, "分栏是空的：每一节要以 [标签] 开头");
        return;
    }
    if let Err(why) = no_leading_content(&sections, "分栏") {
        render_problem(block, fmt, &why);
        return;
    }

    // 栏头默认露出来；要纯版面就 labels=off（与 banner 的 rounded=off 同一套取值）
    let labels = block
        .param("labels")
        .map(|value| value.trim().to_lowercase());
    let headless = match labels.as_deref() {
        None | Some("" | "on" | "true" | "yes") => false,
        Some("off" | "false" | "no" | "0") => true,
        Some(_) => {
            render_problem(block, fmt, "labels= 只认 off（不写栏头）；不写就是写");
            return;
        }
    };

    fmt.cr();
    fmt.open(
        "div",
        &[(
            "class",
            if headless {
                "columns columns--nolabels".to_string()
            } else {
                "columns".to_string()
            },
        )],
    );
    for (label, content) in &sections {
        fmt.cr();
        fmt.open("div", &[("class", "columns__col".to_string())]);
        if !headless {
            fmt.cr();
            // 栏目名是**这一栏的标签**，不是正文里的一个标题 —— 与 `::tabs` 那边用
            // `<button>` 同一个道理：这里是分栏，不是文档的章节，插一个 `hN` 进去
            // 会把整页的标题层级打乱（跳级、或与正文标题抢位置）。「标题的样子」
            // 由样式在 `.columns__head` 上给
            fmt.open("div", &[("class", "columns__head".to_string())]);
            fmt.text(label);
            fmt.close("div");
        }
        fmt.cr();
        fmt.contents(&content.children);
        fmt.cr();
        fmt.close("div");
    }
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

    /// 每节一栏、同时可见，默认把 [标签] 当栏头
    #[test]
    fn columns_render_one_column_per_section_with_a_head() {
        let html = render("::columns\n  [写法]\n  这边写\n  [效果]\n  那边看\n");
        assert_eq!(html.matches("class=\"columns__col\"").count(), 2, "{html}");
        assert!(html.contains("columns__head"), "{html}");
        assert!(html.contains(">写法<") && html.contains(">效果<"), "{html}");
        // 分栏不是选项卡：不藏任何一节，也没有可点的标签
        assert!(!html.contains("tabs__tab"), "{html}");
        assert!(!html.contains("hidden"), "{html}");

        // 一栏里照样是完整的一份小文档
        let rich = render("::columns\n  [一]\n  **加粗**\n");
        assert!(rich.contains("<strong>加粗</strong>"), "{rich}");

        // labels=off：纯摆版面，栏头不露
        let bare = render("::columns labels=off\n  [一]\n  内容\n");
        assert!(bare.contains("columns--nolabels"), "{bare}");
        assert!(!bare.contains("columns__head"), "{bare}");

        let stray = render("::columns\n  没写标签\n  [一]\n  正文\n");
        assert!(stray.contains("template--problem"), "{stray}");
        let empty = render("::columns\n");
        assert!(empty.contains("template--problem"), "{empty}");
        let bad = render("::columns labels=大概\n  [一]\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
    }
}
