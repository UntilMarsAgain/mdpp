//! 内部链接：`[[目标|显示文字]]`
//!
//! 目标在前、显示文字在后（同 MediaWiki），`|显示文字` 可以省略，省略时显示文字就是
//! 目标本身。
//!
//! 这一层只负责**认**出链接、把目标递给 [`crate::link::LinkResolver`]，然后按递回来的
//! 东西渲染：
//!
//! | 情形 | 渲染成 |
//! |---|---|
//! | [`ResolvedLink::Link`] | `<a href=… class="wikilink">` |
//! | [`ResolvedLink::Node`] | 整个节点归那个函数写（连 `<a>` 都由它决定） |
//! | `None`，**或压根没递解析器** | `<a class="wikilink wikilink--missing" data-missing="true">` —— **红链** |
//!
//! 「解析器说没有」与「压根没人递解析器」渲染成同样的红链：两者都是「调用方不知道
//! `[[某页]]` 存不存在」，而渲染成一条看起来正常的链等于替它宣称「这一页存在」。
//!
//! 红链**故意不带 `href`**（理由见 [`crate::link`]）：带上了浏览器就会自己走，接单击
//! 处理的一方再也轮不到 —— 而那正是「这一页还没有」最需要有人接手的时候。
//! 想换掉它的样子有两条路：递 [`ResolvedLink::Node`] 自己画，或者用自己那份样式接
//! `wikilink--missing` 与 `data-missing="true"` 这两个钩子。

use crate::context;
use crate::link::ResolvedLink;
use crate::telemetry::{debug, trace};
use markdown_it::parser::inline::{InlineRule, InlineState, Text};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

/// 一个 `[[…]]` 链接。
#[derive(Debug)]
pub struct WikiLink {
    /// 笔记里原样写的目标（`|显示文字` 之前那一半），便于排障与回退
    pub doc: String,
    /// 当次渲染递进来的解析器给的结果
    pub resolved: Option<ResolvedLink>,
}

impl NodeValue for WikiLink {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        // 认得出来、且给的是**函数**：整个节点归它写。
        //
        // 注意这里不走下面那条路径 —— 那种情况是「自己画一个 `<a>`」，
        // 这一种是「连 `<a>` 都由它决定」（可能根本不是 `<a>`）。
        if let Some(ResolvedLink::Node(renderer)) = &self.resolved {
            renderer.render(node, fmt);
            return;
        }

        // 认不出来 —— 解析器说没有，或压根没有解析器。两种情形渲染成同一样东西，
        // 理由在模块头。
        let Some(ResolvedLink::Link(target)) = &self.resolved else {
            fmt.open(
                "a",
                &[
                    ("class", "wikilink wikilink--missing".to_string()),
                    ("data-doc", self.doc.clone()),
                    ("data-missing", "true".to_string()),
                ],
            );
            fmt.contents(&node.children);
            fmt.close("a");
            return;
        };

        // `class` 单独攒着最后合并 —— HTML 里**一个元素只能有一个 class 属性**，
        // 写两个的话浏览器取第一个，而那会正好把解析器给的那几个类名丢掉。
        let mut classes: Vec<&str> = vec!["wikilink"];
        let mut attrs: Vec<(&str, String)> = Vec::new();
        attrs.push(("href", target.url.clone()));
        for (key, value) in &target.attrs {
            if key == "class" {
                classes.extend(value.split_whitespace());
            } else {
                // 借用一下这个 String：它活到这次 render 结束，够 fmt 用
                attrs.push((key.as_str(), value.clone()));
            }
        }

        let mut all: Vec<(&str, String)> = Vec::with_capacity(attrs.len() + 2);
        all.push(("class", classes.join(" ")));
        all.push(("data-doc", self.doc.clone()));
        all.extend(attrs);

        fmt.open("a", &all);
        fmt.contents(&node.children);
        fmt.close("a");
    }
}

/// 挂到解析器上
pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<WikiLinkScanner>();
}

/// 造一个 wikilink 节点。
///
/// 扫描器与**直接写链接**的地方（模板页读不出来时那个提示框，见
/// [`crate::block::expand`]）共用这一份：判定与写法都在这里定死，散开写两份
/// 迟早会对不上。
pub fn link_node(doc: &str, text: &str) -> Node {
    // 与扫描器同口径：有当次渲染递进来的解析器才判得出目标在不在
    let resolved = context::links().and_then(|resolver| resolver.resolve(doc));
    // 红链的分两档，因为它们要查的东西不是一件：
    // - `debug`：**压根没递解析器** —— 那一篇里每一个链接都会红，那多半是没配好；
    // - `trace`：递了解析器而它说没有 —— 一篇正文里几百个是常事（写到还没写的页面）。
    if resolved.is_none() {
        match context::links() {
            Some(_) => trace!(doc, "解析器说没有这个目标，红链"),
            None => debug!(doc, "没有链接解析器，这个链接按红链渲染"),
        }
    }
    let mut node = Node::new(WikiLink {
        doc: doc.to_owned(),
        resolved,
    });
    node.children.push(Node::new(Text {
        content: text.to_owned(),
    }));
    node
}

