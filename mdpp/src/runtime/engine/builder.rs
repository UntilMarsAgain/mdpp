use std::sync::Arc;

use markdown_it::MarkdownIt;
use markdown_it::generics::inline::emph_pair::EmphPairScanner;

use crate::block;
use crate::registry::BlockRegistry;
use crate::syntax;
use crate::telemetry::debug;

use super::Mdpp;
use super::rules::{MarkExternalLinks, UniqueHeadingAnchors};

/// [`Mdpp`] 的构建器。
///
/// 默认值见模块头；每一项都能单独改。
#[derive(Debug)]
pub struct MdppBuilder {
    registry: Option<BlockRegistry>,
    syntax: SyntaxOptions,
}

/// 那几条可开关的语法。
#[derive(Debug, Clone, Copy)]
struct SyntaxOptions {
    math: bool,
    wikilink: bool,
    tables: bool,
    heading_anchors: bool,
    raw_html: bool,
    setext: bool,
    indented_code: bool,
    typographer: bool,
}

impl Default for SyntaxOptions {
    fn default() -> Self {
        Self {
            math: true,
            wikilink: true,
            tables: true,
            heading_anchors: true,
            // 四个「关掉」，理由在模块头
            raw_html: false,
            setext: false,
            indented_code: false,
            typographer: false,
        }
    }
}

impl Default for MdppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MdppBuilder {
    /// 默认设置
    pub fn new() -> Self {
        Self {
            registry: None,
            syntax: SyntaxOptions::default(),
        }
    }

    /// 换掉块注册表（默认是 [`BlockRegistry::builtin`]）。
    ///
    /// 想「内置的 + 我自己的」就从 [`BlockRegistry::with_builtins`] 开始灌。
    pub fn registry(mut self, registry: BlockRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    /// 行内数学公式 `$…$`（默认开）
    pub fn math(mut self, on: bool) -> Self {
        self.syntax.math = on;
        self
    }

    /// 内部链接 `[[…]]`（默认开）
    pub fn wikilink(mut self, on: bool) -> Self {
        self.syntax.wikilink = on;
        self
    }

    /// 表格（默认开）
    pub fn tables(mut self, on: bool) -> Self {
        self.syntax.tables = on;
        self
    }

    /// raw HTML：正文里的 HTML 原样透传。默认**关**，理由在模块头。要在自己那套
    /// 已经做过净化的管线里用它，才打开。
    pub fn raw_html(mut self, on: bool) -> Self {
        self.syntax.raw_html = on;
        self
    }

    /// 给标题加 `id`（默认开）。关掉之后 `[跳转](#标题)` 就少了个落点。
    pub fn heading_anchors(mut self, on: bool) -> Self {
        self.syntax.heading_anchors = on;
        self
    }

    /// Setext 标题（`标题` 下一行写 `===` 或 `---`）。默认**关**。
    ///
    /// 理由是它和 `---`（thematic break）撞：摘掉之后 `标题\n---` 老老实实退化成
    /// 「段落 + 分隔线」，开着的时候它是标题 —— 一段写错了的正文会变成另一种意思。
    pub fn setext(mut self, on: bool) -> Self {
        self.syntax.setext = on;
        self
    }

    /// 缩进代码块（四空格开头）。默认**关**。理由见模块头。
    pub fn indented_code(mut self, on: bool) -> Self {
        self.syntax.indented_code = on;
        self
    }

    /// 排版替换（`--` → `–`、直引号 → 弯引号）。默认**关**。理由见模块头。
    pub fn typographer(mut self, on: bool) -> Self {
        self.syntax.typographer = on;
        self
    }

    /// 建出来
    pub fn build(self) -> Mdpp {
        let options = self.syntax;
        let registry = Arc::new(self.registry.unwrap_or_else(BlockRegistry::builtin));

        // markdown-it 默认是个**空解析器**：CommonMark 语法本身也是插件，
        // 必须显式注册。
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);

        if options.setext {
            // 留着什么都不用做：`cmark::add` 已经把它装上了
        } else {
            // 块级规则在另一个 ruler 里，所以摘的是 `md.block` 而不是 `md.remove_rule`
            md.block
                .remove_rule::<markdown_it::plugins::cmark::block::lheading::LHeadingScanner>();
        }

        if !options.indented_code {
            md.block
                .remove_rule::<markdown_it::plugins::cmark::block::code::CodeScanner>();
        }

        markdown_it::plugins::extra::add(&mut md);

        // `extra::add` 是一整套，接着按开关把不要的摘掉。
        //
        // 摘法：`Ruler::remove_rule` 按**规则类型**摘。markdown-it 0.6 只有一列规则
        //（没有老版本的 `ruler2`），块级走 `md.block`、行内走 `md.inline`。
        if !options.tables {
            md.block
                .remove_rule::<markdown_it::plugins::extra::tables::TableScanner>();
        }
        if !options.typographer {
            md.remove_rule::<markdown_it::plugins::extra::typographer::TypographerRule>();
            md.remove_rule::<markdown_it::plugins::extra::smartquotes::SmartQuotesRule<
                '\u{2018}',
                '\u{2019}',
                '\u{201c}',
                '\u{201d}',
            >>();
        }
        if options.raw_html {
            // 这一条是**安全开关**：正文里的 HTML 原样透传，净化就全在调用方身上了
            markdown_it::plugins::html::add(&mut md);
        }

        if options.heading_anchors {
            // 自己挂锚点，而不是 `heading_anchors` 插件：插件每次 `md.parse` 都
            // 无条件 `push` 一个 `id`，嵌套解析搬进来的子树会被外层再挂一遍；
            // 这里一条规则走完整棵树，按出现次序给 slug 加计数，天然唯一。
            md.add_rule::<UniqueHeadingAnchors>();
        }

        // 外部链接标记：给带协议的站外链接加一个 `external` 类，样式表据此
        // 画「外链」符号。与 heading_anchors 无关，无条件挂。
        md.add_rule::<MarkExternalLinks>();

        if options.math {
            syntax::math::add(&mut md);
        }
        if options.wikilink {
            syntax::wikilink::add(&mut md);
        }
        // 行内高亮没有开关 —— 一条随库的基础语法，跟删除线一个待遇
        syntax::mark::add(&mut md);

        // `__` 与 `_` 从 CommonMark 的强调档整体退役：同一对记号分别是下划线与
        // 下标，`^…^` 补上标（那把「贴字」的尺与代价见 syntax::baseline 的模块
        // 头）。摘的是下划线那一个扫描器 —— 一条规则管着两个长度的配对，星号
        // 的机器不受牵动。
        md.inline.remove_rule::<EmphPairScanner<'_', false>>();
        syntax::baseline::add(&mut md);

        // `::` 块。挂载位置是刻意的：必须排在段落规则**之前**，否则 `::名字`
        // 会先被当成一行普通文字收走。
        block::add(&mut md);

        // 建好之后报一句「这一份解析器带了什么」—— 排查「为什么这个块没生效」时，
        debug!(
            blocks = registry.len(),
            names = ?registry.names(),
            "解析器已建好"
        );

        Mdpp {
            parser: Arc::new(md),
            registry,
        }
    }
}
