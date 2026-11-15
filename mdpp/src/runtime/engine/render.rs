use std::sync::Arc;

use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, QualName, local_name, ns, parse_fragment};
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::{MarkdownIt, Node};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use super::{Features, Heading, RenderReport};
use crate::assets::{self, Asset, AssetMode};
use crate::block::dispatch::BlockNode;
use crate::context::RenderContext;
use crate::parse::markdown::parse_markdown;
use crate::registry::BlockRegistry;
use crate::syntax::math::Math;
use crate::telemetry::{Size, debug, trace};

/// 装上这一趟的渲染状态：上下文、注册表、资源收集器、嵌套入口，以及从外层
/// 继承的层数与变体作用域。
fn enter_render_env(
    parser: &Arc<MarkdownIt>,
    registry: &Arc<BlockRegistry>,
    context: &RenderContext,
    mode: AssetMode,
) -> crate::env::Guard {
    let (depth, template_vars, render_depth, variant, variant_seq, inline) =
        crate::env::with(|env| {
            (
                env.depth,
                env.template_vars.clone(),
                env.render_depth,
                env.variant.clone(),
                Arc::clone(&env.variant_seq),
                env.inline,
            )
        })
        .unwrap_or_else(|| (0, None, 0, None, crate::env::sequence(), false));
    let env = crate::env::RenderEnv {
        context: context.clone(),
        registry: Arc::clone(registry),
        assets: crate::runtime::asset::Sink {
            mode: mode.clone(),
            ..crate::runtime::asset::Sink::default()
        },
        nested: Some(Box::new({
            let parser = Arc::clone(parser);
            let registry = Arc::clone(registry);
            let context = context.clone();
            let mode = mode.clone();
            move |more: &str| render_parts(&parser, &registry, &context, more, mode.clone()).0
        })),
        variant,
        variant_seq,
        depth,
        template_vars,
        render_depth,
        inline,
        ..crate::env::RenderEnv::default()
    };
    crate::env::enter(env)
}

/// 真正干活的那一趟：装上这一趟的渲染状态，解析、渲染，退出时还原。
///
/// 拆成自由函数（而不是留在 `&self` 的方法里）是因为嵌套渲染要在线程局部里存一个
/// `'static` 闭包，而闭包得抓住解析器与上下文的**所有权**（`Arc` 的克隆）。
///
/// 返回值里的资源只在**最外层**有意义；嵌套渲染的资源由 `env::Guard` 合并回
/// 外层，不会丢。
pub(super) fn render_parts(
    parser: &Arc<MarkdownIt>,
    registry: &Arc<BlockRegistry>,
    context: &RenderContext,
    source: &str,
    mode: AssetMode,
) -> (String, Vec<Asset>, RenderReport) {
    // 「再渲染一段」用的是**当次这一趟**的上下文与注册表，而不是新的空的一份
    let guard = enter_render_env(parser, registry, context, mode);
    let nested = guard.nested();
    trace!(
        depth = crate::context::depth(),
        "嵌套渲染：装上再渲染一段的能力"
    );
    let parsed = parse_markdown(parser, source);
    finish(guard, nested, &parsed, Some(source))
}

/// 渲染一棵已经解析好的 AST：装上渲染状态，再让 markdown-it 走树。
///
/// 与 [`render_parts`] 的区别只在于「不解析」—— 嵌套渲染仍然走 `render_parts`，
/// 所以 `<markdown>` 这类在渲染途中再解析的块照常工作。
pub(super) fn render_node_parts(
    parser: &Arc<MarkdownIt>,
    registry: &Arc<BlockRegistry>,
    node: &Node,
    context: &RenderContext,
    mode: AssetMode,
) -> (String, Vec<Asset>, RenderReport) {
    let guard = enter_render_env(parser, registry, context, mode);
    let nested = guard.nested();
    finish(guard, nested, node, None)
}

