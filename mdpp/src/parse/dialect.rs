//! 解析器方言的唯一装配点。
//!
//! 同一个 markdown-it 有两种用法，规则集必须一致：
//!
//! - [`Mode::Render`]：真正渲染正文的那个解析器；
//! - [`Mode::Scan`]：只找「原始区」的探针。它要认出 `::code` / `::css` 这类
//!   [`ContentMode::Raw`](crate::registry::ContentMode) 块，否则块正文里的
//!   `[链接](a b.png)` 会在预扫描阶段被改写。
//!
//! 扫描模式**不展开模板、不解析块正文**：`ScanBlockScanner` 只量出 Raw 块的
//! 字节区间，把它们作为 [`RawBlock`] 节点挂进语法树。这样探针不必知道模板页，
//! 也不会递归。
//!
//! 两个模式共用 [`SyntaxOptions`] 与同一份 `::` 规则挂载逻辑；扫描模式刻意不摘
//! setext / 缩进代码块，保持「原始区判定」与 CommonMark 一致。

use std::ops::Range;
use std::sync::LazyLock;

use markdown_it::generics::inline::emph_pair::EmphPairScanner;
use markdown_it::plugins::cmark::block::code::CodeScanner;
use markdown_it::plugins::cmark::block::lheading::LHeadingScanner;
use markdown_it::plugins::extra::smartquotes::SmartQuotesRule;
use markdown_it::plugins::extra::tables::TableScanner;
use markdown_it::plugins::extra::typographer::TypographerRule;
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

use crate::block::registry::{BlockRegistry, BlockSpec};
use crate::runtime::engine::rules::{MarkExternalLinks, UniqueHeadingAnchors};
use crate::syntax;

/// 解析器要用哪一种规则集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// 真正渲染：全部块/行内规则都在。
    Render,
    /// 只找原始区：不装模板展开、不装渲染期插件。
    Scan,
}

/// 那几条可开关的语法。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SyntaxOptions {
    pub(crate) math: bool,
    pub(crate) wikilink: bool,
    pub(crate) tables: bool,
    pub(crate) heading_anchors: bool,
    pub(crate) raw_html: bool,
    pub(crate) setext: bool,
    pub(crate) indented_code: bool,
    pub(crate) typographer: bool,
}

impl Default for SyntaxOptions {
    fn default() -> Self {
        Self {
            math: true,
            wikilink: true,
            tables: true,
            heading_anchors: true,
            // 四个「关掉」，理由见 `engine` 模块头
            raw_html: false,
            setext: false,
            indented_code: false,
            typographer: false,
        }
    }
}

/// 建一个解析器。
pub(crate) fn build(mode: Mode, options: &SyntaxOptions) -> MarkdownIt {
    let mut md = MarkdownIt::new();
    // markdown-it 默认是个空解析器：CommonMark 本身也是插件，必须显式注册。
    markdown_it::plugins::cmark::add(&mut md);

    match mode {
        Mode::Scan => {
            // HTML 块与行内代码也要进原始区；扫描模式不渲染，开着 HTML 插件
            // 只是让判定与 CommonMark 一致。
            markdown_it::plugins::html::add(&mut md);
            crate::block::add_scan(&mut md);
        }
        Mode::Render => {
            if !options.setext {
                md.block.remove_rule::<LHeadingScanner>();
            }
            if !options.indented_code {
                md.block.remove_rule::<CodeScanner>();
            }

            markdown_it::plugins::extra::add(&mut md);
            if !options.tables {
                md.block.remove_rule::<TableScanner>();
            }
            if !options.typographer {
                md.remove_rule::<TypographerRule>();
                md.remove_rule::<SmartQuotesRule<'\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}'>>();
            }
            if options.raw_html {
                // 安全开关：正文 HTML 原样透传，净化责任在调用方
                markdown_it::plugins::html::add(&mut md);
            }

            if options.heading_anchors {
                md.add_rule::<UniqueHeadingAnchors>();
            }
            md.add_rule::<MarkExternalLinks>();

            if options.math {
                syntax::math::add(&mut md);
            }
            if options.wikilink {
                syntax::wikilink::add(&mut md);
            }
            syntax::mark::add(&mut md);

            // `__` / `_` 整体退役为下划线/下标，见 `syntax::baseline` 的模块头
            md.inline.remove_rule::<EmphPairScanner<'_', false>>();
            syntax::baseline::add(&mut md);

            // `::` 块：必须排在段落规则之前
            crate::block::add(&mut md);
        }
    }

    md
}

/// 扫描方言的解析器（进程内只建一次）。
pub(crate) fn scan_parser() -> &'static MarkdownIt {
    static SCAN: LazyLock<MarkdownIt> =
        LazyLock::new(|| build(Mode::Scan, &SyntaxOptions::default()));
    &SCAN
}

/// 扫描模式里「这里是一段 Raw 块」的记号。
///
/// 节点不参与渲染，只给 [`crate::raw::raw_zones`] 读区间用。
#[derive(Debug)]
pub(crate) struct RawBlock {
    pub(crate) range: Range<usize>,
}

impl NodeValue for RawBlock {
    fn render(&self, _node: &Node, _fmt: &mut dyn Renderer) {
        // 扫描结果永远不进渲染；真走到这里说明用错了，但也不该 panic
    }
}

/// 查一块的注册项：当前渲染的注册表优先，不在渲染里就用内置表。
///
/// 扫描器只关心 [`ContentMode::Raw`](crate::registry::ContentMode)，内置表足以
/// 覆盖 `::code` / `::css` 这些；自定义 Raw 块在它自己的 `Mdpp` 里渲染时，
/// 线程局部注册表里查得到。
pub(crate) fn spec_for(name: &str) -> Option<BlockSpec> {
    if let Some(spec) = crate::block::registry::lookup(name) {
        return Some(spec);
    }
    static BUILTIN: LazyLock<BlockRegistry> = LazyLock::new(BlockRegistry::builtin);
    BUILTIN.get(name).cloned()
}
