//! 内部链接：`[[目标|显示文字]]`
//!
//! 顺序遵循 MediaWiki：**目标在前、显示文字在后**，`|显示文字` 可以省略
//! （省略时显示文字就是目标本身）。注意别写反成 `[[显示文字|目标]]`。
//!
//! ## 渲染成什么由外部决定
//!
//! 这一层只负责**认**出链接、把目标递给 [`crate::link::LinkResolver`]，然后按递回来的
//! 东西渲染（见 [`ResolvedLink`] 的两个变体）。
//!
//! **解析器没递进来**（[`crate::context::links`] 是 `None`）时落成
//! `<a class="wikilink" data-doc="目标">显示文字</a>` —— **故意不带 `href`**。
//! 理由见 `link` 模块：带上了浏览器就会自己走，接单击处理的一方再也轮不到。

use crate::context;
use crate::link::ResolvedLink;
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

        // `class` 单独攒着最后合并 —— HTML 里**一个元素只能有一个 class 属性**，
        // 写两个的话浏览器取第一个，而那会正好把解析器给的那几个类名丢掉。
        let mut classes: Vec<&str> = vec!["wikilink"];
        let mut attrs: Vec<(&str, String)> = Vec::new();

        if let Some(ResolvedLink::Link(target)) = &self.resolved {
            attrs.push(("href", target.url.clone()));
            for (key, value) in &target.attrs {
                if key == "class" {
                    classes.extend(value.split_whitespace());
                } else {
                    // 借用一下这个 String：它活到这次 render 结束，够 fmt 用
                    attrs.push((key.as_str(), value.clone()));
                }
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
        assert!(html.contains(r#"class="wikilink""#), "{html}");
        assert!(html.contains(r#"data-doc="目标文档""#), "{html}");
        assert!(html.contains(">显示文字</a>"), "{html}");
        // 没递解析器时不能带 href：带上了浏览器就会自己走
        assert!(!html.contains(r#"href="#), "{html}");
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
        assert!(html.contains(r#"<span class="redlink">没有的条目 ✗</span>"#), "{html}");
        assert!(!html.contains("<a "), "{html}");
        assert!(!html.contains("data-doc"), "{html}");
    }

    /// 解析器认不出来 → 只留 `data-doc`，不标红也不标蓝
    #[test]
    fn an_unresolved_target_stays_unmarked() {
        let html = render_with(Targets(vec!["存在的条目".into()]), "[[没有的条目]]\n");
        assert!(html.contains(r#"data-doc="没有的条目""#), "{html}");
        assert!(!html.contains("href="), "{html}");
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
                    assert!(html.contains(&format!(r#"href="/page/条目{index}""#)), "{html}");
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