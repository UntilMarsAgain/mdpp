// mdpp —— Markdown++
//
// 把 markdown-it 之上的那套「块级方言」（`::名字 参数=值` 开块，缩进划边界）整理成
// 能被别人接手的东西。
//
// 三条设计线索，全文都能对上：
//
// 1. **这一层不知道外面有什么。** 目标存不存在、模板页正文从哪来、正文写到哪去，一概
//    不问 —— 那些由调用方通过 trait 递进来（[`link::LinkResolver`]、
//    [`block::template::TemplateSource`]）。库里每个「需要知道外部」的地方都是一个
//    trait 方法，没有一个是全局函数。
//
//    那两个 trait 上**一个实现都不带**：给一个现成的实现（一张 `HashMap`、一个文件
//    目录）就等于把「东西存在哪」定成默认答案。要找现成的例子看 `mdpp-cli` —— 它那
//    两份实现各只有十几行。
//
//    同一条道理延伸到**呈现**：本库不带样式表、不带脚本、不套整页，只给一段 HTML
//    片段。官方那套 CSS/JS 在 `mdpp-cli/assets/`（同一个 MIT 许可，可自取）。
// 2. **要能扩展，且扩展不 fork。** `::` 语法由一张注册表驱动（[`registry`]），注册表
//    是**普通数据**：外部构造自己那份，灌进 [`MdppBuilder::registry`] 即可。
// 3. **渲染状态是每次调用各自一份。** 理由写在 [`context`]。
//
// ## 一分钟上手
//
//! ```
//! use mdpp::Mdpp;
//!
//! let mdpp = Mdpp::new();
//! let html = mdpp.render("::note title=\"提醒\"\n  这里可以写 **markdown**。\n");
//! assert!(html.contains(r#"class="callout callout--note""#));
//! assert!(html.contains("<strong>markdown</strong>"));
//!
//! // 内部链接交给自己的解析器
//! use mdpp::{link::{LinkResolver, LinkTarget, ResolvedLink}, RenderContext};
//! use std::sync::Arc;
//!
//! struct Mine;
//! impl LinkResolver for Mine {
//!     fn resolve(&self, target: &str) -> Option<ResolvedLink> {
//!         Some(ResolvedLink::Link(LinkTarget::new(format!("/notes/{target}"))))
//!     }
//! }
//!
//! let html = mdpp.render_with("[[另一篇]]", &RenderContext::new().with_links(Arc::new(Mine)));
//! assert!(html.contains(r#"href="/notes/另一篇""#));
//! ```
//!
//! 注册自己的一块：
//!
//! ```
//! use mdpp::registry::{BlockRegistry, ContentMode};
//! use mdpp::Mdpp;
//!
//! let mut registry = BlockRegistry::with_builtins();
//! registry.register_fn("问候", ContentMode::Markdown, |block, _node, fmt| {
//!     fmt.text(&format!("你好，{}", block.param("who").unwrap_or("世界")));
//! });
//!
//! let html = Mdpp::builder().registry(registry).build().render("::问候 who=某人\n");
//! assert!(html.contains("你好，某人"));
//! ```

#![deny(missing_docs)]

/// 把仓库根的 README 里的 ```rust 段当 doctest 跑一遍。
///
/// 没有这一条时 README 的代码从没被编译过 —— `MyPages` 那个元组结构体被当构造器
/// 传进 `Arc::new` 的错误就是这么活下来的。`cfg(doctest)` 让它只存在于测试里，
/// 不进真实的文档。
#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
pub struct ReadmeSnippets;

pub mod block;
pub mod context;
pub mod engine;
pub mod link;
pub mod raw;
pub mod registry;
pub mod slug;
pub mod syntax;
pub mod targets;
pub mod telemetry;

/// 重导出：写块渲染器要用的 `Node` / `Renderer` / `NodeValue` 就在这里，
/// 使用方不必自己往 `Cargo.toml` 里加一条 `markdown-it` 就能对上版本。
pub use markdown_it;

pub use block::template::{TemplatePage, TemplateSource};
pub use context::RenderContext;
pub use engine::{Mdpp, MdppBuilder};
pub use link::{LinkResolver, LinkTarget, NodeRenderer, ResolvedLink};
pub use registry::{Block, BlockRegistry, BlockSpec, ContentMode};

/// 这一版带了的现成块名（诊断与命令行帮助都用它）。
pub fn builtin_blocks() -> Vec<&'static str> {
    block::stdlib::names()
}
