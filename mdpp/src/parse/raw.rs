//! 源码里的「原始区」：代码块、HTML 块、行内代码，以及 `::code` / `::css` 这类
//! [`ContentMode::Raw`](crate::registry::ContentMode) 块。
//!
//! 它们的共同点是**markdown 的记号在里面不算记号**，于是 `![a](b c.png)` 的括号、
//! `[标签]` 的方括号都只是一段字。[`crate::targets`] 问的是「这行的字能不能动」——
//! 原始区里的不能动，改了就成了说谎（代码片段里的写法是给人看的例子）。
//!
//! 原始区由 `dialect::scan_parser` 解析一遍得到。扫描方言与渲染方言共用同一套
//! `::` 块规则；区别只在扫描模式下 `ScanBlockScanner` 不查模板、不解析正文，只把
//! Raw 块的字节区间记成 `RawBlock` 节点。这样探针不必知道模板页，也不会递归。
//!
//! ## HTML 块只认到「空行为止」
//!
//! CommonMark 把 HTML 块分了七类，逐类精确复刻不值当 —— 这里交给 markdown-it 的
//! HTML 插件判定，块边界与渲染时完全一致。

use std::ops::Range;

use markdown_it::Node;
use markdown_it::plugins::cmark::block::code::CodeBlock;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::inline::backticks::CodeInline;
use markdown_it::plugins::html::html_block::HtmlBlock;
use markdown_it::plugins::html::html_inline::HtmlInline;

use crate::parse::dialect::{self, RawBlock};

/// 源码里**不能当 markdown 记号看**的字节区间。
///
/// 区间来自一次真实解析的 `srcmap`，所以块引用/列表的前缀、嵌套层级都由
/// markdown-it 自己算好；`::code` 这类 Raw 块由 `RawBlock` 记下整块的区间。
/// 调用方只要按区间把源码切开，区间内原样搬走即可。
pub fn raw_zones(source: &str) -> Vec<Range<usize>> {
    let root = dialect::scan_parser().parse(source);
    let mut found = Vec::new();
    collect_zones(&root, &mut found);
    found.sort_by_key(|zone| zone.start);

    let mut merged: Vec<Range<usize>> = Vec::new();
    for zone in found {
        match merged.last_mut() {
            // srcmap 是块级的：相邻/重叠的区间合成一段，切分时少几次边界判断
            Some(last) if zone.start <= last.end => last.end = last.end.max(zone.end),
            _ => merged.push(zone),
        }
    }
    merged
}

fn collect_zones(node: &Node, out: &mut Vec<Range<usize>>) {
    if let Some(raw) = node.cast::<RawBlock>() {
        out.push(raw.range.clone());
        return;
    }

    let raw = node.is::<CodeBlock>()
        || node.is::<CodeFence>()
        || node.is::<HtmlBlock>()
        || node.is::<HtmlInline>()
        || node.is::<CodeInline>();
    if raw {
        if let Some(map) = node.srcmap {
            let (start, end) = map.get_byte_offsets();
            out.push(start..end);
        }
        // 原始区里不会再嵌套 markdown 节点；不再往下走，省掉一次遍历
        return;
    }
    for child in &node.children {
        collect_zones(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::raw_zones;

    /// 围栏仍是原始区
    #[test]
    fn a_fence_is_a_zone() {
        let zones = raw_zones("```\n[a](b c.png)\n```\n");
        assert!(!zones.is_empty(), "{zones:?}");
    }

    /// `::code` 的正文也是原始区 —— 这正是预扫描曾经改坏字符的地方
    #[test]
    fn a_raw_block_is_a_zone() {
        let src = "::code lang=md\n    [x](a b.png)\n";
        let zones = raw_zones(src);
        assert!(
            zones
                .iter()
                .any(|zone| zone.start == 0 && zone.end == src.trim_end().len()),
            "{zones:?}"
        );
    }

    /// 普通段落不是原始区
    #[test]
    fn ordinary_text_is_not_a_zone() {
        assert!(raw_zones("普通 [x](a b.png)\n").is_empty());
    }
}
