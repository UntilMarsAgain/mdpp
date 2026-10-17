//! **用户自定义块**的展开：`::卡片` → 找模板页 `卡片` → 填 `{{参数}}` → 解析成节点。
//!
//! 与内置块的界在名字：内置名字按注册表走，非内置名字才来这里找同名页。所以这一步的
//! 产物**不是一个「渲染成某个样子」的块，而是一段内容** —— 页面里怎么写就怎么长。
//!
//! 两种「停下来说清楚」的情形都返回一段说明而不是硬展开：这一页**在但读不出来**
//! （报成「未知块」会把作者引向错的方向），以及套得太深（多半是块自己嵌自己）。

use markdown_it::parser::block::BlockState;
use markdown_it::{Node, NodeValue, Renderer};

use crate::block::dispatch;
use crate::block::fill;
use crate::block::template::{self, TemplatePage, page_key};
use crate::context::{self, MAX_TEMPLATE_DEPTH};
use crate::registry::Block;
use crate::syntax::wikilink;
use crate::telemetry::{debug, trace, warn};

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
        TemplatePage::Ready(text) => {
            trace!(name, bytes = text.len(), "展开用户块");
            text
        }
        TemplatePage::Unreadable { reason } => {
            debug!(name, %reason, "模板页在，但这一刻读不出来");
            return Some(unreadable_node(name, &reason));
        }
    };

    if context::depth() >= MAX_TEMPLATE_DEPTH {
        // warn 而不是 debug：作者多半不知道自己写了个转圈的块
        warn!(
            name,
            limit = MAX_TEMPLATE_DEPTH,
            "块嵌套超过上限，多半是自嵌"
        );
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

/// 用户块展开后的节点：它自己没有 HTML，只把解析出来的子节点写出去。
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
/// 「那一页读不出来」与「套得太深」共用它，差别只在徽标与那句话。两种都**不给**
/// 解锁框：那种场合输了口令问题还在。
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
        // 徽标那一行与「为什么」那一段都交给**分发那边**写：两个框的 class 名与
        // 结构在一处定死，改一处就两处一起改（各拼一份的话，不一致是静悄悄的）
        dispatch::head(fmt, self.badge, &self.block);
        fmt.cr();
        dispatch::why_line(fmt, &self.block.body);
        fmt.cr();
        // 能做的下一步就是去把那一页打开
        fmt.open("p", &[("class", "template__where".to_string())]);
        fmt.contents(&node.children);
        fmt.close("p");
        fmt.cr();
        fmt.close("div");
        fmt.cr();
    }
}

/// 造一个「说清出了什么事」的节点，children 上挂一条指向那一页的链接
fn reported_node(badge: &'static str, class: &'static str, name: &str, why: &str) -> Node {
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

/// 页名的链接：用户在正文里能跟着它去把那一页改掉。
///
/// 由**链接解析器**接住；没递解析器时它就是一条红链（见 [`crate::syntax::wikilink`]）。
fn link_to_page(name: &str) -> Node {
    let key = page_key(name).unwrap_or_else(|| name.to_string());
    let target = template::display_name(&key);
    wikilink::link_node(&target, &target)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::block::template::{TemplatePage, TemplateSource};
    use crate::context::MAX_TEMPLATE_DEPTH;
    use crate::{Mdpp, RenderContext};

    /// 一份手写的模板页来源：键 → 那一页**这一刻**的样子。
    ///
    /// 键已经归一过（小写、剥了前缀），所以直接查 —— 这也是文档里建议的实现
    #[derive(Debug, Default)]
    struct Pages(HashMap<String, TemplatePage>);

    impl TemplateSource for Pages {
        fn page(&self, name: &str) -> Option<TemplatePage> {
            self.0.get(name).cloned()
        }
    }

    /// 正文里只写 `::名`，把那一页的**这一刻的样子**递进去
    fn render(page: &str, source: TemplatePage) -> String {
        let mut pages = Pages::default();
        pages.0.insert(page.to_string(), source);
        let context = RenderContext::new().with_templates(Arc::new(pages));
        Mdpp::new().render_with(&format!("::{page}\n"), &context)
    }

    /// 那一页**在但读不出来**：报的是「读不出来」，**不是**「未知块」——
    /// 名字是对的，说成「没这个名字」会把作者引去查根本没错的地方
    #[test]
    fn a_page_that_cannot_be_read_says_that_rather_than_unknown() {
        let html = render(
            "卡片",
            TemplatePage::Unreadable {
                reason: "读不了：权限不够".into(),
            },
        );
        assert!(!html.contains("template--unknown"), "{html}");
        assert!(html.contains("template--unreadable"), "{html}");
        assert!(html.contains("模板页读不出来"), "{html}");
        assert!(html.contains("读不了：权限不够"), "{html}");
        // 框里顺带把**那一页的名字**指给作者：下一步就是去改它
        assert!(html.contains("template:卡片"), "{html}");
    }

    /// 自己嵌自己：转到上限就停下并说清是哪一层，而不是一直转下去
    #[test]
    fn a_block_that_embeds_itself_stops_at_the_limit() {
        let html = render("转圈", TemplatePage::Ready("::转圈\n  甲\n".into()));
        assert!(html.contains("块嵌套太深"), "{html}");
        assert!(
            html.contains(&format!("超过 {MAX_TEMPLATE_DEPTH} 层")),
            "{html}"
        );
        // 上限那句话是给人看的建议，不是把内容吞了 —— 还给了那一页的名字
        assert!(html.contains("是不是自己嵌自己"), "{html}");
        assert!(html.contains("template:转圈"), "{html}");
    }

    /// 没有这一页 → `None`，调用方（[`crate::block::dispatch`]）照旧报「未知块」。
    ///
    /// 这条钉的是**分工**：展开这条路只处理「有这一页」，不越界替别人报错
    #[test]
    fn no_such_page_falls_through_to_the_unknown_box() {
        let html = Mdpp::new().render("::压根没有这一页\n");
        assert!(html.contains("template--unknown"), "{html}");
        assert!(!html.contains("template--unreadable"), "{html}");
    }
}
