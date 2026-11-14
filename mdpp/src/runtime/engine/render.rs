use std::collections::HashSet;
use std::sync::Arc;

use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::lheading::SetextHeader;
use markdown_it::{MarkdownIt, Node};

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
    let (depth, template_vars, render_depth, variant, variant_seq) = crate::env::with(|env| {
        (
            env.depth,
            env.template_vars.clone(),
            env.render_depth,
            env.variant.clone(),
            Arc::clone(&env.variant_seq),
        )
    })
    .unwrap_or_else(|| (0, None, 0, None, crate::env::sequence()));
    let env = crate::env::RenderEnv {
        context: context.clone(),
        registry: Arc::clone(registry),
        assets: crate::runtime::asset::Sink {
            mode: mode.clone(),
            assets: Vec::new(),
            hashes: HashSet::new(),
        },
        styles: Vec::new(),
        scripts: Vec::new(),
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
        features: Features::default(),
        problems: crate::engine::Problems::default(),
        diagnostics: 0,
        problem_detail: None,
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
    let outline = outline_of(&parsed);
    let features = features_of(&parsed);
    // 嵌套渲染里出现的特性也要让最外层看见：`::demo` 里的 mermaid 一样要引脚本
    crate::env::with(|env| env.features.merge(features));
    let started = std::time::Instant::now();
    let html = parsed.render();
    let size = Size::measure(source, started.elapsed(), &html);
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
    let outline = outline_of(node);
    let features = features_of(node);
    crate::env::with(|env| env.features.merge(features));
    let html = node.render();
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

/// 从解析树里取标题。模板页在解析期已经展开，所以模板里的标题也会在这里出现。
fn outline_of(root: &Node) -> Vec<Heading> {
    let mut outline = Vec::new();
    root.walk(|node, _| {
        let level = if let Some(heading) = node.cast::<ATXHeading>() {
            heading.level
        } else if let Some(heading) = node.cast::<SetextHeader>() {
            heading.level
        } else {
            return;
        };
        let text = node.collect_text().trim().to_string();
        let id = node
            .attrs
            .iter()
            .find(|(name, _)| *name == "id")
            .map(|(_, value)| value.clone());
        outline.push(Heading { level, text, id });
    });
    outline
}

/// 从解析树里取前端特性。这里只认**语义节点**：正文里恰好写着
/// `class="mdpp-mermaid"` 不算 —— 那是 `::html` 的内容，不是我们生成的图。
fn features_of(root: &Node) -> Features {
    let mut features = Features::default();
    root.walk(|node, _| {
        if let Some(fence) = node.cast::<CodeFence>() {
            if !fence
                .info
                .split_whitespace()
                .next()
                .unwrap_or("")
                .is_empty()
            {
                features.highlight = true;
            }
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
