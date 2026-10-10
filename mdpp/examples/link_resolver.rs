// 内部链接：`[[目标]]` 的三种结局。
//
//     cargo run -p mdpp --example link_resolver
//
// | 情形 | 渲染成 |
// |---|---|
// | `ResolvedLink::Link` | 带 `href` 的普通链 |
// | `ResolvedLink::Node` | 整个节点归那个函数写（连 `<a>` 都由它决定） |
// | `None`，或压根没递解析器 | **红链**：`class="wikilink wikilink--missing"` |

use std::collections::HashSet;
use std::sync::Arc;

use mdpp::markdown_it::{Node, Renderer};
use mdpp::{
    Mdpp, RenderContext,
    link::{LinkResolver, LinkTarget, NodeRenderer, ResolvedLink},
};

/// 一个说得清「哪些页存在」的假仓库
#[derive(Debug)]
struct FakeVault {
    existing: HashSet<String>,
}

/// 认得出的给地址，认不出的什么都不给 —— 后者就落成红链。
impl LinkResolver for FakeVault {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        // 带命名空间前缀的一律当成「别人家的页」：出得去就出得去
        if let Some(site) = target.strip_prefix("外站:") {
            return Some(ResolvedLink::Link(
                LinkTarget::new(format!("https://example.org/wiki/{site}")).attr("rel", "external"),
            ));
        }

        self.existing.contains(target).then(|| {
            ResolvedLink::Link(
                LinkTarget::new(format!("/notes/{target}")).attr("data-page", target),
            )
        })
    }
}

/// 一个「谁都不认」的解析器
#[derive(Debug)]
struct NoVault;

impl LinkResolver for NoVault {
    fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
        None
    }
}

/// 递 `Node` 的那一种：目标没有 `href`，但**仍然是个链接**（人会点它），
/// 只是「点它会发生什么」由使用方决定。
struct RedLink;

impl NodeRenderer for RedLink {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("a", &[("class", "wikilink wikilink--missing".to_string())]);
        fmt.contents(&node.children);
        fmt.text("（还没写）");
        fmt.close("a");
    }
}

/// 递了它自己那份红链的实现：库的默认形态长什么样、我的能不能换掉
#[derive(Debug)]
struct DrawsItsOwnRedLink;

impl LinkResolver for DrawsItsOwnRedLink {
    fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
        Some(ResolvedLink::Node(Box::new(RedLink)))
    }
}

fn render_with(resolver: impl LinkResolver + 'static, src: &str) -> String {
    let resolver: Arc<dyn LinkResolver> = Arc::new(resolver);
    Mdpp::new().render_with(src, &RenderContext::new().with_links(resolver))
}

fn main() {
    let source = "[[已经写了的那页]]、[[还没写的那页]]、[[外站:平陆运河]]\n";

    let mut existing = HashSet::new();
    existing.insert("已经写了的那页".to_string());

    println!("== 递了「哪些页存在」的解析器 ==\n");
    println!("{}", render_with(FakeVault { existing }, source));

    println!("\n== 递了一个谁都不认的解析器 —— 与下面那句**完全一样** ==\n");
    println!("{}", render_with(NoVault, source));

    println!("\n== 压根没递解析器 ==\n");
    println!("{}", Mdpp::new().render(source));

    println!("\n== 想换掉红链的样子：递 Node，自己画 ==\n");
    println!("{}", render_with(DrawsItsOwnRedLink, "[[随便一页]]\n"));

    println!("\n== 想让它看起来正常：给一个能问出地址的解析器 ==\n");
    println!(
        "{}",
        render_with(
            FakeVault {
                existing: HashSet::from(["随便一页".to_string()])
            },
            "[[随便一页]]\n"
        )
    );
}
