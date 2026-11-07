//! 解析器本体：`Mdpp` 与它的构建器。
//!
//! ## 构建一次，渲染多次
//!
//! [`Mdpp`] 持有的是一个构建好的 markdown-it。构建要装十几条规则、跑若干遍
//! `plugins::*::add`，那是一次性的活；[`Mdpp::render`] 只取 `&self`，可以反复调。
//!
//! ## 并发
//!
//! **`Mdpp` 既不是 `Send` 也不是 `Sync`。** markdown-it 的 `ext` 集合是 `RefCell`，
//! 而 `parse` 期间真的读写它（围栏代码块的语言前缀、标题的 slug 函数都从那里取），
//! 所以两个线程共用一个解析器是**数据竞争**，不是「慢一点」。
//!
//! 定成**一个线程建一个**：建一次是微秒级的事，比外面套一把锁划算，也比给一个
//! `!Sync` 的东西硬加 `unsafe impl Sync` 划算。
//!
//! **渲染状态**（链接解析器、模板页、注册表那一趟）是线程局部的，各线程各渲各的
//! 互不干扰 —— 由 [`crate::context`] 与 [`crate::syntax::wikilink`] 的测试钉着。
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

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use markdown_it::generics::inline::emph_pair::EmphPairScanner;
use markdown_it::parser::core::CoreRule;
use markdown_it::plugins::cmark::inline::autolink::Autolink;
use markdown_it::plugins::cmark::inline::link::Link;
use markdown_it::plugins::extra::linkify::Linkified;
use markdown_it::{MarkdownIt, Node};

use crate::assets::{self, Asset, AssetMode};
use crate::block;
use crate::context::{self, RenderContext};
use crate::registry::BlockRegistry;
use crate::slug;
use crate::syntax;
use crate::targets;
use crate::telemetry::{Size, debug, trace};

/// 「在渲染途中再渲染一段 markdown」的能力，即 `::html` 里的 `<markdown>`。
///
/// 走线程局部里的闭包而不是一个全局解析器：一个进程里可以有多个 [`Mdpp`]，而嵌套
/// 渲染要用的是**当次那一个**。「用哪个」在 [`Mdpp::render_with`] 入口处记下来，
/// 出栈时还原。
type Nested = Box<dyn Fn(&str) -> String>;

thread_local! {
    /// 当次渲染的「再渲染一段」能力
    static NESTED: RefCell<Option<Nested>> = const { RefCell::new(None) };
}

/// 当次渲染里再渲染一段 markdown —— **沿用同一份上下文与注册表**。
///
/// 不在渲染中时返回空串（而不是 panic）：净化器可以被单独调用，那一次没有
/// 「当前渲染」可言。
pub fn render_nested(source: &str) -> String {
    // 取出来再调，不能拿着 `borrow()` 调：里面那次渲染会重新装上自己的，
    // 而那一步要 `borrow_mut()`。
    let taken = NESTED.with(|cell| cell.borrow_mut().take());
    let out = match &taken {
        Some(run) => run(source),
        None => String::new(),
    };
    if let Some(run) = taken {
        NESTED.with(|cell| *cell.borrow_mut() = Some(run));
    }
    out
}

/// 装上「再渲染一段」的能力，返回出栈时还原的守卫。
struct NestedGuard {
    previous: Option<Nested>,
}

impl Drop for NestedGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        NESTED.with(|cell| *cell.borrow_mut() = previous);
    }
}

