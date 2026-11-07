//! 给块作者与模板作者看的公开 API 门面。
//!
//! 这里是「写一个块需要什么」的稳定入口，不是内部实现的目录。具体实现分别落在
//! [`crate::block`] 与 [`crate::runtime`]。

pub mod author;
