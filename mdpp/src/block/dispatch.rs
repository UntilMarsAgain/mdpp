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
use crate::runtime::env::ProblemDetail;
use crate::telemetry::debug;

/// 「没这个名字」的框的 class。`lib.rs`、嵌套渲染与 `<markdown>` 的兜底也用它。
pub(crate) const UNKNOWN_CLASS: &str = "mdpp-template--unknown";
/// 「名字对、用法不对」的框的 class。
pub(crate) const MISUSED_CLASS: &str = "mdpp-template--problem";

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
            crate::env::unknown_problem();
            render_unknown(block, fmt)
        }
    }
}

/// 名字查不到时的兜底：渲染一个框，把名字与参数原样列出，内容仍用代码块裹住。
///
/// 参数回显成等号写法，于是从输出上就能看出解析成了什么 —— 引号去了哪儿、空格是否
/// 保住。
pub fn render_unknown(block: &Block, fmt: &mut dyn Renderer) {
    let detail = ProblemDetail {
        badge: "未知块".to_string(),
        class: UNKNOWN_CLASS.to_string(),
        name: block.name.clone(),
        why: String::new(),
    };
    write_problem(&detail, block, true, false, fmt);
}

/// 名字认识、但用法不对时的提示。与「未知块」分开，是因为一个是「没这个名字」、
/// 一个是「名字对、参数不对」。
pub fn render_problem(block: &Block, fmt: &mut dyn Renderer, why: &str) {
    crate::env::misused_problem();
    let detail = ProblemDetail {
        badge: "块的用法有问题".to_string(),
        class: MISUSED_CLASS.to_string(),
        name: block.name.clone(),
        why: why.to_string(),
    };
    write_problem(&detail, block, true, false, fmt);
}

/// 行内位置的问题：不能写 `<div>`，只能写一个 `<span>`。
///
/// 类型检查失败时用它；与 [`problem_box`] 共用“只写一次、只记一次”的约定，
/// 但外壳必须是行内元素，否则会制造新的非法 HTML。
pub(crate) fn render_inline_problem(block: &Block, fmt: &mut dyn Renderer, why: &str) {
    crate::env::misused_problem();
    let detail = ProblemDetail {
        badge: "行内内容有问题".to_string(),
        class: "mdpp-inline-error".to_string(),
        name: block.name.clone(),
        why: why.to_string(),
    };
    write_problem(&detail, block, false, true, fmt);
}

/// 一个可延迟渲染的问题节点：保留具体徽标/名字/原因，渲染时再决定 span 还是框。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProblemCount {
    None,
    Unknown,
    Misused,
}

#[derive(Debug, Clone)]
pub(crate) struct ProblemNode {
    pub(crate) block: Block,
    pub(crate) detail: ProblemDetail,
    pub(crate) echo_body: bool,
    pub(crate) force_inline: bool,
    pub(crate) count: ProblemCount,
}

impl ProblemNode {
    pub(crate) fn new(
        block: Block,
        detail: ProblemDetail,
        echo_body: bool,
        force_inline: bool,
        count: ProblemCount,
    ) -> Self {
        Self {
            block,
            detail,
            echo_body,
            force_inline,
            count,
        }
    }
}

impl NodeValue for ProblemNode {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        match self.count {
            ProblemCount::Unknown => crate::env::unknown_problem(),
            ProblemCount::Misused => crate::env::misused_problem(),
            ProblemCount::None => {}
        }
        write_problem(
            &self.detail,
            &self.block,
            self.echo_body,
            self.force_inline,
            fmt,
        );
    }
}

