use std::collections::HashSet;

use markdown_it::parser::core::CoreRule;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::lheading::SetextHeader;
use markdown_it::plugins::cmark::inline::autolink::Autolink;
use markdown_it::plugins::cmark::inline::link::Link;
use markdown_it::plugins::extra::linkify::Linkified;
use markdown_it::{MarkdownIt, Node, NodeValue};

use crate::slug;

/// 给标题挂 `id`，并保证**同一份文档内唯一**。
///
/// 规则每跑一次 `md.parse` 走整棵树。子解析（块正文、分栏的每一节、模板展开）
/// 先给自己的子树挂过一遍，外层这一遍会**重挂**：不是再 push 一个，而是把
/// 已有的 `id` 改成按整棵树重新算出来的那个。于是嵌套不会留下重复属性，
/// 同页两个同名标题也会得到 `安装` / `安装-1`。
pub(super) struct UniqueHeadingAnchors;

impl CoreRule for UniqueHeadingAnchors {
    fn run(root: &mut Node, _md: &MarkdownIt) {
        let mut seen: HashSet<String> = HashSet::new();
        root.walk_mut(|node, _| {
            if !(node.is::<ATXHeading>() || node.is::<SetextHeader>()) {
                return;
            }
            let base = slug::heading_id(&node.collect_text());
            let base = if base.is_empty() {
                // `## !!!` 也该有个能跳过去的锚点
                "section".to_string()
            } else {
                base
            };
            let mut id = base.clone();
            let mut count = 1usize;
            while !seen.insert(id.clone()) {
                id = format!("{base}-{count}");
                count += 1;
            }
            match node.attrs.iter_mut().find(|(name, _)| *name == "id") {
                Some((_, value)) => *value = id,
                None => node.attrs.push(("id", id)),
            }
        });
    }
}

/// 给**站外链接**挂 `class="mdpp-external"`，并把链接文字包进
/// `.external__text`：链接本身不画下划线，只给文字画；末尾那个图标由 CSS
/// 的 `::after` 补，于是复制链接不会把图标带走，图标也不会跟着下划线跑。
///
/// 认的是 CommonMark 的普通链接（`[文字](地址)`、`<地址>` 自动链接，以及
/// linkify 认出来的裸地址），地址以 `http://`、`https://`、`//` 开头就是站外。
///
/// 不碰 [`crate::syntax::wikilink`] 造的节点：内部链接的目标由解析器决定，
/// 它是站内导航，哪怕解析出来是个绝对地址也不该画「出站」符号。
///
/// 每层 `md.parse` 的 core 阶段都会走整棵树（嵌套块的子解析已挂过一次），
/// 所以加之前先看类里有没有 —— 否则嵌套一深就是 `class="mdpp-external external"`。
pub(super) struct MarkExternalLinks;

/// 外链文字的那层 span：`text-decoration` 只画在它上面，不画到 `::after` 图标上
#[derive(Debug, Clone)]
struct ExternalText;

impl NodeValue for ExternalText {
    fn render(&self, node: &Node, fmt: &mut dyn markdown_it::Renderer) {
        fmt.open("span", &[("class", "mdpp-external__text".to_string())]);
        fmt.contents(&node.children);
        fmt.close("span");
    }
}

impl CoreRule for MarkExternalLinks {
    fn run(root: &mut Node, _md: &MarkdownIt) {
        root.walk_mut(|node, _| {
            // 三种链接节点各存各的地址：普通链接、尖括号自动链接、linkify 认出的裸地址。
            // 先克隆出来，后面要改 `attrs`，不能一边借着值一边改节点。
            let url = node
                .cast::<Link>()
                .map(|link| link.url.clone())
                .or_else(|| node.cast::<Autolink>().map(|link| link.url.clone()))
                .or_else(|| node.cast::<Linkified>().map(|link| link.url.clone()));
            let Some(url) = url else {
                return;
            };
            if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//"))
            {
                return;
            }

            if let Some((_, class)) = node.attrs.iter_mut().find(|(name, _)| *name == "class") {
                if !class.split_whitespace().any(|item| item == "mdpp-external") {
                    class.push_str(" external");
                }
            } else {
                node.attrs.push(("class", "mdpp-external".to_string()));
            }

            // 嵌套解析会把同一棵树再过一遍：已经包过就别再套一层
            if node.children.iter().any(|child| child.is::<ExternalText>()) {
                return;
            }
            let children = std::mem::take(&mut node.children);
            if !children.is_empty() {
                let mut wrapper = Node::new(ExternalText);
                wrapper.children = children;
                node.children.push(wrapper);
            }
        });
    }
}
