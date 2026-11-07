//! `::` 块的注册表：外部在这里挂块，内部在这里派发。
//!
//! [`BlockRegistry`] 是普通数据 —— 不含解析器，也不含全局状态。想要一份自定义的，
//! 构造一份交给 [`MdppBuilder::registry`](crate::engine::MdppBuilder::registry)，
//! 两个 [`Mdpp`](crate::engine::Mdpp) 各带各的，互不相干。
//!
//! 注册一块要交代两件事：渲染函数，以及**块内容怎么处理**（[`ContentMode`]）。
//! 后者不是可选项 —— 扫描器要靠它在内容被解析**之前**决定走哪条路，那时渲染函数
//! 还没被调用过。
//!
//! [`BlockRegistry::register`] 对同名块**覆盖**，这是改掉一个内置块最短的路。

mod block;
mod scope;
mod spec;
pub(crate) mod table;
mod tokenize;

pub use block::{Block, ContentMode};
pub use scope::{Guard, current, enter, lookup};
pub use spec::{BlockRenderer, BlockRendererFn, BlockSpec, ContentModeFn};
pub use table::BlockRegistry;
pub use tokenize::tokenize;

#[cfg(test)]
mod tests;