/// 写出一处诊断：行内上下文写 span，块级上下文写原来的框。
///
/// 诊断只记一次；块框由 `problem_box` 负责记，行内 span 在这里记。
pub(crate) fn write_problem(
    detail: &ProblemDetail,
    block: &Block,
    echo_body: bool,
    force_inline: bool,
    fmt: &mut dyn Renderer,
) {
    if force_inline || crate::env::is_inline() {
        crate::env::note_problem(&detail.badge, &detail.class, &detail.name, &detail.why);
        let mut text = detail.badge.clone();
        if detail.name.is_empty() {
            if !detail.why.is_empty() {
                text.push('：');
                text.push_str(&detail.why);
            }
        } else {
            text.push('：');
            text.push_str(&detail.name);
            if !detail.why.is_empty() {
                text.push('。');
                text.push_str(&detail.why);
            }
        }
        fmt.open("span", &[("class", "mdpp-inline-error".to_string())]);
        fmt.text(&text);
        fmt.close("span");
        return;
    }

    problem_box(
        block,
        fmt,
        &detail.badge,
        &detail.class,
        &detail.why,
        |fmt| {
            if echo_body {
                body_as_code(block, fmt);
            }
        },
    );
}

/// 问题框的公共外壳：徽标、块头、原因，再由调用方补最后一段内容。
///
/// Rust 分发、用户块展开与内置诊断原语 `::problem` 都从这里出去。嵌套渲染超限与
/// `<markdown>` 超限那两处手边没有 [`Block`]、只能拼字符串，走 [`problem_html`]；
/// 两处共用 [`UNKNOWN_CLASS`] / [`MISUSED_CLASS`]，改类名只改常量。
/// 计数由各自的入口负责，这里只管写，避免同一个框被数两次。
pub(crate) fn problem_box(
    block: &Block,
    fmt: &mut dyn Renderer,
    badge: &str,
    class: &str,
    why: &str,
    extra: impl FnOnce(&mut dyn Renderer),
) {
    crate::env::note_problem(badge, class, &block.name, why);
    fmt.cr();
    fmt.open("div", &[("class", format!("mdpp-template {class}"))]);
    fmt.cr();
    head(fmt, badge, block);
    fmt.cr();
    why_line(fmt, why);
    fmt.cr();
    extra(fmt);
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// 只有外壳与原因的问题框，给「手边没有 [`Block`]、又必须拼字符串」的两条路径用
/// （嵌套渲染超限、`<markdown>` 超限）。class 与 [`problem_box`] 共用常量。
pub(crate) fn problem_html(class: &str, why: &str) -> String {
    format!(
        "<div class=\"mdpp-template {class}\"><p class=\"mdpp-template__why\">{}</p></div>",
        html_escape::encode_text(why)
    )
}

/// 徽标 + 块名 + 解析到的参数（回显成等号写法，一眼看出解析成了什么）
///
/// 用户块展开那两个框（[`crate::block::expand`]）也用它 —— 两处各拼一份的话，
/// 改一个 class 名必然只改一半，而那种不一致是静悄悄的
pub(crate) fn head(fmt: &mut dyn Renderer, badge: &str, block: &Block) {
    fmt.open("p", &[("class", "mdpp-template__head".to_string())]);
    fmt.open("span", &[("class", "mdpp-template__badge".to_string())]);
    fmt.text(badge);
    fmt.close("span");
    fmt.open("code", &[("class", "mdpp-template__name".to_string())]);
    fmt.text(&block.name);
    fmt.close("code");
    for (key, value) in &block.params {
        fmt.open("code", &[("class", "mdpp-template__param".to_string())]);
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
    fmt.open("p", &[("class", "mdpp-template__why".to_string())]);
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
/// 要按大小写不敏感地认标签的地方自己转。分节由严格缩进扫描器产生，标签一定非空；
/// 空标签、标签前正文这些情况在扫描阶段就已经报错。
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
        assert!(html.contains("mdpp-template__name"), "{html}");
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
        assert!(!unknown.contains("mdpp-template--problem"), "{unknown}");

        // 名字是注册过的，错在用法：`[link]` 后面没写页面名
        let problem = Mdpp::new().render("::navbox title=x\n  [link]\n    甲\n");
        assert!(problem.contains("mdpp-template--problem"), "{problem}");
        assert!(!problem.contains("template--unknown"), "{problem}");
        assert!(problem.contains("mdpp-template__why"), "{problem}");
    }
}
