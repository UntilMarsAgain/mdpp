//! 内部链接的解析器接口。
//!
//! [`ResolvedLink`] 的两个变体是两种不同的目标性质：
//!
//! - [`ResolvedLink::Link`]：目标出得去，给一个 `href`。
//! - [`ResolvedLink::Node`]：目标没有 `href`，但仍是个链接（人会点它），只是这一下
//!   点击该由谁接手由使用方决定。
//!
//! 第二种刻意不给 `href`：`href` 一旦有，浏览器就会自己走，使用方的单击处理
//! 根本轮不到。
//!
//! ## `None` 与「压根没递解析器」
//!
//! 两者渲染成**同样的红链**。调用方不知道目标在不在，渲染成一条看起来正常的链
//! 等于替它宣称「这一页存在」。
//! 浏览器就会自己走。

use std::fmt;

use markdown_it::{Node, Renderer};

/// 内部链接的解析器。
///
/// 实现方持有自己那一侧的东西（仓库索引、路由表…），本库一概不看。
///
/// 要求：**同一段原文、同一个目标，每次给出同一个答案** —— 渲染是纯函数，
/// 解析器若在两次渲染之间改主意，缓存下来的 HTML 就会是错的。
pub trait LinkResolver: Send + Sync {
    /// 解析一个链接目标（`[[…]]` 里 `|` 之前的那一半）。
    ///
    /// `None` = 认不出来，落成红链。
    fn resolve(&self, target: &str) -> Option<ResolvedLink>;
}

/// 解析出来的内部链接。
#[derive(Debug)]
pub enum ResolvedLink {
    /// 带 `href` 的 `<a>`。
    Link(LinkTarget),
    /// 整个节点由这个函数写（连 `<a>` 都由它决定）。
    Node(Box<dyn NodeRenderer>),
}

/// 一个可跳转的地址，以及要跟在 `<a>` 上的额外属性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// `href` 的值
    pub url: String,
    /// 额外属性（`class`、`data-*` …），按给出的次序写在 `href` 之后。
    ///
    /// 值不检查，由渲染器转义。`class` 会与解析器自带的那个合并，不写第二个
    /// `class` 属性。
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
/// 拿到的 [`Node`] 带着**已经渲染好的子节点**（`[[目标|显示文字]]` 里的显示文字），
/// 所以最省事的实现就是照着 `<a>` 写一圈：
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
