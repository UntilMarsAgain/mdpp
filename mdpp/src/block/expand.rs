//! **用户自定义块**的展开：`::卡片` → 找模板页 `卡片` → 填 `{{param}}` → 解析成节点。
//!
//! 与内置块的界在名字：内置名字按注册表走，非内置名字才来这里找同名页。模板页本身
//! 不声明输出形态；展开后把产物缓冲下来，按调用处的期望和实际识别出的形态决定是否
//! 补段落、剥段落或回滚成诊断。
//!
//! 两种「停下来说清楚」的情形都返回一段说明而不是硬展开：这一页**在但读不出来**
//! （报成「未知块」会把作者引向错的方向），以及套得太深（多半是块自己嵌自己）。

use std::sync::Arc;

use markdown_it::parser::block::BlockState;
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

use crate::block::dispatch;
use crate::block::fill;
use crate::block::template::{self, TemplatePage, TemplateVars, page_key};
use crate::block::typecheck::{self, TemplateType};
use crate::context;
use crate::registry::Block;
use crate::runtime::capture::Capture;
use crate::runtime::env::ProblemDetail;
use crate::syntax::wikilink;
use crate::telemetry::{debug, trace, warn};

/// 展开一个**用户自定义块**：模板页的正文 → 填 `{{param}}` → 解析成节点。
///
/// `page` 由扫描器传入：它先查过一次「这是不是模板」，这里不再查第二遍。
/// 返回的节点在渲染时重新进入模板作用域，`{{@名字}}` 在那里才取值。
pub(crate) fn expand_user_block(
    name: &str,
    params: Vec<(String, String)>,
    body: String,
    page: TemplatePage,
    state: &mut BlockState,
) -> Node {
    expand_with_md(name, params, body, page, state.md, None)
}

/// 行内调用 `::名字[...]` 的展开：输出类型强制为 inline。
pub(crate) fn expand_inline_block(
    name: &str,
    params: Vec<(String, String)>,
    body: String,
    page: TemplatePage,
    md: &MarkdownIt,
) -> Node {
    expand_with_md(name, params, body, page, md, Some(TemplateType::Inline))
}

fn expand_with_md(
    name: &str,
    params: Vec<(String, String)>,
    body: String,
    page: TemplatePage,
    md: &MarkdownIt,
    forced: Option<TemplateType>,
) -> Node {
    let inline = matches!(forced, Some(TemplateType::Inline));
    // 转圈的保护要在**取到正文之后**查：取不到就没有「套」这回事
    let source = match page {
        TemplatePage::Ready(text) => {
            trace!(name, bytes = text.len(), "展开用户块");
            text
        }
        TemplatePage::Unreadable { reason } => {
            debug!(name, %reason, "模板页在，但这一刻读不出来");
            if inline {
                return inline_problem_node(
                    name,
                    "模板页读不出来",
                    "mdpp-template--unreadable",
                    &reason,
                    dispatch::ProblemCount::None,
                );
            }
            return unreadable_node(name, &reason);
        }
    };

    let limit = context::max_template_depth();
    if context::depth() >= limit {
        // warn 而不是 debug：作者多半不知道自己写了个转圈的块
        warn!(name, limit, "块嵌套超过上限，多半是自嵌");
        let why = format!("块嵌套超过 {limit} 层（是不是自己嵌自己？）");
        if inline {
            return inline_problem_node(
                name,
                "块嵌套太深",
                dispatch::MISUSED_CLASS,
                &why,
                dispatch::ProblemCount::Misused,
            );
        }
        return reported_node("块嵌套太深", dispatch::MISUSED_CLASS, name, &why);
    }

    let kind = match forced {
        Some(kind) => {
            if params.iter().any(|(key, _)| key == "type") {
                return inline_problem_node(
                    name,
                    "行内调用写错了",
                    dispatch::MISUSED_CLASS,
                    "行内调用已经固定为 inline，不要再写 type=",
                    dispatch::ProblemCount::Misused,
                );
            }
            kind
        }
        None => match params
            .iter()
            .find(|(key, _)| key == "type")
            .map(|(_, value)| value.as_str())
        {
            Some(value) => match TemplateType::parse(value) {
                Ok(kind) => kind,
                Err(why) => return header_problem(name, params, body, &why),
            },
            None => TemplateType::Block,
        },
    };
    // 把类型也作为普通参数递给模板：`{{type}}` 是早绑定文本，`{{@type}}` 由
    // `TemplateVars` 的 kind 提供。调用处写的大小写不保留，统一成规范值。
    let mut params = params;
    params.retain(|(key, _)| key != "type");
    params.push(("type".to_string(), kind.as_str().to_string()));

    let block = Block {
        name: name.to_string(),
        params: params.clone(),
        body: body.clone(),
    };

    // 作用域要覆盖**解析期**：模板里嵌套的 `::卡片 title={{@标题}}` 在解析时就要
    // 拿到外层的值。节点上再存一份，渲染 `::if` / `::demo` 时重新进入。
    let vars = Arc::new(TemplateVars::new(
        name,
        params,
        body.clone(),
        kind,
        context::template_vars(),
    ));
    let _scope = context::enter_template_vars(Arc::clone(&vars));

    let filled = match fill::substitute_checked(&source, &block, &body) {
        Ok(filled) => filled,
        Err(why) => {
            if inline {
                return inline_problem_node(
                    name,
                    "模板参数缺失",
                    dispatch::MISUSED_CLASS,
                    &why,
                    dispatch::ProblemCount::Misused,
                );
            }
            return reported_node("模板参数缺失", dispatch::MISUSED_CLASS, name, &why);
        }
    };

    let mut parsed = context::deeper(|| crate::parse::markdown::parse_markdown(md, &filled));
    let mut node = Node::new(ScopedBlock { block, vars, kind });
    node.children = std::mem::take(&mut parsed.children);
    node
}