/// 一个构建好的 mdpp 解析器。
///
/// 建它要花一点工夫（十几条规则），渲染是廉价的。
///
/// **既不是 `Send` 也不是 `Sync`** —— 一个线程一个，见模块头。
#[derive(Debug)]
pub struct Mdpp {
    /// 用 `Rc` 而不是直接持有：嵌套渲染要把「用哪个解析器」记进线程局部，而那个闭包
    /// 必须是 `'static` 的 —— 抓住 `&self` 编译不过。
    ///
    /// [`Rc`] 而不是 [`Arc`] 的理由与「不是 `Sync`」是同一条。
    parser: Rc<MarkdownIt>,
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

/// 真正干活的那一趟。四样东西各自装上、各自还原，全在同一次调用里成对出现。
///
/// 拆成自由函数（而不是留在 `&self` 的方法里）是因为嵌套渲染要在线程局部里存一个
/// `'static` 闭包，而闭包得抓住解析器与上下文的**所有权**（`Rc` / `Arc` 的克隆）。
///
/// 返回值里的资源只在**最外层**有意义；嵌套渲染的资源由 [`assets::Guard`] 合并回
/// 外层，不会丢。
fn render_parts(
    parser: &Rc<MarkdownIt>,
    registry: &BlockRegistry,
    context: &RenderContext,
    source: &str,
    mode: AssetMode,
) -> (String, Vec<Asset>) {
    // 目标里的空格先补成 `%20`（CommonMark 不认带空格的目标，见 `targets`）
    let prepared = targets::encode_target_spaces(source);

    // 顺序无所谓，四个都是各自独立的槽位
    let _registry_guard = crate::registry::enter(Arc::new(registry.clone()));
    let _context_guard = context::enter(context.clone());
    let (assets_guard, nested) = assets::enter(mode.clone());
    // 「再渲染一段」用的是**当次这一趟**的上下文与注册表，而不是新的空的一份
    trace!(
        depth = crate::context::depth(),
        "嵌套渲染：装上再渲染一段的能力"
    );
    let _nested_guard = NestedGuard {
        previous: NESTED.with(|cell| {
            cell.borrow_mut().replace(Box::new({
                let parser = Rc::clone(parser);
                let registry = registry.clone();
                let context = context.clone();
                let mode = mode.clone();
                move |more: &str| render_parts(&parser, &registry, &context, more, mode.clone()).0
            }))
        }),
    };

    let started = std::time::Instant::now();
    let html = parser.parse(&prepared).render();
    let size = Size::measure(&prepared, started.elapsed(), &html);
    debug!(
        in_bytes = size.in_bytes,
        out_bytes = size.out_bytes,
        micros = size.micros,
        "渲染一趟"
    );

    // 嵌套渲染不取资源：留在收集器里，守卫出栈时合并回外层
    let assets = if nested {
        Vec::new()
    } else {
        assets::take_assets()
    };
    drop(assets_guard);
    (html, assets)
}

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
            markdown_it::plugins::extra::heading_anchors::add(&mut md, slug::heading_id);
            // 紧接着把重复的收掉：标题 id 那条规则每跑一次 `md.parse` 都走**整棵
            // 树**，包括子解析（Markdown 档的块、分栏的每一节、模板页展开）搬进来
            // 的孩子 —— 子解析已挂过一次，外层再挂一次，页面上就成了
            // `id="甲" id="甲"`。详见 [`DropDuplicateHeadingIds`]。
            md.add_rule::<DropDuplicateHeadingIds>();
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
            parser: Rc::new(md),
            registry,
        }
    }
}

/// 收掉重复的标题 `id`，**留下先挂的那一个**。
///
/// `heading_anchors` 的 core 规则每跑一次 `md.parse` 都会走整棵树。嵌套解析把
/// 子树的节点搬进外层之后，外层那一遍会再挂一次 —— `::note` 里的标题于是有两个
/// `id`，经模板页展开的语法页有三个。重复的 `id` 是无效 HTML，锚点也会指错。
///
/// 挂完立刻收，顺序就定死在这一处：不依赖渲染时去重，也不会改到别的属性。
struct DropDuplicateHeadingIds;

impl CoreRule for DropDuplicateHeadingIds {
    fn run(root: &mut Node, _md: &MarkdownIt) {
        root.walk_mut(|node, _| {
            let mut kept = false;
            node.attrs.retain(|(name, _)| {
                if *name != "id" {
                    return true;
                }
                // 第一个留下，后面的都丢掉
                !std::mem::replace(&mut kept, true)
            });
        });
    }
}

/// 给**站外链接**挂 `class="external"`，样式表据此画外链符号。
///
/// 认的是 CommonMark 的普通链接（`[文字](地址)`、`<地址>` 自动链接，以及
/// linkify 认出来的裸地址），地址以 `http://`、`https://`、`//` 开头就是站外。
///
/// 不碰 [`crate::syntax::wikilink`] 造的节点：内部链接的目标由解析器决定，
/// 它是站内导航，哪怕解析出来是个绝对地址也不该画「出站」符号。
///
/// 每层 `md.parse` 的 core 阶段都会走整棵树（嵌套块的子解析已挂过一次），
/// 所以加之前先看类里有没有 —— 否则嵌套一深就是 `class="external external"`。
struct MarkExternalLinks;

