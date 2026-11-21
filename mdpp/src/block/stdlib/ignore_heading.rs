//! 忽略标题原语：`::ignore-heading`。
//!
//! 它把正文原样渲染，但给外层落一个 `data-mdpp-no-outline` 标记；大纲收集器
//! （`runtime::engine::render::outline_of`）看到这个标记就**整棵子树跳过**。
//!
//! 典型用法是把提示框、卡片、示例区里的标题从侧边目录里摘出去：标题仍然按
//! 原来的层级显示，只是不参与目录统计。它不改变标题的锚点、文字或样式，也不
//! 影响 `Rendered::title()` 之外的其他消费者 —— 大纲里没有它，页面标题自然
//! 也不会选中它。
//!
//! ```mdpp
//! ::ignore-heading
//!   ## 这段标题只出现在正文里
//! ```
//!
//! 参数只有可选的 `class=`：外层的类名。空正文报用法问题，因为一个什么都不
//! 包的忽略标记只会让作者以为标签写错了地方。

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use crate::registry::Block;

/// 渲染 `::ignore-heading`。
pub(super) fn render_ignore_heading(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let mut class = None;
    let mut seen = false;
    for (key, value) in &block.params {
        match key.as_str() {
            "class" => {
                if seen {
                    render_problem(block, fmt, "`::ignore-heading` 的参数 `class` 写了两次");
                    return;
                }
                seen = true;
                let value = value.trim();
                if !value.is_empty() {
                    class = Some(value);
                }
            }
            other => {
                render_problem(
                    block,
                    fmt,
                    &format!("`::ignore-heading` 不认参数 `{other}`：只允许 class="),
                );
                return;
            }
        }
    }

    if node.children.is_empty() {
        render_problem(block, fmt, "内容是空的：::ignore-heading 要包住一些标题");
        return;
    }

    let mut attrs: Vec<(&str, String)> = vec![("data-mdpp-no-outline", String::new())];
    if let Some(class) = class {
        attrs.push(("class", class.to_string()));
    }

    fmt.cr();
    fmt.open("div", &attrs);
    fmt.cr();
    fmt.contents(&node.children);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}
