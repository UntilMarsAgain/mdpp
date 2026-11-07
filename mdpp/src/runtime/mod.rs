//! 一次渲染的运行时：解析器、渲染状态、上下文与资源收集。
//!
//! 这几样东西共享同一个生命周期：进入 `render` 时一起装上，退出时一起还原。
//! 它们放在同一个模块里，是为了让「一次渲染」的边界在代码结构上也看得见。

pub(crate) mod asset;
pub mod assets;
pub mod context;
pub mod engine;
pub(crate) mod env;
mod render_context;
pub mod variants;
