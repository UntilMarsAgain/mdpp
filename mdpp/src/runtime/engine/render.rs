use std::collections::HashSet;
use std::sync::Arc;

use markdown_it::{MarkdownIt, Node};

use crate::assets::{self, Asset, AssetMode};
use crate::context::RenderContext;
use crate::parse::markdown::parse_markdown;
use crate::registry::BlockRegistry;
use crate::telemetry::{Size, debug, trace};

/// 装上这一趟的渲染状态：上下文、注册表、资源收集器、嵌套入口，以及从外层
/// 继承的层数与变体作用域。
fn enter_render_env(
    parser: &Arc<MarkdownIt>,
    registry: &BlockRegistry,
    context: &RenderContext,
    mode: AssetMode,
) -> crate::env::Guard {
    let (depth, variant, variant_seq) =
        crate::env::with(|env| (env.depth, env.variant.clone(), Arc::clone(&env.variant_seq)))
            .unwrap_or_else(|| (0, None, crate::env::sequence()));
    let env = crate::env::RenderEnv {
        context: context.clone(),
        registry: Arc::new(registry.clone()),
        assets: crate::runtime::asset::Sink {
            mode: mode.clone(),
            assets: Vec::new(),
            hashes: HashSet::new(),
        },
        nested: Some(Box::new({
            let parser = Arc::clone(parser);
            let registry = registry.clone();
            let context = context.clone();
            let mode = mode.clone();
            move |more: &str| render_parts(&parser, &registry, &context, more, mode.clone()).0
        })),
        variant,
        variant_seq,
        depth,
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
    registry: &BlockRegistry,
    context: &RenderContext,
    source: &str,
    mode: AssetMode,
) -> (String, Vec<Asset>) {
    // 「再渲染一段」用的是**当次这一趟**的上下文与注册表，而不是新的空的一份
    let guard = enter_render_env(parser, registry, context, mode);
    let nested = guard.nested();
    trace!(
        depth = crate::context::depth(),
        "嵌套渲染：装上再渲染一段的能力"
    );

    let started = std::time::Instant::now();
    let html = parse_markdown(parser, source).render();
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
    drop(guard);
    (html, assets)
}

/// 渲染一棵已经解析好的 AST：装上渲染状态，再让 markdown-it 走树。
///
/// 与 [`render_parts`] 的区别只在于「不解析」—— 嵌套渲染仍然走 `render_parts`，
/// 所以 `<markdown>` 这类在渲染途中再解析的块照常工作。
pub(super) fn render_node_parts(
    parser: &Arc<MarkdownIt>,
    registry: &BlockRegistry,
    node: &Node,
    context: &RenderContext,
    mode: AssetMode,
) -> (String, Vec<Asset>) {
    let guard = enter_render_env(parser, registry, context, mode);
    let nested = guard.nested();
    let html = node.render();
    let assets = if nested {
        Vec::new()
    } else {
        assets::take_assets()
    };
    drop(guard);
    (html, assets)
}