/// 造一个必须按行内形态输出的问题节点。
fn inline_problem_node(
    name: &str,
    badge: &str,
    class: &str,
    why: &str,
    count: dispatch::ProblemCount,
) -> Node {
    Node::new(dispatch::ProblemNode::new(
        Block {
            name: name.to_string(),
            params: Vec::new(),
            body: String::new(),
        },
        ProblemDetail {
            badge: badge.to_string(),
            class: class.to_string(),
            name: name.to_string(),
            why: why.to_string(),
        },
        false,
        true,
        count,
    ))
}

/// 用户块展开后的节点：它自己没有固定 HTML，而是缓冲解析出来的子节点。
///
/// 渲染前把模板作用域装回来 —— `::if` 的 `{{@body}}`、模板里再嵌模板的晚绑定
/// 参数都要靠它；随后按 [`typecheck::classify`] 识别实际产物，与调用处的
/// [`TemplateType`] 比较。
#[derive(Debug)]
struct ScopedBlock {
    block: Block,
    vars: Arc<TemplateVars>,
    kind: TemplateType,
}

impl NodeValue for ScopedBlock {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let _scope = context::enter_template_vars(Arc::clone(&self.vars));
        let inline = self.kind == TemplateType::Inline;
        let _inline_scope = inline.then(crate::env::enter_inline);

        // 模板不声明自己输出什么，调用处只说这次需要什么。先把产物完整收进
        // 缓冲区，再识别实际形态；失败时整段回滚，不能把半截 HTML 写出去。
        let snapshot = crate::env::snapshot();
        let outer_detail = if inline {
            crate::env::problem_detail()
        } else {
            None
        };
        if inline {
            crate::env::clear_problem_detail();
        }
        let mut capture = Capture::default();
        capture.contents(&node.children);
        let html = capture.into_string();
        let inner_detail = if inline {
            crate::env::problem_detail()
        } else {
            None
        };

        let classified = match typecheck::classify(&html) {
            Ok(classified) => classified,
            Err(why) => {
                if inline {
                    fail_inline(&self.block, inner_detail, &why, snapshot, fmt);
                } else {
                    if let Some(snapshot) = snapshot {
                        crate::env::restore(snapshot);
                    }
                    fmt.cr();
                    dispatch::render_problem(&self.block, fmt, &why);
                    fmt.cr();
                }
                return;
            }
        };

        match self.kind {
            TemplateType::Inline => {
                if let Some(inline_html) = classified.inline {
                    crate::env::set_problem_detail(outer_detail.or(inner_detail));
                    fmt.text_raw(&inline_html);
                } else {
                    fail_inline(
                        &self.block,
                        inner_detail,
                        "行内调用收到块级输出，不能写进行内位置",
                        snapshot,
                        fmt,
                    );
                }
            }
            TemplateType::Block => {
                fmt.cr();
                match classified.kind {
                    typecheck::OutputKind::Inline => {
                        let inline = classified.inline.unwrap_or_default();
                        if !inline.is_empty() {
                            fmt.open("p", &[]);
                            fmt.text_raw(&inline);
                            fmt.close("p");
                        }
                    }
                    typecheck::OutputKind::Paragraph | typecheck::OutputKind::Block => {
                        fmt.text_raw(&classified.html);
                    }
                }
                fmt.cr();
            }
        }
    }
}