/// 两个入口共用的收尾：取标题与特性、渲染、取资源、退出守卫。
///
/// `source` 有值时才记渲染尺寸日志（那条日志算的是「一份源文 → 一段 HTML」，
/// 直接渲染现成 AST 时没有可比的源文）。
fn finish(
    guard: crate::env::Guard,
    nested: bool,
    root: &Node,
    source: Option<&str>,
) -> (String, Vec<Asset>, RenderReport) {
    let features = features_of(root);
    // 嵌套渲染里出现的特性也要让最外层看见：`::demo` 里的 mermaid 一样要引脚本
    crate::env::with(|env| env.features.merge(features));
    let started = std::time::Instant::now();
    let html = root.render();
    // 标题从最终 HTML 里取：嵌套解析（`::each` / `::rows` / `<markdown>`）产生的
    // 标题不在外层 AST 里，按产物走才能保住文档顺序与净化后的锚点。
    let outline = if nested {
        Vec::new()
    } else {
        outline_of(&html)
    };
    if let Some(source) = source {
        let size = Size::measure(source, started.elapsed(), &html);
        debug!(
            in_bytes = size.in_bytes,
            out_bytes = size.out_bytes,
            micros = size.micros,
            "渲染一趟"
        );
    }

    // 嵌套渲染不取资源：留在收集器里，守卫出栈时合并回外层
    let assets = if nested {
        Vec::new()
    } else {
        assets::take_assets()
    };
    let styles = if nested {
        Vec::new()
    } else {
        crate::env::take_styles()
    };
    let scripts = if nested {
        Vec::new()
    } else {
        crate::env::take_scripts()
    };
    // 问题计数与特性一样在守卫出栈时合并；这里先取这一趟的最终值
    let (features, problems) = crate::env::with(|env| (env.features, env.problems))
        .unwrap_or_else(|| (Features::default(), crate::engine::Problems::default()));
    drop(guard);
    (
        html,
        assets,
        RenderReport {
            outline,
            features,
            problems,
            styles,
            scripts,
        },
    )
}

/// 从渲染后的 HTML 里取标题，按出现顺序。
///
/// 不用 AST：`::each` / `::rows` / `<markdown>` 在渲染期还会做嵌套解析，那部分
/// 标题不在外层 AST 里；按最终 HTML 收集，顺序、锚点与净化结果都和产物一致。
fn outline_of(html: &str) -> Vec<Heading> {
    let dom = parse_fragment(
        RcDom::default(),
        ParseOpts::default(),
        QualName::new(None, ns!(html), local_name!("body")),
        Vec::new(),
        true,
    )
    .from_utf8()
    .one(html.as_bytes());

    let mut outline = Vec::new();
    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        let mut heading = 0;
        if let NodeData::Element { name, attrs, .. } = &node.data {
            heading = match &*name.local {
                "h1" => 1,
                "h2" => 2,
                "h3" => 3,
                "h4" => 4,
                "h5" => 5,
                "h6" => 6,
                _ => 0,
            };
            if heading != 0 {
                let id = attrs
                    .borrow()
                    .iter()
                    .find(|attr| &*attr.name.local == "id")
                    .map(|attr| attr.value.to_string());
                let text = text_content(&node).trim().to_string();
                outline.push(Heading {
                    level: heading,
                    text,
                    id,
                });
            }
        }
        // 标题里不会再套标题；文字已经由 `text_content` 收过了，不必再往下走。
        if heading == 0 {
            for child in node.children.borrow().iter().rev() {
                stack.push(child.clone());
            }
        }
    }
    outline
}

/// 元素里所有文字节点拼起来（`<code>` 这类行内标签也在内）。
fn text_content(root: &Handle) -> String {
    let mut out = String::new();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        match &node.data {
            NodeData::Text { contents } => out.push_str(&contents.borrow()),
            NodeData::Element { name, .. }
                if matches!(&*name.local, "script" | "style" | "template") =>
            {
                continue;
            }
            _ => {}
        }
        for child in node.children.borrow().iter().rev() {
            stack.push(child.clone());
        }
    }
    out
}

/// 从解析树里取前端特性。这里只认**语义节点**：正文里恰好写着
/// `class="mdpp-mermaid"` 不算 —— 那是 `::html` 的内容，不是我们生成的图。
fn features_of(root: &Node) -> Features {
    let mut features = Features::default();
    root.walk(|node, _| {
        if let Some(fence) = node.cast::<CodeFence>()
            && !fence
                .info
                .split_whitespace()
                .next()
                .unwrap_or("")
                .is_empty()
        {
            features.highlight = true;
        }
        if node.is::<Math>() {
            features.math = true;
        }
        if let Some(block) = node.cast::<BlockNode>() {
            let block = &block.0;
            match block.name.as_str() {
                "mermaid" if !block.body.trim().is_empty() => features.mermaid = true,
                "math" if !block.body.trim().is_empty() => features.math = true,
                "code"
                    if block
                        .param("lang")
                        .is_some_and(|language| !language.trim().is_empty()) =>
                {
                    features.highlight = true;
                }
                _ => {}
            }
        }
    });
    features
}
