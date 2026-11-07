//! 解析器本体：`Mdpp` 与它的构建器。
//!
//! ## 构建一次，渲染多次
//!
//! [`Mdpp`] 持有的是一个构建好的 markdown-it。构建要装十几条规则、跑若干遍
//! `plugins::*::add`，那是一次性的活；[`Mdpp::render`] 只取 `&self`，可以反复调。
//!
//! ## 并发
//!
//! **`Mdpp` 是 `Send + Sync`**：解析器与注册表都在 [`Arc`] 里，构建完之后只读；
//! 渲染状态（链接解析器、模板页、资源收集器那一趟）是每线程一份，见
//! `state` 模块。一个 `Mdpp` 可以被多个线程共享，每页各渲各的。
//!
//! 这不是靠 `unsafe impl` 换来的：markdown-it 的 `MarkdownIt`、本库的
//! `BlockRegistry` 与 `RenderContext` 都满足 `Send + Sync`，编译器会替我们验。
//!
//! ## 默认值：几处刻意的关掉
//!
//! - **raw HTML**：不注册 `plugins::html`。渲染出来的 HTML 常常被直接注入页面，
//!   开着它等于把 XSS 入口交给正文作者。
//! - **缩进代码块**：缩进在本库里有别的用途 —— 它是 `::` 块的边界，两者会互相干扰。
//!   代码块请用围栏写法。
//! - **typographer / smartquotes**：它们会把 `--accent` 变成 `–accent`、把 `"引号"`
//!   变成 `“引号”` —— 在正文里是锦上添花，在代码片段里是**改坏内容**，而这两条规则
//!   不区分代码片段。
//! - **syntect**（随 `plugins::extra` 的默认 features 进来）：它把高亮配色以内联
//!   style 写死，跟随不了主题。
//! - **下划线档的强调**（`_斜_` / `__粗__`）：这两个记号在 mdpp 里改行当了 ——
//!   分别是下标与下划线，另有 `^…^` 上标（[`crate::syntax::baseline`]）；
//!   星号那对原封不动，还是 CommonMark 的全部活性。

mod builder;
mod render;
mod rules;

pub use self::builder::MdppBuilder;

use std::sync::Arc;

use markdown_it::{MarkdownIt, Node};

use crate::assets::{Asset, AssetMode};
use crate::context::RenderContext;
use crate::parse::markdown::parse_markdown;
use crate::registry::BlockRegistry;

use self::render::{render_node_parts, render_parts};

/// 当次渲染里再渲染一段 markdown —— **沿用同一份上下文与注册表**。
///
/// 不在渲染中时返回空串（而不是 panic）：净化器可以被单独调用，那一次没有
/// 「当前渲染」可言。
pub fn render_nested(source: &str) -> String {
    // 取出来再调：里面那次渲染会重新装一份自己的状态，不能占着借用
    let taken = crate::state::with(|state| state.nested.take()).flatten();
    let out = match &taken {
        Some(run) => run(source),
        None => String::new(),
    };
    if let Some(run) = taken {
        crate::state::with(|state| state.nested = Some(run));
    }
    out
}

/// 一个构建好的 mdpp 解析器。
///
/// 建它要花一点工夫（十几条规则），渲染是廉价的。
///
/// **`Send + Sync`**：解析器与注册表都在 [`Arc`] 里，渲染状态每线程一份，
/// 所以一个 `Mdpp` 可以被多个线程共享。
#[derive(Debug)]
pub struct Mdpp {
    /// 用 `Arc` 而不是直接持有：嵌套渲染要把「用哪个解析器」记进线程局部，而那个闭包
    /// 必须是 `'static` 的 —— 抓住 `&self` 编译不过；`Arc` 也让 `Mdpp` 能跨线程共享。
    parser: Arc<MarkdownIt>,
    registry: Arc<BlockRegistry>,
}

impl Default for Mdpp {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Mdpp {
    /// 出一个构建器
    pub fn builder() -> MdppBuilder {
        MdppBuilder::new()
    }

    /// 按默认设置建一个（命令行与「先试一下」够用）
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// 这一份的块注册表
    pub fn registry(&self) -> &BlockRegistry {
        &self.registry
    }

