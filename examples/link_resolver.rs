// 内部链接解析器：接住 `[[目标]]`，决定它渲染成什么。
//
//     cargo run --example link-resolver
//
// 递回来的东西有两种，是**目标性质的不同**，不是口味：
//
// - `ResolvedLink::Link` —— 目标是出得去的地方，给一个 `href`；
// - `ResolvedLink::Node` —— 目标没有 `href`，但**仍然是个链接**（人会点它），
//   只是「点它会发生什么」由使用方决定。
//
// 第二种不给 `href` 不是缺陷：`href` 一旦有，浏览器就会自己走，接单击处理的
// 那一侧根本轮不到。
//
// 跑法：`cargo run --example link-resolver`

use std::collections::HashSet;
use std::sync::Arc;

use mdpp::markdown_it::{Node, Renderer};
use mdpp::{
    link::{LinkResolver, LinkTarget, NodeRenderer, ResolvedLink},
    Mdpp, RenderContext,
};

/// 一个说得清「哪些页存在」的假仓库
#[derive(Debug)]
struct FakeVault {
    existing: HashSet<String>,
}

/// 一个「在不在仓库里」的解析器：存在的给地址，不存在的标成红链。
///
/// 红链那条走的是 `Node` —— 单页应用里点它该由前端拦下替换正文，所以**不能**给
/// `href`（给了浏览器就自己走了）。也正因为它**是个链接**，所以仍然渲染成
/// `<a>`、仍然带 `data-doc`。
impl LinkResolver for FakeVault {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        // 带命名空间前缀的一律当成「别人家的页」：出得去就出得去
        if let Some(site) = target.strip_prefix("外站:") {
            return Some(ResolvedLink::Link(
                LinkTarget::new(format!("https://example.org/wiki/{site}")).attr("rel", "external"),
            ));
        }

        if self.existing.contains(target) {
            return Some(ResolvedLink::Link(
                LinkTarget::new(format!("/notes/{target}")).attr("data-page", target),
            ));
        }

        // 不存在 → 红链。整段交给函数，连 `<a>` 都由它开
        Some(ResolvedLink::Node(Box::new(RedLink)))
    }
}

/// 红链：点它的人知道「这一页还没写」
struct RedLink;

impl NodeRenderer for RedLink {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("a", &[("class", "wikilink wikilink--missing".to_string())]);
        fmt.contents(&node.children);
        fmt.text("（还没写）");
        fmt.close("a");
    }
}

/// 什么都不认的解析器 —— 用来对照「没递解析器」时是什么样
#[derive(Debug)]
struct NoVault;

impl LinkResolver for NoVault {
    fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
        None
    }
}

fn main() {
    let source = "[[已经写了的那页]]、[[还没写的那页]]、[[外站:平陆运河]]\n";

    let mut existing = HashSet::new();
    existing.insert("已经写了的那页".to_string());
    let vault = Arc::new(FakeVault { existing });

    println!("== 递了解析器 ==\n");
    let context = RenderContext::new().with_links(vault);
    println!("{}", Mdpp::new().render_with(source, &context));

    println!("== 递了一个什么都不认的 ==\n");
    let context = RenderContext::new().with_links(Arc::new(NoVault));
    println!("{}", Mdpp::new().render_with(source, &context));

    println!("== 压根没递解析器 ==\n");
    println!("{}", Mdpp::new().render(source));
}
