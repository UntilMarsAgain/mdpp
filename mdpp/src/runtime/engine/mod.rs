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
//! `env` 模块。一个 `Mdpp` 可以被多个线程共享，每页各渲各的。
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

/// 渲染期嵌套（`::demo`、`<markdown>`、`::navbox` 的单元格）最多套这么多层。
///
/// 它与模板展开的 `MAX_TEMPLATE_DEPTH` 分开：模板展开数的是「模板套模板」，
/// 这里数的是「渲染里再渲染」。两者都不数，`::demo` 里套一个会展开自己的模板
/// 就能绕过模板深度保护，把进程栈打穿。
const MAX_RENDER_DEPTH: usize = 16;

/// 当次渲染里再渲染一段 markdown —— **沿用同一份上下文与注册表**。
///
/// 不在渲染中时返回空串（而不是 panic）：净化器可以被单独调用，那一次没有
/// 「当前渲染」可言。
pub fn render_nested(source: &str) -> String {
    // 取出来再调：里面那次渲染会重新装一份自己的状态，不能占着借用
    let taken = crate::env::with(|env| env.nested.take()).flatten();
    let Some(run) = taken else {
        return String::new();
    };

    let exceeded = crate::env::with(|env| {
        if env.render_depth >= MAX_RENDER_DEPTH {
            true
        } else {
            env.render_depth += 1;
            false
        }
    })
    .unwrap_or(false);

    let out = if exceeded {
        format!(
            "<div class=\"mdpp-template mdpp-template--problem\"><p class=\"mdpp-template__why\">\
             嵌套渲染超过 {MAX_RENDER_DEPTH} 层（是不是自己嵌自己？）</p></div>"
        )
    } else {
        let out = run(source);
        // 只有真的进过一层才退一层；超限那一次没有加，不能把外层的数也扣掉
        crate::env::with(|env| env.render_depth = env.render_depth.saturating_sub(1));
        out
    };

    crate::env::with(|env| env.nested = Some(run));
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

    /// 渲染一段 markdown，用默认上下文：没有链接解析器，标准库模板可用；所有 `[[…]]`
    /// 落成红链，标准库之外的非内置 `::名字` 落成「未知块」框。
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
        self.render_report(source, context).html
    }

    /// 单页模式渲染，但把结构信息一起返回：标题、特性、问题计数。
    ///
    /// 与 [`Self::render_with`] 的 HTML 完全一样，只是多一份报告。站点模式用
    /// [`Self::render_with_assets`]；这条给需要整页外壳的单文件模式用。
    pub fn render_report(&self, source: &str, context: &RenderContext) -> Rendered {
        let (html, assets, report) = render_parts(
            &self.parser,
            &self.registry,
            context,
            source,
            AssetMode::Inline,
        );
        Rendered::new(html, assets, report)
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
        let (html, assets, report) = render_parts(
            &self.parser,
            &self.registry,
            context,
            source,
            AssetMode::Linked {
                prefix: asset_prefix.to_string(),
            },
        );
        Rendered::new(html, assets, report)
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
        let env = crate::env::RenderEnv {
            context: context.clone(),
            registry: Arc::clone(&self.registry),
            // 解析阶段不收集资源；只有渲染才会碰到 ::css / ::js
            assets: crate::runtime::asset::Sink::default(),
            nested: None,
            variant: None,
            variant_seq: crate::env::sequence(),
            depth: 0,
            template_vars: None,
            render_depth: 0,
            features: Features::default(),
            problems: Problems::default(),
        };
        let _guard = crate::env::enter(env);
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
        let (html, assets, report) = render_node_parts(
            &self.parser,
            &self.registry,
            node,
            context,
            AssetMode::Linked {
                prefix: asset_prefix.to_string(),
            },
        );
        Rendered::new(html, assets, report)
    }
}

/// 这一页用到的、需要外部前端资源才能运转的东西。
///
/// 它由**渲染出来的结构**决定，而不是数 HTML 里出现了哪个类名：`::code lang=rust`
/// 与 ` ```rust ` 都算高亮，`::mermaid` 算图，行内/块级公式算数学。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Features {
    /// 有没有带语言名的代码块（需要 highlight.js）
    pub highlight: bool,
    /// 有没有公式（需要 KaTeX）
    pub math: bool,
    /// 有没有 mermaid 图（需要 mermaid.js）
    pub mermaid: bool,
}

impl Features {
    pub(crate) fn merge(&mut self, other: Self) {
        self.highlight |= other.highlight;
        self.math |= other.math;
        self.mermaid |= other.mermaid;
    }
}

/// 渲染结果里的一个标题。
///
/// `text` 是标题的纯文本（行内标记已经拿掉），`id` 是标题锚点；关掉
/// `heading_anchors` 时没有 `id`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 标题级别，1–6
    pub level: u8,
    /// 标题文字
    pub text: String,
    /// 标题锚点
    pub id: Option<String>,
}

/// 渲染里出现的「问题框」计数。
///
/// 与从 HTML 里数类名不同，这个计数发生在渲染那一处：围栏代码或正文里恰好写着
/// `template--problem` 不会被算进来，嵌套渲染里的问题也不会漏。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Problems {
    /// 没注册的块
    pub unknown: usize,
    /// 名字认识、但参数或内容用法不对的块
    pub misused: usize,
}

impl Problems {
    pub(crate) fn merge(&mut self, other: Self) {
        self.unknown += other.unknown;
        self.misused += other.misused;
    }
}

/// 一次渲染的产物：HTML、资源，以及供页面外壳使用的结构信息。
///
/// 单页模式（[`Mdpp::render`]）只有 HTML，资源已经内联进去；
/// 站点模式（[`Mdpp::render_with_assets`]）的 HTML 里是链接，内容在这里。
///
/// `outline` / `features` / `problems` 都是渲染时顺手记下来的：调用方不必再拿
/// HTML 做字符串匹配。嵌套渲染的 features/problems 会合并进来，标题只取这一份
/// 文档自己的 AST。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// 渲染好的 HTML
    pub html: String,
    /// 收集到的 CSS / JS 资源（已按内容去重）
    pub assets: Vec<Asset>,
    /// 文档大纲，按标题出现顺序；不含嵌套模板页自己的标题
    pub outline: Vec<Heading>,
    /// 需要外部前端资源的特性
    pub features: Features,
    /// 问题框计数
    pub problems: Problems,
}

impl Rendered {
    /// 第一个一级标题的文字。没有一级标题时返回 `None`
    pub fn title(&self) -> Option<&str> {
        self.outline
            .iter()
            .find(|heading| heading.level == 1 && !heading.text.is_empty())
            .map(|heading| heading.text.as_str())
    }

    pub(super) fn new(html: String, assets: Vec<Asset>, report: RenderReport) -> Self {
        Self {
            html,
            assets,
            outline: report.outline,
            features: report.features,
            problems: report.problems,
        }
    }
}

/// `render_parts` 的返回值里除 HTML/资源外的那一份。
#[derive(Debug, Default)]
pub(super) struct RenderReport {
    pub(super) outline: Vec<Heading>,
    pub(super) features: Features,
    pub(super) problems: Problems,
}

#[cfg(test)]
mod tests;