    /// 渲染一段 markdown，用默认上下文：没有链接解析器、没有模板页，于是所有 `[[…]]`
    /// 落成红链，所有非内置的 `::名字` 落成「未知块」框。
    ///
    /// `::css` / `::js` / `::html js` 走**单页**模式：同内容按哈希去重后内联。
    pub fn render(&self, source: &str) -> String {
        self.render_with(source, &RenderContext::new())
    }

    /// 带上下文渲染。这是**带上下文**的那一条：链接解析器与模板页来源都在
    /// [`RenderContext`] 里。两者进入时装上、返回时还原，所以本方法可以递归调用
    /// （`::html` 里的 `<markdown>` 就是这么做的），外层那一趟不受影响。
    ///
    /// `::css` / `::js` / `::html js` 走**单页**模式：同内容按哈希去重后内联。
    pub fn render_with(&self, source: &str, context: &RenderContext) -> String {
        render_parts(
            &self.parser,
            &self.registry,
            context,
            source,
            AssetMode::Inline,
        )
        .0
    }

    /// 站点模式渲染：`::css` / `::js` / `::html js` 不内联，而是按 `asset_prefix`
    /// 生成 `<link>` / `<script src>`，内容随 [`Rendered::assets`] 一起返回。
    ///
    /// `asset_prefix` 是**相对这一页**的前缀（如 `assets` 或 `../../assets`），
    /// 由调用方按页面深度算好传进来 —— 库不知道页面在站点的哪一层。
    pub fn render_with_assets(
        &self,
        source: &str,
        context: &RenderContext,
        asset_prefix: &str,
    ) -> Rendered {
        let (html, assets) = render_parts(
            &self.parser,
            &self.registry,
            context,
            source,
            AssetMode::Linked {
                prefix: asset_prefix.to_string(),
            },
        );
        Rendered { html, assets }
    }

    /// 解析成 markdown-it 的 AST，**不渲染**。
    ///
    /// 返回的是 [`crate::markdown_it::Node`]（本库重导出了 `markdown_it`），
    /// 可以用它的 `children` / `attrs` / `walk` 做二次加工，再交给
    /// [`Self::render_node`]。注意：AST 里的 `::` 块要到渲染时才查注册表，
    /// 所以「解析用哪个 `Mdpp`」与「渲染用哪个 `Mdpp`」应当一致。
    ///
    /// 这是给「不止填空」的扩展用的：例如从当前子树生成目录、按标题重排、
    /// 把一段正文塞进自定义结构。普通渲染不需要它。
    pub fn parse(&self, source: &str) -> Node {
        self.parse_with(source, &RenderContext::new())
    }

    /// 带上下文解析成 AST。链接解析器与模板页来源在解析阶段就会被用到
    /// （`[[…]]` 的节点、`::模板` 的展开）。
    pub fn parse_with(&self, source: &str, context: &RenderContext) -> Node {
        let state = crate::state::RenderState {
            context: Some(context.clone()),
            registry: Some(Arc::clone(&self.registry)),
            // 解析阶段不收集资源；只有渲染才会碰到 ::css / ::js
            assets: None,
            nested: None,
            theme_slot: None,
            depth: 0,
        };
        let _guard = crate::state::enter(state);
        parse_markdown(&self.parser, source)
    }

    /// 渲染一棵已经解析好的 AST（单页模式，资源内联）。
    pub fn render_node(&self, node: &Node, context: &RenderContext) -> String {
        render_node_parts(
            &self.parser,
            &self.registry,
            node,
            context,
            AssetMode::Inline,
        )
        .0
    }

    /// 渲染一棵已经解析好的 AST（站点模式，资源按 `asset_prefix` 外链）。
    pub fn render_node_with_assets(
        &self,
        node: &Node,
        context: &RenderContext,
        asset_prefix: &str,
    ) -> Rendered {
        let (html, assets) = render_node_parts(
            &self.parser,
            &self.registry,
            node,
            context,
            AssetMode::Linked {
                prefix: asset_prefix.to_string(),
            },
        );
        Rendered { html, assets }
    }
}

/// 一次渲染的产物：HTML 与收集到的资源。
///
/// 单页模式（[`Mdpp::render`]）只有 HTML，资源已经内联进去；
/// 站点模式（[`Mdpp::render_with_assets`]）的 HTML 里是链接，内容在这里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// 渲染好的 HTML
    pub html: String,
    /// 收集到的 CSS / JS 资源（已按内容去重）
    pub assets: Vec<Asset>,
}

#[cfg(test)]
mod tests;