/// 行内模板失败：恢复半成品，只留下一个保留原始诊断的 span。
fn fail_inline(
    block: &Block,
    inner: Option<ProblemDetail>,
    fallback: &str,
    snapshot: Option<crate::env::Snapshot>,
    fmt: &mut dyn Renderer,
) {
    if let Some(snapshot) = snapshot {
        crate::env::restore(snapshot);
    }
    let detail = inner.unwrap_or_else(|| ProblemDetail {
        badge: "行内内容有问题".to_string(),
        class: "mdpp-template--problem".to_string(),
        name: block.name.clone(),
        why: fallback.to_string(),
    });
    if detail.class.ends_with("--unknown") {
        crate::env::unknown_problem();
    } else {
        crate::env::misused_problem();
    }
    dispatch::write_problem(&detail, block, false, true, fmt);
}

/// 模板调用头写错了：按「用法有问题」的框渲染，而不是退回普通段落。
///
/// 扫描器已经知道这个名字是模板（或 `::if`），所以这里可以放心把错误摆出来；
/// 普通块的头解析失败仍走原来的宽松回退。
pub(crate) fn header_problem(
    name: &str,
    params: Vec<(String, String)>,
    body: String,
    why: &str,
) -> Node {
    Node::new(HeaderProblem {
        block: Block {
            name: name.to_string(),
            params,
            body,
        },
        why: why.to_string(),
    })
}

#[derive(Debug)]
struct HeaderProblem {
    block: Block,
    why: String,
}