impl CoreRule for MarkExternalLinks {
    fn run(root: &mut Node, _md: &MarkdownIt) {
        root.walk_mut(|node, _| {
            // 三种链接节点各存各的地址：普通链接、尖括号自动链接、linkify 认出的裸地址。
            // 先克隆出来，后面要改 `attrs`，不能一边借着值一边改节点。
            let url = node
                .cast::<Link>()
                .map(|link| link.url.clone())
                .or_else(|| node.cast::<Autolink>().map(|link| link.url.clone()))
                .or_else(|| node.cast::<Linkified>().map(|link| link.url.clone()));
            let Some(url) = url else {
                return;
            };
            if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//"))
            {
                return;
            }

            if let Some((_, class)) = node.attrs.iter_mut().find(|(name, _)| *name == "class") {
                if !class.split_whitespace().any(|item| item == "external") {
                    class.push_str(" external");
                }
            } else {
                node.attrs.push(("class", "external".to_string()));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Mdpp;
    use crate::registry::BlockRegistry;
    use crate::registry::ContentMode;
    use markdown_it::{Node, Renderer};

    fn md_of(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 兜底：标题、强调这些最基本的记号还认得 —— 开关加了这么多，先看这里
    #[test]
    fn commonmark_is_there() {
        let html = md_of("# 一级\n\n正文里有**强调**。\n");
        assert!(html.contains("<h1"), "{html}");
        assert!(html.contains("<strong>强调</strong>"), "{html}");
    }

    /// 正文里的 HTML 默认转义掉 —— 透传得是调用方自己打开 raw HTML 才有的待遇
    #[test]
    fn raw_html_is_escaped() {
        let html = md_of("<script>alert(1)</script>\n");
        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains("&lt;script"), "{html}");
    }

    /// `javascript:` 那类协议不许落成 `href` —— 链接是点得开的，这一条漏了就是注入入口
    #[test]
    fn dangerous_link_protocols_are_rejected() {
        let html = md_of("[x](javascript:alert(1))\n");
        assert!(!html.contains(r#"href="javascript:"#), "{html}");
    }

    /// 站外链接挂 `external` 类，相对地址与内部链接都不挂 —— 样式表靠这个类画外链符号
    #[test]
    fn external_links_are_marked_but_internal_ones_are_not() {
        let html = md_of("[外](https://example.com/x) [内](../指南/) [[某页]]\n");
        assert!(
            html.contains(r#"<a class="external" href="https://example.com/x">"#),
            "{html}"
        );
        // 全页只有那一条外链带这个类
        assert_eq!(html.matches("external").count(), 1, "{html}");
    }

    /// 三种链接节点（普通链接、尖括号自动链接、裸地址）都认，站内与邮件不认
    #[test]
    fn every_kind_of_external_link_is_marked() {
        let html =
            md_of("[外](https://example.com/x) <https://example.com/y> https://example.com/z\n");
        assert_eq!(html.matches(r#"class="external""#).count(), 3, "{html}");
        // 相对地址、内部链接、邮件都不该带这个类
        let html = md_of("[内](../指南/) [[某页]] [信](mailto:a@b.c)\n");
        assert!(!html.contains("external"), "{html}");
    }

    /// 嵌套块里的外链也只挂一次类：core 规则每层 `md.parse` 都会走整棵树
    #[test]
    fn nested_blocks_do_not_duplicate_the_external_class() {
        let html = md_of("::note\n  [外](https://example.com/x)\n");
        assert_eq!(html.matches(r#"class="external""#).count(), 1, "{html}");
    }

    /// 围栏上的语言名要带进 `class="language-…"` —— 高亮与样式都认这个类
    #[test]
    fn fenced_code_keeps_its_language_class() {
        let html = md_of("```rust\nfn main() {}\n```\n");
        assert!(html.contains(r#"<code class="language-rust">"#), "{html}");
    }

    /// Setext 默认关：`标题` 下一行写 `---` 得退回「段落 + 分隔线」，
    /// 一段写歪的正文不能悄悄变成另一种意思
    #[test]
    fn setext_headings_are_off_by_default() {
        let html = md_of("标题\n===\n");
        assert!(!html.contains("<h1"), "Setext 标题应当已被移除: {html}");
        // 下划线是 `---` 时更要确认：它本身就是 thematic break，
        // 移除 Setext 后应退化成「段落 + 一条分隔线」
        let html = md_of("标题\n---\n");
        assert!(!html.contains("<h1"), "{html}");
        assert!(html.contains("<hr"), "--- 应当退化成分隔线: {html}");
    }

    /// 摘 Setext 时别把 `# 标题` 那条路一起摘了 —— 两种标题是分开的两条规则
    #[test]
    fn atx_headings_still_work() {
        let html = md_of("# 一级\n\n## 二级\n");
        assert!(html.contains("<h1"), "{html}");
        assert!(html.contains("<h2"), "{html}");
    }

    /// 标题默认带 `id` —— `[跳转](#…)` 得有落点，锚点开关的开那一半
    #[test]
    fn headings_get_anchor_ids() {
        let html = md_of("## 文本与行内元素\n");
        assert!(html.contains(r#"id="文本与行内元素""#), "{html}");
    }

    /// 嵌套解析（块里的标题、模板页展开）不该把同一个标题挂两遍 `id` ——
    /// 子解析挂一次，外层 core 再走整棵树又挂一次，重复的 id 是无效 HTML
    #[test]
    fn nested_parses_do_not_duplicate_heading_ids() {
        let html = md_of("::note\n  # 标题甲\n");
        assert_eq!(
            html.matches(r#"id="标题甲""#).count(),
            1,
            "一个标题只该有一个 id：{html}"
        );
    }

    /// 关掉之后一个 `id=` 都不剩 —— 开关要关得干净，而不是只关掉一半
    #[test]
    fn anchor_anchors_can_be_turned_off() {
        let html = Mdpp::builder()
            .heading_anchors(false)
            .build()
            .render("## 表格\n");
        assert!(html.contains("<h2"), "{html}");
        assert!(!html.contains("id="), "{html}");
    }

    /// 四个空格开头的行是正文不是代码 —— 缩进在本库里是 `::` 块的边界，两者会互相干扰
    #[test]
    fn indented_code_blocks_are_off_by_default() {
        let html = md_of("    四个空格开头\n");
        assert!(!html.contains("<pre"), "不该再被当成代码块：{html}");
        assert!(html.contains("四个空格开头"), "{html}");
    }

    /// 代码片段里的字面量不许被排版规则改写。
    ///
    /// 排版规则会把 `--` 变成 `–`、把直引号变成弯引号 —— 在正文里是锦上添花，
    /// 在代码片段里则是**改坏了内容**：`--accent` 变成 `–accent` 就不再是那个
    /// CSS 变量名了。
    #[test]
    fn code_spans_keep_their_literals() {
        let source = r#"`--accent` 与 `a -- b` 与 "引号""#;
        let html = md_of(source);
        assert!(html.contains("--accent"), "破折号被改写了：{html}");
        assert!(html.contains("a -- b"), "破折号被改写了：{html}");
        assert!(!html.contains('\u{201c}'), "引号被换成弯引号了：{html}");
        assert!(
            html.contains("&quot;引号&quot;"),
            "直引号应当原样保留：{html}"
        );
    }

    /// 排版规则是可以打开的 —— 这里是证明「默认关」是一个选择，不是不支持
    #[test]
    fn the_typographer_can_be_turned_on_on_purpose() {
        let html = Mdpp::builder().typographer(true).build().render("a -- b\n");
        assert!(html.contains('\u{2013}'), "{html}");
    }

    /// 关掉内部链接之后，`[[…]]` 老实当普通文字
    #[test]
    fn the_wikilink_syntax_can_be_turned_off() {
        let html = Mdpp::builder().wikilink(false).build().render("[[目标]]\n");
        assert!(!html.contains("wikilink"), "{html}");
        assert!(html.contains("[[目标]]"), "{html}");
    }

    /// 自定义注册表换掉整张表：`::quote` 不在了，它就是未知块
    #[test]
    fn a_custom_registry_replaces_the_whole_table() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("我的块", ContentMode::Markdown, |_, _, _| {});
        let mdpp = Mdpp::builder().registry(registry).build();

        assert!(!mdpp.render("::quote\n  甲\n").contains("<blockquote"));
        assert!(mdpp.render("::quote\n  甲\n").contains("template--unknown"));
    }

    /// 注册进去的块真的渲染了
    #[test]
    fn a_registered_block_renders() {
        let mut registry = BlockRegistry::with_builtins();
        registry.register_fn("问候", ContentMode::Markdown, |block, node, fmt| {
            fmt.open("p", &[("class", "greet".to_string())]);
            fmt.text(&format!("你好，{}", block.param("who").unwrap_or("世界")));
            fmt.contents(&node.children);
            fmt.close("p");
        });
        let mdpp = Mdpp::builder().registry(registry).build();

        let html = mdpp.render("::问候 who=某人\n  内容\n");
        assert!(html.contains(r#"class="greet""#), "{html}");
        assert!(html.contains("你好，某人"), "{html}");
        // 内容按 Markdown 档处理，所以正文解析过了
        assert!(html.contains("<p>内容</p>"), "{html}");
    }

    /// **原始档**的内容不解析：写在里面的 markdown 记号原样留着
    #[test]
    fn a_raw_block_keeps_its_body_untouched() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("原样", ContentMode::Raw, |block, node, fmt| {
            fmt.open("pre", &[]);
            fmt.open("code", &[]);
            fmt.text(&block.body);
            fmt.close("code");
            fmt.close("pre");
            assert!(node.children.is_empty(), "Raw 档不该解析内容");
        });
        let mdpp = Mdpp::builder().registry(registry).build();

        let html = mdpp.render("::原样\n  *星号* 与 $a_1$\n");
        assert!(html.contains("*星号* 与 $a_1$"), "{html}");
        assert!(!html.contains("<em>"), "{html}");
    }

    /// 两个 `Mdpp` 各带各的注册表，互不干扰 —— 注册表是**每解析器一份**，
    /// 不是全局一份
    #[test]
    fn two_parsers_keep_their_own_registry() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("只有一个", ContentMode::Raw, |_, _, fmt| {
            fmt.text("出来了")
        });
        let one = Mdpp::builder().registry(registry).build();
        let two = Mdpp::builder().registry(BlockRegistry::new()).build();

        assert!(one.render("::只有一个\n").contains("出来了"));
        assert!(!two.render("::只有一个\n").contains("出来了"));
        // 交错着来也各归各
        assert!(one.render("::只有一个\n").contains("出来了"));
        assert!(!two.render("::只有一个\n").contains("出来了"));
    }

    /// 嵌套渲染走的是**当次那一份**注册表：外层注册了 `::套娃`，内层也要认得它
    ///
    /// 这条钉的是 `render_nested` 而不是「另建一个 `Mdpp`」—— 后者会拿到一份**内置**
    /// 注册表，于是外层注册的自定义块在内层全成了未知块。
    #[test]
    fn nested_rendering_reuses_the_outer_registry() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("套娃", ContentMode::Markdown, |block, _node, fmt| {
            if block.has("里") {
                fmt.text("到底了");
                return;
            }
            fmt.text(&crate::engine::render_nested("::套娃 里\n"));
        });
        let mdpp = Mdpp::builder().registry(registry).build();

        let html = mdpp.render("::套娃\n");
        assert!(html.contains("到底了"), "{html}");
        assert!(!html.contains("template--unknown"), "{html}");
    }

    /// 渲染器可以拿到子节点（内容已经被解析过）—— 容器型块的常规需求
    #[test]
    fn renderers_can_reach_the_parsed_children() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("壳", ContentMode::Markdown, |_, node, fmt| {
            fmt.open("div", &[("class", "shell".to_string())]);
            fmt.contents(&node.children);
            fmt.close("div");
        });
        let mdpp = Mdpp::builder().registry(registry).build();

        let html = mdpp.render("::壳\n  - 甲\n  - 乙\n");
        assert!(html.contains("<li>甲</li>"), "{html}");
        assert!(html.contains(r#"<div class="shell">"#), "{html}");
    }

    /// 渲染器拿到的是 markdown-it 的渲染器，签名照旧
    #[test]
    fn a_renderer_can_be_a_plain_function_pointer() {
        fn shout(block: &crate::registry::Block, node: &Node, fmt: &mut dyn Renderer) {
            fmt.text(block.name.as_str());
            fmt.contents(&node.children);
        }
        let mut registry = BlockRegistry::new();
        registry.register_fn("叫", ContentMode::Markdown, shout);
        let mdpp = Mdpp::builder().registry(registry).build();
        let html = mdpp.render("::叫\n  甲\n");
        assert!(html.contains("叫"), "{html}");
        assert!(html.contains("<p>甲</p>"), "{html}");
    }
}
