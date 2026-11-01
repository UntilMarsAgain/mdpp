//! 本库自带的**行内**语法：数学公式、内部链接、行内高亮，以及下标、上标与
//! 下划线。
//!
//! 块级的 `::` 方言在 [`crate::block`]；这里一共四组行内规则。

pub mod baseline;
pub mod mark;
pub mod math;
pub mod wikilink;