#[doc(hidden)]
pub struct WikiLinkScanner;

impl InlineRule for WikiLinkScanner {
    const MARKER: char = '[';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let body = state.src[state.pos..state.pos_max].strip_prefix("[[")?;
        let end = body.find("]]")?;
        let inner = &body[..end];

        // MediaWiki 顺序：目标在前，显示文字在后
        let (doc, text) = match inner.split_once('|') {
            Some((doc, text)) => (doc, text),
            None => (inner, inner),
        };

        // 目标或显示文字为空都不认，交回普通文本处理
        if doc.is_empty() || text.is_empty() {
            return None;
        }

        // 不要自己推进 state.pos：tokenize 会用这里返回的长度去推进
        let consumed = 2 + end + 2;
        Some((link_node(doc, text), consumed))
    }
}

#[cfg(test)]
mod tests {
    use crate::context::RenderContext;
    use crate::engine::Mdpp;
    use crate::link::{LinkResolver, LinkTarget, NodeRenderer, ResolvedLink};
    use markdown_it::{Node, Renderer};
    use std::sync::Arc;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 认得出来的目标就是这些，其余一律认不出来
    struct Targets(Vec<String>);

    impl LinkResolver for Targets {
        fn resolve(&self, target: &str) -> Option<ResolvedLink> {
            self.0
                .iter()
                .any(|known| known == target)
                .then(|| ResolvedLink::Link(LinkTarget::new(format!("/page/{target}"))))
        }
    }

    fn render_with(resolver: impl LinkResolver + 'static, src: &str) -> String {
        let resolver: Arc<dyn LinkResolver> = Arc::new(resolver);
        Mdpp::new().render_with(src, &RenderContext::new().with_links(resolver))
    }

