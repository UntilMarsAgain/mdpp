//! 内部链接：解析器在外面，目标也在外面 —— 这里只定**怎么递、递回来什么**。
//!
//! ## 两种递回来的东西
//!
//! [`ResolvedLink`] 有两个变体，是两种**不同的目标性质**，不是两种口味：
//!
//! - [`ResolvedLink::Link`]：目标是**出得去的地方**，该给一个 `href`。
//!   —— 跨站链接、指向某个页面路由的地址。
//! - [`ResolvedLink::Node`]：目标没有 `href`，但**仍然是个链接**（人会点它），
//!   只是这一下点击该由谁接手，是使用方自己的事。
//!   —— 单页应用里的一页：`<a>` 不带 href，点它由前端拦下并替换正文。
//!
//! 第二种不给 `href` 不是缺陷而是要紧的设计：`href` 一旦有，浏览器就会自己走，
//! 使用方的单击处理根本轮不到。所以「点它会发生什么」必须是显式递进来的东西。
//!
//! ## 返回 `None` 的含义
//!
//! **认不出来** —— 那一段留作字面文本（渲染成不带 `href` 的 `<a class="wikilink" data-doc=…>`，
//! 至少把原文留着，见 `syntax::wikilink`）。宁可什么都不承诺，也不要给一个错的地址。

use std::fmt;

use markdown_it::{Node, Renderer};

/// 内部链接的解析器。
///
/// 实现方通常持有自己那一侧的东西：仓库索引、路由表、命名空间表 —— 本库一概不看。
/// 它要保证的只有一件事：**同一段原文、同一个目标，每次给出同一个答案**。渲染是
/// 纯函数，解析器若在两次渲染之间改主意，缓存下来的 HTML 就会是错的。
pub trait LinkResolver: Send + Sync {
    /// 解析一个链接目标（`[[…]]` 里 `|` 之前的那一半）。
    ///
    /// `None` = 认不出来，那一段留作字面文本。
    fn resolve(&self, target: &str) -> Option<ResolvedLink>;
}

/// 解析出来的内部链接。
#[derive(Debug)]
pub enum ResolvedLink {
    /// 出得去的目标：渲染成带 `href` 的 `<a>`。
    Link(LinkTarget),
    /// 没有 `href`、但仍然是个链接：函数自己写整个节点。
    Node(Box<dyn NodeRenderer>),
}

/// 一个可跳转的地址，以及要跟在 `<a>` 上的额外属性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// `href` 的值
    pub url: String,
    /// 额外属性（`class`、`data-*` …）。按给出的次序写在 `href` 之后。
    ///
    /// 刻意**不**检查值：它不是 CSS，也不进 `style`，是 HTML 属性值 ——
    /// 渲染器（markdown-it）负责转义。
    pub attrs: Vec<(String, String)>,
}

impl LinkTarget {
    /// 就是一个地址，没有额外属性
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            attrs: Vec::new(),
        }
    }

    /// 挂一个属性（链式）
    pub fn attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.push((key.into(), value.into()));
        self
    }
}

/// 自己写出一个节点。
///
/// 实现方拿到的 [`Node`] 带着**已经渲染好的子节点**（就是 `[[目标|显示文字]]` 里的
/// 显示文字），所以最省事的实现是照着 `<a>` 写一圈：
///
/// ```
/// use mdpp::{link::NodeRenderer, markdown_it::{Node, Renderer}};
///
/// struct Mark;
///
/// impl NodeRenderer for Mark {
///     fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
///         fmt.open("a", &[("class", "wikilink".to_string())]);
///         fmt.contents(&node.children);
///         fmt.close("a");
///     }
/// }
/// ```
pub trait NodeRenderer: Send + Sync {
    /// 写出整个节点 —— 开标签、属性、子节点、闭标签都归你。
    fn render(&self, node: &Node, fmt: &mut dyn Renderer);
}

/// 闭包也算一个 `NodeRenderer`。
///
/// 这一条 blanket impl 与 [`crate::block::template`] 那边的选择是**反着**的，
/// 说清区别免得看着像自相矛盾：
///
/// - 这里给的是**写法**上的方便 —— 写一个闭包就是「直接说出要渲染成什么」，
///   里面没有任何「东西存在哪」的知识；
/// - `TemplateSource` 与 [`LinkResolver`] 上刻意**不给**任何实现 —— 那两个 trait
///   的内容恰恰就是「东西存在哪」，给一个现成的就会把最常见的那种存法定成默认答案。
impl<F> NodeRenderer for F
where
    F: Fn(&Node, &mut dyn Renderer) + Send + Sync,
{
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        self(node, fmt)
    }
}

impl fmt::Debug for dyn NodeRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NodeRenderer")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 什么都认得，认作一个普通地址
    pub(crate) struct AnyTarget;

    impl LinkResolver for AnyTarget {
        fn resolve(&self, target: &str) -> Option<ResolvedLink> {
            Some(ResolvedLink::Link(LinkTarget::new(format!(
                "/page/{target}"
            ))))
        }
    }

    /// 什么都不认得 —— 全部留作字面文本
    pub(crate) struct NoneAtAll;

    impl LinkResolver for NoneAtAll {
        fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
            None
        }
    }

    /// 一个闭包就是一个 `NodeRenderer` —— 这是最常见的那种写法
    #[test]
    fn a_closure_is_a_node_renderer() {
        fn write_through(_node: &Node, fmt: &mut dyn Renderer) {
            fmt.text("写了");
        }
        let boxed: Box<dyn NodeRenderer> = Box::new(|_node: &Node, fmt: &mut dyn Renderer| {
            fmt.text("写了");
        });
        assert_eq!(std::mem::size_of_val(&*boxed), std::mem::size_of::<()>());
        // 两条路都编译得过（一个函数项、一个闭包），那就够了
        let _ = write_through;
    }

    #[test]
    fn link_target_chains_attrs() {
        let target = LinkTarget::new("/x")
            .attr("class", "wikilink")
            .attr("data-id", "7");
        assert_eq!(target.url, "/x");
        assert_eq!(
            target.attrs,
            vec![
                ("class".to_string(), "wikilink".to_string()),
                ("data-id".to_string(), "7".to_string())
            ]
        );
    }
}
