//! 一次渲染的运行时环境：上下文、注册表、资源收集器、变体作用域都装在这里。
//!
//! 之前这些字段散在几个线程局部里，读一个块渲染器时很难看出「现在到底有哪些
//! 东西可用」。现在只有一份 [`RenderEnv`]：进入渲染时装上，退出时还原；嵌套渲染
//! 退出时把内层资源合并回外层。
//!
//! 线程局部仍然是 markdown-it 回调的入口：`NodeValue::render(&self, node, fmt)`
//! 没有「用户数据」参数，规则只能从这里取当前环境。但**只有本模块直接碰线程局部**，
//! 其它模块通过 [`with`] 读一个明确的 [`RenderEnv`]，不再各自维护字段。
//!
//! ## 关于依赖环
//!
//! `env` 持有 `RenderContext` / `BlockRegistry` / `Sink` 等类型，而这些模块又通过
//! `env` 读当前环境，看起来是一个模块级环。它是**类型归属环**，不是行为环：
//! 运行时环境本来就是这些类型的交汇点。要彻底消掉它，需要把环境显式传给所有
//! 渲染回调，而 markdown-it 的 `NodeValue::render` 没有这个参数。另一个
//! `engine ↔ block` 的环是 `::` 语法本身需要的，属于有意设计。

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use crate::assets::Sink;
use crate::context::RenderContext;
use crate::registry::BlockRegistry;

/// 「在渲染途中再渲染一段 markdown」的能力。
pub(crate) type Nested = Box<dyn Fn(&str) -> String>;

/// 当前变体组作用域（内部）。公开的组/选项在 [`crate::variants`]。
#[derive(Debug, Clone)]
pub(crate) struct VariantScope {
    pub(crate) group: String,
    pub(crate) value: String,
}

/// 新建一份变体组 ID 计数器。
pub(crate) fn sequence() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

/// 一趟渲染的全部跨切面状态。
pub(crate) struct RenderEnv {
    /// 链接解析器与模板页来源
    pub(crate) context: RenderContext,
    /// 当前注册表
    pub(crate) registry: Arc<BlockRegistry>,
    /// 资源收集器（单页/站点两种模式）
    pub(crate) assets: Sink,
    /// 嵌套渲染入口；只有真正的渲染装得上
    pub(crate) nested: Option<Nested>,
    /// 当前变体组作用域
    pub(crate) variant: Option<VariantScope>,
    /// 页内变体组 ID 计数器；嵌套渲染共享同一份
    pub(crate) variant_seq: Arc<AtomicUsize>,
    /// 模板展开层数
    pub(crate) depth: usize,
}

impl Default for RenderEnv {
    fn default() -> Self {
        RenderEnv {
            context: RenderContext::default(),
            registry: Arc::new(BlockRegistry::default()),
            assets: Sink::default(),
            nested: None,
            variant: None,
            variant_seq: sequence(),
            depth: 0,
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<RenderEnv>> = const { RefCell::new(None) };
}

/// 在**当前**环境上做一件事。不在渲染中就返回 `None`。
pub(crate) fn with<R>(f: impl FnOnce(&mut RenderEnv) -> R) -> Option<R> {
    CURRENT.with(|cell| cell.borrow_mut().as_mut().map(f))
}

/// 取走当前环境（嵌套渲染要先把闭包拿出来，避免在 `with` 的借用里再进渲染）
fn take() -> Option<RenderEnv> {
    CURRENT.with(|cell| cell.borrow_mut().take())
}

/// 直接放回一份环境（[`take`] 取走之后用）
pub(crate) fn set(env: Option<RenderEnv>) {
    CURRENT.with(|cell| *cell.borrow_mut() = env);
}

/// 进入一趟渲染。返回的守卫出栈时：
///
/// 1. 把内层收集到的资源**合并回外层**（嵌套渲染不让资源消失）；
/// 2. 还原进入之前的那一份（可能没有）。
pub(crate) struct Guard {
    previous: Option<RenderEnv>,
}

impl Guard {
    /// 这一趟是不是嵌套渲染
    pub(crate) fn nested(&self) -> bool {
        self.previous.is_some()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let inner = take();
        if let Some(mut previous) = self.previous.take() {
            if let Some(inner) = inner {
                for asset in inner.assets.assets {
                    if previous.assets.hashes.insert(asset.hash.clone()) {
                        previous.assets.assets.push(asset);
                    }
                }
            }
            set(Some(previous));
        }
    }
}

/// 装上这一趟的环境，返回守卫。`nested` 由守卫回答。
pub(crate) fn enter(env: RenderEnv) -> Guard {
    let previous = take();
    set(Some(env));
    Guard { previous }
}
