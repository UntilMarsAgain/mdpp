//! 一次渲染的**全部**跨切面状态，装在一个槽里。
//!
//! 之前是五份独立的线程局部：上下文、注册表、资源收集器、嵌套渲染能力、
//! `::theme` 当前节。它们必须在同一时刻装上、同一时刻还原，分开之后很容易
//! 出现「上下文装了、注册表还是上一趟的」这类半截状态。
//!
//! 现在只有一份 [`RenderState`]：进入时存下外层（可能没有），退出时把内层资源
//! 合并回外层，再还原。同步渲染下线程局部仍然是合适的传递方式 —— 回调签名
//! （`NodeValue::render(&self, node, fmt)`）里没有「用户数据」参数，环境只能
//! 从旁边拿。相关讨论见 [`crate::context`]。

use std::cell::RefCell;
use std::sync::Arc;

use crate::assets::Sink;
use crate::context::RenderContext;
use crate::registry::BlockRegistry;

/// 「在渲染途中再渲染一段 markdown」的能力。
pub(crate) type Nested = Box<dyn Fn(&str) -> String>;

/// 一趟渲染的环境。
#[derive(Default)]
pub(crate) struct RenderState {
    /// 链接解析器与模板页来源
    pub(crate) context: Option<RenderContext>,
    /// 当前注册表
    pub(crate) registry: Option<Arc<BlockRegistry>>,
    /// 资源收集器（单页/站点两种模式）
    pub(crate) assets: Option<Sink>,
    /// 嵌套渲染入口；只有 `render_parts` 装得上
    pub(crate) nested: Option<Nested>,
    /// 当前 `::theme` 节
    pub(crate) theme_slot: Option<String>,
    /// 模板展开层数
    pub(crate) depth: usize,
}

thread_local! {
    static CURRENT: RefCell<Option<RenderState>> = const { RefCell::new(None) };
}

/// 在**当前**状态上做一件事。不在渲染中就返回 `None`。
pub(crate) fn with<R>(f: impl FnOnce(&mut RenderState) -> R) -> Option<R> {
    CURRENT.with(|cell| cell.borrow_mut().as_mut().map(f))
}

/// 取走当前状态（嵌套渲染要先把闭包拿出来，避免在 `with` 的借用里再进渲染）
pub(crate) fn take() -> Option<RenderState> {
    CURRENT.with(|cell| cell.borrow_mut().take())
}

/// 直接放回一份状态（[`take`] 取走之后用）
pub(crate) fn set(state: Option<RenderState>) {
    CURRENT.with(|cell| *cell.borrow_mut() = state);
}

/// 进入一趟渲染。返回的守卫出栈时：
///
/// 1. 把内层收集到的资源**合并回外层**（嵌套渲染不让资源消失）；
/// 2. 还原进入之前的那一份（可能没有）。
pub(crate) struct Guard {
    previous: Option<RenderState>,
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
                if let Some(inner_sink) = inner.assets {
                    match previous.assets.as_mut() {
                        Some(outer) => {
                            for asset in inner_sink.assets {
                                if outer.hashes.insert(asset.hash.clone()) {
                                    outer.assets.push(asset);
                                }
                            }
                        }
                        // 外层只是装了上下文/注册表（低层用法）：把资源收进来，
                        // 不让内层那一趟的 ::css / ::js 凭空消失
                        None => previous.assets = Some(inner_sink),
                    }
                }
            }
            set(Some(previous));
        } else {
            // 最外层：内层已经取走（`take`），不留下任何状态
            drop(inner);
        }
    }
}

/// 装上这一趟的环境，返回守卫。`nested` 由守卫回答。
pub(crate) fn enter(state: RenderState) -> Guard {
    let previous = take();
    set(Some(state));
    Guard { previous }
}

/// 进不去（渲染外）时用的空状态：只给低层 `enter` 兜底，不参与真正的渲染。
pub(crate) fn default_state() -> RenderState {
    RenderState::default()
}
