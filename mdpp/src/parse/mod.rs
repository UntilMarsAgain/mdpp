//! 源码解析的辅助件：原始区、链接目标、标题 slug 与行内语法。
//!
//! 它们都只面对一段文本/一棵 markdown-it 的树，不持有渲染状态；放在同一处，
//! 与 [`crate::block`] 的「块方言」和 [`crate::runtime`] 的「渲染运行时」分开。

pub(crate) mod markdown;
pub mod raw;
pub mod slug;
pub mod syntax;
pub mod targets;
