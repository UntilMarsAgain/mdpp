//! 按名字分发：查注册表，查不到就渲染一个报错框。
//!
//! ## 两种「出不来」，两句不同的话
//!
//! 分发要处理的情况不止一种，而它们**对作者来说是不同的事**，混成一句最难查：
//!
//! - **没有这个块**（[`render_unknown`]）：名字压根没注册。
//! - **用法有问题**（[`render_problem`]）：名字认识，参数不对。
//!
//! 第三种「模板页在、但这一刻读不出来」不在这里 —— 那走的是用户块展开那条路
//! （[`crate::block::expand`]），它不是分发失败，是**展开失败**。
//!
//! ## 为什么这些框不是可关闭的选项
//!
//! 它们是**给人看的问题报告**：正文写错了应该有话说出来。关掉的话，一段写坏的块
//! 会静静渲染成一片空白，而作者看不出是自己的问题。

use markdown_it::{Node, NodeValue, Renderer};

use crate::block::Section;
use crate::registry::{self as reg, Block};
use crate::telemetry::debug;

/// 一个 `::` 块节点。
///
/// 里面只有头 —— 内容要么解析成子节点挂在 [`Node::children`] 上，要么就只是
/// `body` 那段原文，由 [`ContentMode`](crate::registry::ContentMode) 决定。
#[derive(Debug, Clone)]
pub struct BlockNode(pub Block);

impl NodeValue for BlockNode {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        render(&self.0, node, fmt);
    }
}

/// 分发：注册表 → 未知块框。
pub fn render(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    match reg::lookup(&block.name) {
        Some(spec) => spec.render(block, node, fmt),
        None => {
            // 这一条是最常被搜到的日志：「为什么我写的块没生效」—— 答案是它没注册
            debug!(name = %block.name, "没注册这个块，渲染成未知块框");
            render_unknown(block, fmt)
        }
    }
}

/// 名字查不到时的兜底：渲染一个框，把名字与参数原样列出，内容仍用代码块裹住。
///
/// 这是**给作者看的错误提示**，不是「不认识就静静丢掉」。参数回显成等号写法，
/// 所以从输出上就能看出解析成了什么 —— 引号去了哪儿、空格是否保住。
pub fn render_unknown(block: &Block, fmt: &mut dyn Renderer) {
    fmt.cr();
    fmt.open(
        "div",
        &[("class", "template template--unknown".to_string())],
    );
    fmt.cr();
    head(fmt, "未知块", block);
    fmt.cr();
    body_as_code(block, fmt);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// 名字认识、但用法不对时的提示。
///
/// 与「未知块」分开：一个是「没这个名字」，一个是「名字对、参数不对」——
/// 提示词要跟着这个区别走，否则作者会往错的方向查。
pub fn render_problem(block: &Block, fmt: &mut dyn Renderer, why: &str) {
    fmt.cr();
    fmt.open(
        "div",
        &[("class", "template template--problem".to_string())],
    );
    fmt.cr();
    head(fmt, "块的用法有问题", block);
    fmt.cr();
    fmt.open("p", &[("class", "template__why".to_string())]);
    fmt.text(why);
    fmt.close("p");
    fmt.cr();
    body_as_code(block, fmt);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// 徽标 + 块名 + 解析到的参数（回显成等号写法，一眼看出解析成了什么）
pub fn head(fmt: &mut dyn Renderer, badge: &str, block: &Block) {
    fmt.open("p", &[("class", "template__head".to_string())]);
    fmt.open("span", &[("class", "template__badge".to_string())]);
    fmt.text(badge);
    fmt.close("span");
    fmt.open("code", &[("class", "template__name".to_string())]);
    fmt.text(&block.name);
    fmt.close("code");
    for (key, value) in &block.params {
        fmt.open("code", &[("class", "template__param".to_string())]);
        let shown = if value.is_empty() {
            key.clone()
        } else {
            format!("{key}={value}")
        };
        fmt.text(&shown);
        fmt.close("code");
    }
    fmt.close("p");
}

/// 内容原样给出来（代码块）：写错了也不该让内容静静消失
pub fn body_as_code(block: &Block, fmt: &mut dyn Renderer) {
    fmt.open("pre", &[]);
    fmt.open("code", &[]);
    fmt.text(&block.body);
    fmt.close("code");
    fmt.close("pre");
}

/// 取一个分节模板的几节：`(标签原样, 那一节的节点)`。
///
/// 标签**不做大小写转换**：它是显示用的字（`[API]` 该照原样出现在选项卡上），
/// 要按大小写不敏感地认标签的地方自己转。
pub fn sections(node: &Node) -> Vec<(&str, &Node)> {
    node.children
        .iter()
        .filter_map(|child| {
            child
                .cast::<Section>()
                .map(|section| (section.label.as_str(), child))
        })
        .collect()
}