    #[test]
    fn target_first_display_second() {
        // [[目标文档|显示文字]]
        let html = render("[[目标文档|显示文字]]\n");
        assert!(html.contains(r#"data-doc="目标文档""#), "{html}");
        assert!(html.contains(">显示文字</a>"), "{html}");
    }

    /// **压根没递解析器 → 每一个内链都是红链。**
    ///
    /// 钉的是这一条而不是「渲染得出来」：渲染成一条看起来正常的链，等于替调用方
    /// 宣称「这一页存在」，而那是它没说过、也没法说的事。
    #[test]
    fn with_no_resolver_every_link_is_a_red_link() {
        let html = render("[[甲]] 与 [[乙]]\n");
        assert_eq!(
            html.matches(r#"class="wikilink wikilink--missing""#)
                .count(),
            2,
            "{html}"
        );
        assert_eq!(html.matches(r#"data-missing="true""#).count(), 2, "{html}");
        assert!(!html.contains("href="), "红链不该有 href：{html}");
    }

    /// 解析器说没有 → 也是红链。与「压根没递解析器」**渲染成完全一样的东西**。
    #[test]
    fn a_resolver_saying_no_gives_the_same_red_link() {
        #[derive(Debug)]
        struct FindsNothing;
        impl LinkResolver for FindsNothing {
            fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
                None
            }
        }

        let with = render_with(FindsNothing, "[[甲]]\n");
        let without = Mdpp::new().render("[[甲]]\n");
        assert_eq!(with, without, "「说没有」与「没问」不该长得不一样");
        assert!(with.contains("wikilink--missing"), "{with}");
    }

    /// 红链**不带 href**：带上���浏览器就会自己走，接单击处理的一方再也轮不到 ——
    /// 而那正是「这一页还没有」最需要有人接手的时候。
    #[test]
    fn a_red_link_carries_no_href() {
        let html = render("[[目标]]\n");
        assert!(!html.contains(r#"href="#), "{html}");
        assert!(html.contains(r#"data-doc="目标""#), "{html}");
    }

    #[test]
    fn display_text_defaults_to_the_target() {
        let html = render("[[同名文档]]\n");
        assert!(html.contains(r#"data-doc="同名文档""#), "{html}");
        assert!(html.contains(">同名文档</a>"), "{html}");
    }

    #[test]
    fn broken_links_fall_back_to_text() {
        // 未闭合、空目标、空显示文字，都不该被当成内部链接
        for src in ["[[未闭合\n", "[[|只有显示文字]]\n", "[[只有目标|]]\n"] {
            let html = render(src);
            assert!(!html.contains("wikilink"), "{src:?} 不该被解析: {html}");
        }
    }

    #[test]
    fn stays_literal_inside_code_span() {
        let html = render("`[[a|b]]`\n");
        assert!(!html.contains("wikilink"), "{html}");
    }

    /// 递进来的解析器说存在 → 带上地址
    #[test]
    fn a_resolver_that_answers_gives_out_a_href() {
        let html = render_with(Targets(vec!["存在的条目".into()]), "[[存在的条目]]\n");
        assert!(html.contains(r#"href="/page/存在的条目""#), "{html}");
        assert!(html.contains(r#"data-doc="存在的条目""#), "{html}");
    }

    /// 递进来的解析器**额外给的属性**要跟在 href 后面
    #[test]
    fn extra_attributes_from_the_resolver_come_along() {
        struct WithAttrs;
        impl LinkResolver for WithAttrs {
            fn resolve(&self, target: &str) -> Option<ResolvedLink> {
                Some(ResolvedLink::Link(
                    LinkTarget::new(format!("/p/{target}"))
                        .attr("class", "wikilink--ok")
                        .attr("data-missing", "false"),
                ))
            }
        }
        let html = render_with(WithAttrs, "[[甲]]\n");
        assert!(html.contains(r#"href="/p/甲""#), "{html}");
        assert!(html.contains(r#"data-missing="false""#), "{html}");
        // 两个 class 都得在（这是使用方的事，渲染器只是照写）
        assert!(
            html.contains(r#"class="wikilink wikilink--ok""#),
            "两个 class 要合成一个：{html}"
        );
    }

    /// 解析器可以整个接管节点 —— 这是「目标不是 href，但仍然是个链接」那条路
    #[test]
    fn a_node_renderer_takes_over_the_whole_element() {
        struct Marked;
        impl LinkResolver for Marked {
            fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
                Some(ResolvedLink::Node(Box::new(
                    |node: &Node, fmt: &mut dyn Renderer| {
                        fmt.open("span", &[("class", "redlink".to_string())]);
                        fmt.contents(&node.children);
                        fmt.text(" ✗");
                        fmt.close("span");
                    },
                )))
            }
        }
        let html = render_with(Marked, "[[没有的条目]]\n");
        // 整个节点归它：不是 `<a>`，也没有 data-doc
        assert!(
            html.contains(r#"<span class="redlink">没有的条目 ✗</span>"#),
            "{html}"
        );
        assert!(!html.contains("<a "), "{html}");
        assert!(!html.contains("data-doc"), "{html}");
    }

    /// 解析器认得的那几条 → **不带**红链的记号，两者不会混
    #[test]
    fn a_known_target_is_not_marked_missing() {
        let html = render_with(Targets(vec!["存在的条目".into()]), "[[存在的条目]]\n");
        assert!(html.contains(r#"href="/page/存在的条目""#), "{html}");
        assert!(
            !html.contains("data-missing"),
            "认得的链接不该标成没找到：{html}"
        );
        assert!(!html.contains("wikilink--missing"), "{html}");
    }

    /// 渲染状态必须是「每次调用各自一份」：并行渲染时不能互相干扰。
    ///
    /// 这条有来历 —— 状态若挂在解析器的全局 `ext` 集合上，并行跑测试立刻出现
    /// 「一个线程的还原打断了另一个线程的渲染」。
    #[test]
    fn the_resolver_does_not_leak_across_threads() {
        let handles: Vec<_> = (0..4)
            .map(|index| {
                std::thread::spawn(move || {
                    let html = render_with(
                        Targets(vec![format!("条目{index}")]),
                        &format!("[[条目{index}]]\n"),
                    );
                    assert!(
                        html.contains(&format!(r#"href="/page/条目{index}""#)),
                        "{html}"
                    );
                    assert!(html.contains("wikilink"), "{html}");
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn node_renderer_object_works_too() {
        // 不是闭包而是一个实现了 trait 的值，同样能塞进去
        struct Shouty;
        impl NodeRenderer for Shouty {
            fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
                fmt.open("b", &[]);
                fmt.contents(&node.children);
                fmt.close("b");
            }
        }
        struct ViaType;
        impl LinkResolver for ViaType {
            fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
                Some(ResolvedLink::Node(Box::new(Shouty)))
            }
        }
        let html = render_with(ViaType, "[[随便]]\n");
        assert!(html.contains("<b>随便</b>"), "{html}");
    }
}