impl NodeValue for HeaderProblem {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        dispatch::render_problem(&self.block, fmt, &self.why);
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
        if self.class == dispatch::MISUSED_CLASS {
            crate::env::misused_problem();
        }
        let detail = ProblemDetail {
            badge: self.badge.to_string(),
            class: self.class.to_string(),
            name: self.block.name.clone(),
            why: self.block.body.clone(),
        };
        if crate::env::is_inline() {
            dispatch::write_problem(&detail, &self.block, false, true, fmt);
            return;
        }
        // 外壳、徽标与「为什么」都交给分发那边写：几个框的 class 名与结构在一处
        // 定死，改一处就一起改；这里只补「下一步去改哪一页」。
        dispatch::problem_box(
            &self.block,
            fmt,
            self.badge,
            self.class,
            &self.block.body,
            |fmt| {
                fmt.open("p", &[("class", "mdpp-template__where".to_string())]);
                fmt.contents(&node.children);
                fmt.close("p");
            },
        );
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
    reported_node("模板页读不出来", "mdpp-template--unreadable", name, reason)
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
        render_call(page, source, &format!("::{page}\n"))
    }

    fn render_call(page: &str, source: TemplatePage, call: &str) -> String {
        let mut pages = Pages::default();
        pages.0.insert(page.to_string(), source);
        let context = RenderContext::new().with_templates(Arc::new(pages));
        Mdpp::new().render_with(call, &context)
    }

    fn render_pages(pages: &[(&str, &str)], call: &str) -> String {
        let mut map = Pages::default();
        for (name, source) in pages {
            map.0.insert(
                (*name).to_string(),
                TemplatePage::Ready((*source).to_string()),
            );
        }
        let context = RenderContext::new().with_templates(Arc::new(map));
        Mdpp::new().render_with(call, &context)
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
        assert!(html.contains("mdpp-template--unreadable"), "{html}");
        assert!(html.contains("模板页读不出来"), "{html}");
        assert!(html.contains("读不了：权限不够"), "{html}");
        // 框里顺带把**那一页的名字**指给作者：下一步就是去改它
        assert!(html.contains("template:卡片"), "{html}");
    }

    /// 自己嵌自己：允许递归，转到新的上限才停下并说清是哪一层
    #[test]
    fn a_block_that_embeds_itself_stops_at_the_depth_limit() {
        let html = render("转圈", TemplatePage::Ready("::转圈\n  甲\n".into()));
        assert!(html.contains("块嵌套太深"), "{html}");
        assert!(
            html.contains(&format!("超过 {MAX_TEMPLATE_DEPTH} 层")),
            "{html}"
        );
        // 那句话是给人看的建议，不是把内容吞了 —— 还给了那一页的名字
        assert!(html.contains("是不是自己嵌自己"), "{html}");
        assert!(html.contains("template:转圈"), "{html}");
        assert!(!html.contains("mdpp-template--unreadable"), "{html}");
    }

    /// `::demo` 会在渲染期再解析正文，过去能绕过模板深度保护把栈打穿。
    /// 现在渲染期另有一条预算，输出是一个诊断框，不是崩溃。
    #[test]
    fn demo_nested_inside_a_template_cannot_recurse_forever() {
        let mut pages = Pages::default();
        pages.0.insert(
            "套娃".to_string(),
            TemplatePage::Ready("::demo\n  ::套娃\n".into()),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));
        let report = Mdpp::new().render_report("::套娃\n", &context);
        assert!(report.html.contains("嵌套渲染超过"), "{}", report.html);
        // 渲染期的嵌套超限同样算一个误用框
        assert!(report.problems.misused >= 1, "{:?}", report.problems);
    }

    /// 嵌套上限由 `RenderContext` 传：同一个模板，1 层停、2 层通
    #[test]
    fn the_template_depth_limit_comes_from_the_context() {
        let mut pages = Pages::default();
        pages
            .0
            .insert("内".to_string(), TemplatePage::Ready("IN".to_string()));
        pages
            .0
            .insert("外".to_string(), TemplatePage::Ready("::内".to_string()));
        let pages = Arc::new(pages);

        let one = Mdpp::new().render_with(
            "::外\n",
            &RenderContext::new()
                .with_templates(pages.clone())
                .with_max_template_depth(1),
        );
        assert!(one.contains("块嵌套超过 1 层"), "{one}");

        let two = Mdpp::new().render_with(
            "::外\n",
            &RenderContext::new()
                .with_templates(pages)
                .with_max_template_depth(2),
        );
        assert!(two.contains("IN"), "{two}");
    }

    /// 模板里再调模板：外层 `{{@body}}` 在解析期就能填进内层参数
    #[test]
    fn a_nested_template_call_sees_the_outer_scope() {
        let mut pages = Pages::default();
        pages.0.insert(
            "内".to_string(),
            TemplatePage::Ready("IN=[{{title}}]".to_string()),
        );
        pages.0.insert(
            "外".to_string(),
            TemplatePage::Ready("::内 title={{@body}}".to_string()),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));
        let html = Mdpp::new().render_with("::外\n  你好 世界\n", &context);
        assert!(html.contains("IN=[你好 世界]"), "{html}");
    }

    /// 模板参数缺失时报出来，`{{?名字}}` 则按空串填 —— 不再原样留下一段 `{{...}}`
    #[test]
    fn a_missing_template_parameter_is_reported_unless_it_is_optional() {
        let required = render("卡片", TemplatePage::Ready("标题：{{title}}".into()));
        assert!(required.contains("mdpp-template--problem"), "{required}");
        assert!(required.contains("title"), "{required}");

        let optional = render("卡片", TemplatePage::Ready("标题：{{?title}}".into()));
        assert!(!optional.contains("mdpp-template--problem"), "{optional}");
        assert!(optional.contains("标题："), "{optional}");
        assert!(!optional.contains("{{"), "{optional}");
    }

    /// 正文是多行、模板里的 `{{body}}` 又缩进着：填完的每一行都得落在那份缩进里，
    /// 否则 `::code` 只收下第一行，其余漏成页面内容（语法页的分栏模板踩的就是这个）
    #[test]
    fn a_multiline_body_keeps_the_indent_of_the_placeholder() {
        let mut pages = Pages::default();
        pages.0.insert(
            "对照".to_string(),
            TemplatePage::Ready("::code lang=mdpp\n  {{body}}\n".into()),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));
        let html = Mdpp::new().render_with("::对照\n  甲\n  乙\n", &context);
        // 两行都在代码块里
        assert!(html.contains("甲\n乙"), "{html}");
        // 第二行没有漏到代码块外面
        assert!(!html.contains("<p>乙</p>"), "{html}");
    }

    /// 没有这一页 → `None`，调用方（[`crate::block::dispatch`]）照旧报「未知块」。
    ///
    /// 这条钉的是**分工**：展开这条路只处理「有这一页」，不越界替别人报错
    #[test]
    fn no_such_page_falls_through_to_the_unknown_box() {
        let html = Mdpp::new().render("::压根没有这一页\n");
        assert!(html.contains("template--unknown"), "{html}");
        assert!(!html.contains("mdpp-template--unreadable"), "{html}");
    }

    #[test]
    fn a_template_can_be_declared_inline_and_unwraps_one_paragraph() {
        let html = render_call(
            "卡片",
            TemplatePage::Ready("一段**文字**".into()),
            "::卡片 type=inline\n",
        );
        assert!(html.contains("一段<strong>文字</strong>"), "{html}");
        assert!(!html.contains("<p>"), "{html}");
    }

    #[test]
    fn an_inline_template_can_output_pure_inline_markup() {
        let html = render_call(
            "卡片",
            TemplatePage::Ready("::element tag=span inline=on\n  行内".into()),
            "::卡片 type=inline\n",
        );
        assert!(html.contains("<span>"), "{html}");
        assert!(html.contains("行内"), "{html}");
        assert!(!html.contains("<p>"), "{html}");
        assert!(!html.contains("template--problem"), "{html}");
    }

    #[test]
    fn a_block_shaped_inline_template_is_refused() {
        let html = render_call(
            "卡片",
            TemplatePage::Ready("::element tag=div\n  块内容".into()),
            "::卡片 type=inline\n",
        );
        assert!(html.contains("mdpp-inline-error"), "{html}");
        assert!(!html.contains("mdpp-template__badge"), "{html}");
        assert!(!html.contains("<div>"), "{html}");
    }

    #[test]
    fn type_is_available_to_the_template_and_conditionals() {
        let source = concat!(
            "::if condition-equals=\"{{@type}}\" value=inline\n",
            "  [then]\n",
            "    ::inline\n",
            "      行内 {{type}}\n",
            "  [else]\n",
            "    ::element tag=div\n",
            "      块 {{type}}\n",
        );
        let inline = render_call(
            "卡片",
            TemplatePage::Ready(source.into()),
            "::卡片 type=inline\n",
        );
        assert!(inline.contains("行内 inline"), "{inline}");
        assert!(!inline.contains("<div>"), "{inline}");

        let block = render_call("卡片", TemplatePage::Ready(source.into()), "::卡片\n");
        assert!(block.contains("<div>"), "{block}");
        assert!(block.contains("块 block"), "{block}");
    }

    #[test]
    fn an_invalid_template_type_is_a_problem() {
        let html = render_call(
            "卡片",
            TemplatePage::Ready("一段".into()),
            "::卡片 type=wat\n",
        );
        assert!(html.contains("mdpp-template--problem"), "{html}");
        assert!(html.contains("type="), "{html}");
    }

    #[test]
    fn inline_wrappers_use_the_same_check_for_template_output() {
        let html = render_pages(
            &[
                ("卡片", "一段**文字**"),
                ("框", "::element tag=span inline=on\n  ::卡片"),
            ],
            "::框 type=inline\n",
        );
        assert!(html.contains("<span>"), "{html}");
        assert!(html.contains("<strong>文字</strong>"), "{html}");
        assert!(!html.contains("<p>"), "{html}");
        assert!(!html.contains("template--problem"), "{html}");
    }

    #[test]
    fn a_block_call_wraps_inline_template_output_in_one_paragraph() {
        let html = render_call(
            "卡片",
            TemplatePage::Ready("::element tag=span inline=on\n  行内".into()),
            "::卡片\n",
        );
        assert!(html.contains("<p><span>"), "{html}");
        assert!(html.contains("行内"), "{html}");
        assert_eq!(html.matches("<p>").count(), 1, "{html}");
    }

    #[test]
    fn a_block_call_leaves_a_paragraph_template_alone() {
        let html = render_call("卡片", TemplatePage::Ready("一段".into()), "::卡片\n");
        assert_eq!(html.matches("<p>").count(), 1, "{html}");
        assert!(!html.contains("<p><p>"), "{html}");
    }

    #[test]
    fn a_failed_inline_template_rolls_back_its_styles() {
        let source = concat!(
            "::css\n",
            "  .rollback-inline { color: red; }\n",
            "::element tag=div\n",
            "  块内容\n",
        );
        let html = render_call(
            "卡片",
            TemplatePage::Ready(source.into()),
            "::卡片 type=inline\n",
        );
        assert!(html.contains("mdpp-inline-error"), "{html}");
        assert!(!html.contains("rollback-inline"), "{html}");
    }
}
