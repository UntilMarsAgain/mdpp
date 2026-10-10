//! **用户自定义块**的展开：`::卡片` → 找模板页 `卡片` → 填 `{{参数}}` → 解析成节点。
//!
//! 与注册表里那些内置块的界在名字：内置名字按注册表走，非内置名字才来这里找同名页。
//! 所以这一步的产物**不是一个「渲染成某个样子」的块，而是一段内容** ——
//! 页面里怎么写就怎么长（见 [`ExpandedBlock`]）。
//!
//! 三个「停下来说清楚」的情形，都返回一段说明而不是硬展开：
//!
//! - 这一页**在但读不出来**（没解锁、坏了）—— 报成「未知块」会把作者引向错的方向；
//! - 套得太深（多半是块自己嵌自己）—— [`MAX_TEMPLATE_DEPTH`] 是转圈的保护。

use markdown_it::parser::block::BlockState;
use markdown_it::{Node, NodeValue, Renderer};

use crate::block::fill;
use crate::block::template::{self, page_key, TemplatePage};
use crate::context::{self, MAX_TEMPLATE_DEPTH};
use crate::registry::Block;
use crate::syntax::wikilink;

/// 展开一个**用户自定义块**：模板页的正文 → 填 `{{参数}}` → 解析成节点。
///
/// 返回 `None` = 「没有这一页」（调用方照旧报「未知块」）；
/// 其余情形返回一段**说明** —— 那多半是块自己嵌自己，或者那一页锁着。
pub(crate) fn expand_user_block(
    name: &str,
    params: Vec<(String, String)>,
    body: String,
    state: &mut BlockState,
) -> Option<Node> {
    // 转圈的保护要在**取到正文之后**查：取不到就没有「套」这回事
    let source = match template::page_for(name)? {
        TemplatePage::Ready(text) => text,
        TemplatePage::Unreadable { reason } => return Some(unreadable_node(name, &reason)),
    };

    if context::depth() >= MAX_TEMPLATE_DEPTH {
        return Some(reported_node(
            "块嵌套太深",
            "template--problem",
            name,
            &format!("块嵌套超过 {MAX_TEMPLATE_DEPTH} 层（是不是自己嵌自己？）"),
        ));
    }

    let block = Block {
        name: name.to_string(),
        params,
        body: body.clone(),
    };
    let filled = fill::substitute(&source, &block, &body);

    let md = state.md;
    let mut parsed = context::deeper(|| md.parse(&filled));
    let mut node = Node::new(ExpandedBlock);
    node.children = std::mem::take(&mut parsed.children);
    Some(node)
}

/// 用户块展开后的节点：它自己没有 HTML，只把解析出来的子节点写出去
///
/// （与 `::html` 那种「渲染成某个样子」的块不同 —— 用户块就是**一段内容**，
/// 页面里怎么写就怎么长。）
#[derive(Debug)]
struct ExpandedBlock;

impl NodeValue for ExpandedBlock {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.contents(&node.children);
        fmt.cr();
    }
}

/// 一个「说清出了什么事」的节点：徽标 + 原因 + 指向那一页的链接。
///
/// 两种场合共用它，差别只在徽标与那句话：
///
/// - **那一页读不出来** —— 东西没问题，只是打不开；
/// - **套得太深** —— 说法本身有问题。
///
/// 两种都**不给**解锁框：那种场合输了口令问题还在，给个输入框是荒唐的。
#[derive(Debug)]
struct Reported {
    block: Block,
    badge: &'static str,
    class: &'static str,
}

impl NodeValue for Reported {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.open("div", &[("class", format!("template {}", self.class))]);
        fmt.cr();
        fmt.open("p", &[("class", "template__head".to_string())]);
        fmt.open("span", &[("class", "template__badge".to_string())]);
        fmt.text(self.badge);
        fmt.close("span");
        fmt.open("code", &[("class", "template__name".to_string())]);
        fmt.text(&self.block.name);
        fmt.close("code");
        fmt.close("p");
        fmt.cr();
        fmt.open("p", &[("class", "template__why".to_string())]);
        fmt.text(&self.block.body);
        fmt.close("p");
        fmt.cr();
        // **给得出下一步**：能做的下一步就是去把那一页打开。
        fmt.open("p", &[("class", "template__where".to_string())]);
        fmt.contents(&node.children);
        fmt.close("p");
        fmt.cr();
        fmt.close("div");
        fmt.cr();
    }
}

/// 造一个「说清出了什么事」的节点，children 上挂一条指向那一页的链接
fn reported_node(
    badge: &'static str,
    class: &'static str,
    name: &str,
    why: &str,
) -> Node {
    let mut node = Node::new(Reported {
        block: Block {
            name: name.to_string(),
            params: Vec::new(),
            body: why.to_string(),
        },
        badge,
        class,
    });
    node.children.push(link_to_page(name));
    node
}

/// 那一页存在，但这一刻读不出来
fn unreadable_node(name: &str, reason: &str) -> Node {
    reported_node("模板页读不出来", "template--unreadable", name, reason)
}

/// 页名的链接。用户在正文里能跟着它去把那一页改掉。
///
/// 链接由**链接解析器**接住；没递解析器时它就是一个不带 `href` 的 `<a>`，
/// 至少把目标留着（见 [`crate::syntax::wikilink`]）。
fn link_to_page(name: &str) -> Node {
    let key = page_key(name).unwrap_or_else(|| name.to_string());
    let target = template::display_name(&key);
    wikilink::link_node(&target, &target)
}