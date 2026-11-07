//! 把一段 markdown 交给真正的解析器，**每一层**都走这里。
//!
//! 链接目标里的空格编码（[`crate::targets`]）必须在原始区之外做，而原始区只有
//! 解析器自己知道（块引用、列表、嵌套容器）。所以先探针解析一遍拿区间，再交给
//! 真正的解析器 —— 顶层、块正文、模板展开与 `<markdown>` 用的是同一条规矩。

use markdown_it::{MarkdownIt, Node};

/// 解析一段 markdown。调用方负责在需要时装上渲染状态。
pub(crate) fn parse_markdown(parser: &MarkdownIt, source: &str) -> Node {
    let prepared = crate::targets::encode_target_spaces(source);
    parser.parse(&prepared)
}
