//! 按名字分发：查注册表，查不到就渲染一个报错框。
//!
//! 两种「出不来」对作者来说是不同的事，混成一句最难查：
//!
//! - **没有这个块**（[`render_unknown`]）：名字压根没注册；
//! - **用法有问题**（[`render_problem`]）：名字认识，参数不对。
//!
//! 「模板页在、但这一刻读不出来」不在这里 —— 那走用户块展开（[`crate::block::expand`]），
//! 它不是分发失败。
//!
//! 这些框**没有开关**：正文写错了应该有话说出来，否则一块写坏就静静渲染成一片空白。

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
            // 这一条是最常被搜到的日志：「为什么我写的块没生效」
            debug!(name = %block.name, "没注册这个块，渲染成未知块框");
            render_unknown(block, fmt)
        }
    }
}

/// 名字查不到时的兜底：渲染一个框，把名字与参数原样列出，内容仍用代码块裹住。
///
/// 参数回显成等号写法，于是从输出上就能看出解析成了什么 —— 引号去了哪儿、空格是否
/// 保住。
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

/// 名字认识、但用法不对时的提示。与「未知块」分开，是因为一个是「没这个名字」、
/// 一个是「名字对、参数不对」。
pub fn render_problem(block: &Block, fmt: &mut dyn Renderer, why: &str) {
    fmt.cr();
    fmt.open(
        "div",
        &[("class", "template template--problem".to_string())],
    );
    fmt.cr();
    head(fmt, "块的用法有问题", block);
    fmt.cr();
    why_line(fmt, why);
    fmt.cr();
    body_as_code(block, fmt);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// 徽标 + 块名 + 解析到的参数（回显成等号写法，一眼看出解析成了什么）
///
/// 用户块展开那两个框（[`crate::block::expand`]）也用它 —— 两处各拼一份的话，
/// 改一个 class 名必然只改一半，而那种不一致是静悄悄的
pub(crate) fn head(fmt: &mut dyn Renderer, badge: &str, block: &Block) {
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

/// 那句「为什么」—— 框里的第三行。`render_problem` 与用户块展开那两个框共用
pub(crate) fn why_line(fmt: &mut dyn Renderer, text: &str) {
    fmt.open("p", &[("class", "template__why".to_string())]);
    fmt.text(text);
    fmt.close("p");
}

/// 内容原样给出来（代码块）：写错了也不该让内容静静消失
fn body_as_code(block: &Block, fmt: &mut dyn Renderer) {
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

/// 分节块的预检之一：`[标签]` **之前不许有正文**。
///
/// 三个分节块（`tabs` / `columns` / `navbox`）都得问这一句 —— 这种情形几乎总是**漏写
/// 了方括号**（正文被当成了没有标签的那一节），所以句子里要点名「每一节都要以
/// [标签] 开头」。三个地方各写各的措辞的话，改一处就只改得了一处
///
/// `what` 是块的显示名（「选项卡」「分栏」「导航框」），句子的主语。
/// `Err` 是要摆给作者看的那一句，调用方拿去 [`render_problem`]。
///
/// **「一节都没有」不在这里**：那句话各块说的不是一件事 —— 分栏要的是「每一节」，
/// 导航框要的是「一个分组都没有（通栏撑不起一个框）」，合成一句反而都说不清
pub(crate) fn no_leading_content(sections: &[(&str, &Node)], what: &str) -> Result<(), String> {
    if sections.iter().any(|(label, _)| label.is_empty()) {
        return Err(format!(
            "有内容写在第一个 [标签] 之前：{what}的每一节都要以 [标签] 开头"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::Mdpp;

    /// 没有这个名字：框里说清**是哪个块**、参数被解析成了什么，而且**内容还在**
    ///
    /// 回显成等号写法是刻意的 —— 从输出上就能看出解析对不对（引号去了哪儿、
    /// 空格有没有保住），不必回头去猜
    #[test]
    fn an_unknown_block_echoes_its_name_its_params_and_its_body() {
        let html = Mdpp::new().render("::没有这个块 key=\"1 2\"\n  正文一行\n");
        assert!(html.contains("template--unknown"), "{html}");
        assert!(html.contains("template__name"), "{html}");
        assert!(html.contains("没有这个块"), "{html}");
        assert!(html.contains("key=1 2"), "{html}");
        assert!(html.contains("正文一行"), "{html}");
    }

    /// 两种「出不来」是**两个框**：一个说没这个名字，一个说名字对、写法不对。
    ///
    /// 混成一句最难查 —— 作者会先去查那个名字（可名字本来没错）
    #[test]
    fn the_two_failure_boxes_are_not_the_same_box() {
        let unknown = Mdpp::new().render("::没有这个块\n  甲\n");
        assert!(unknown.contains("template--unknown"), "{unknown}");
        assert!(!unknown.contains("template--problem"), "{unknown}");

        // 名字是注册过的，错在用法：`[link]` 后面没写页面名
        let problem = Mdpp::new().render("::navbox title=x\n  [link]\n  甲\n");
        assert!(problem.contains("template--problem"), "{problem}");
        assert!(!problem.contains("template--unknown"), "{problem}");
        assert!(problem.contains("template__why"), "{problem}");
    }
}
