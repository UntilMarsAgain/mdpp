//! 本库自带的**行内**语法：数学公式、内部链接、行内高亮与下划线/上下标。
//!
//! 块级的 `::` 方言在 [`crate::block`]；这里只放四组行内规则。

pub mod mark;
pub mod math;
pub mod wikilink;

/// 行内解析到手的**当前一行**（`pos` 起，到第一个换行或 `pos_max`，谁先算谁）。
///
/// 行内解析是**整段一递**的：段落的多行原文连着软换行一起递进来，`pos_max`
/// 到段尾而不是行尾 —— 所以「一对一记号自我配对」的行内规则（公式、高亮、
/// 下划线/上下标）都要在这里**收进自己看到的这一行**：跨了个软换行就当作
/// 没有闭合。不然隔几行的两个记号会悄悄结成一对，把中间一大段吞进去。
///
/// 星号那对不用这一把尺 —— 它有自己的完整配对机器，跨行正是 CommonMark
/// 该有的样子。
pub(crate) fn line_of(src: &str, pos: usize, pos_max: usize) -> &str {
    let rest = &src[pos..pos_max];
    match rest.find(['\n', '\r']) {
        Some(i) => &rest[..i],
        None => rest,
    }
}
