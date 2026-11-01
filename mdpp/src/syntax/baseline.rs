//! 下标、上标与下划线：`_…_`、`^…^`、`__…__`
//!
//! CommonMark 那对下划线强调在构建解析器时**摘掉**（[`crate::engine`] 里那句
//! `remove_rule`），同一对记号改成两个文字排版的记号，`^…^` 补成第三个。
//! 星号那对（`*斜*`、`**粗**`）原样保留 —— 它们自带完整配对机器，是 CommonMark
//! 该有的样子。
//!
//! 三条共用一把「贴字」的尺：
//!
//! - 配对的舞台是**块**：行内解析整段一递（CommonMark 行内记号的约定），到段
//!   尾都没找到闭着的记号就当普通文字。短语级的下划线跨软换行照样成对，与
//!   星号那对一致；
//! - 下标与上标是**字位级**的记号：内容里**不许有空白** —— `_2 3_`、`^a b^`
//!   不认；换行也是空白，所以它们实际只在一行里成立；
//! - 内容**紧贴**记号：贴边有空白不认（`_ 5_`、`^5 ^` 只是普通文字；这一条
//!   对下标与上标被上面那条一并管住，真正落在这里的是下划线）；
//! - 记号开头上叠着同类记号不认，而且**前一个字符还是记号的也不认** ——
//!   `___a___`、`^^^^a^^^^` 整串都是普通文字，长串的记号不炸、不误吞；
//! - **词内也认**（pandoc 的 `H~2~O` 是同一把尺）：`H_2_O`、`一__重点__二`
//!   都成立。代价是行文里的 `file_name_v2` 会被切成下标 —— 要照实显示的
//!   东西（代码、命令、文件名）照 CommonMark 的老习惯用反引号。
//!
//! 代码块与行内代码里的这些记号不受影响 —— 那是更外层的规则先接走的。

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

/// 一段**下划线**：`__…__`。
///
/// 用的是 HTML 的 `<u>`，语义按规范是「可以另加下划线的一条线」—— 传统纸面
/// 排版标重点的那条。要换样子直接给 `u` 写样式。
#[derive(Debug, Clone, PartialEq)]
pub struct Underline;

/// 一段**下标**：`_…_`（`H_2_O`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Subscript;

/// 一段**上标**：`^…^`（`E=mc^2^`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Superscript;

impl NodeValue for Underline {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("u", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("u");
    }
}

impl NodeValue for Subscript {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("sub", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("sub");
    }
}

impl NodeValue for Superscript {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("sup", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("sup");
    }
}

/// 挂到解析器上
pub fn add(md: &mut MarkdownIt) {
    // 双记号在前：`__…__` 的开头也以 `_` 起头，两条挂在同一个记号上时由它先试
    md.inline.add_rule::<UnderlineScanner>();
    md.inline.add_rule::<SubscriptScanner>();
    md.inline.add_rule::<SuperscriptScanner>();
}

/// 三个记号最后一步的共同动作：把自己的节点挂上，把窗口收成**记号内部**，递回
/// 行内解析，再把填好的节点换回来。做法同 cmark 链接（`full_link`）与
/// [`super::mark`] —— 换回来之后外层才收这个节点，里外不串。
///
/// `start` 是记号开头的字节位置，`inner_end` 是**内部内容**结尾的字节位置；
/// 返回 `(填好的节点, 该向后推进的字节数)`。
fn enclosed(
    state: &mut InlineState,
    start: usize,
    inner_end: usize,
    open_len: usize,
    node: Node,
) -> (Node, usize) {
    let old_node = std::mem::replace(&mut state.node, node);
    let max = state.pos_max;

    state.pos = start + open_len;
    state.pos_max = inner_end;
    state.md.inline.tokenize(state);

    state.pos = start;
    state.pos_max = max;
    let filled = std::mem::replace(&mut state.node, old_node);
    (filled, open_len * 2 + inner_end - start - open_len)
}

/// 贴字记号共同的第一关：**前一个字符还是这个记号**（`___` 那一串的成员），
/// 就不独立成开头，退回文字一条。
fn in_a_longer_run(state: &InlineState, start: usize, marker: u8) -> bool {
    start > 0 && state.src.as_bytes()[start - 1] == marker
}

#[doc(hidden)]
pub struct UnderlineScanner;

impl InlineRule for UnderlineScanner {
    const MARKER: char = '_';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let start = state.pos;
        if in_a_longer_run(state, start, b'_') {
            return None;
        }

        // 行内解析整段一递：配对在**块**内找闭合 —— 跨软换行也照找
        let after = state.src[start..state.pos_max].strip_prefix("__")?;
        // 三个往上的一串（`___`）不认
        if after.starts_with('_') {
            return None;
        }
        let end = after.find("__")?;
        let inner = &after[..end];
        // 贴边有空白就不认 —— 短语级的记号：内容带空白、跨软换行照样认
        if inner.is_empty()
            || inner.starts_with(char::is_whitespace)
            || inner.ends_with(char::is_whitespace)
        {
            return None;
        }

        let (node, consumed) = enclosed(state, start, start + 2 + end, 2, Node::new(Underline));
        Some((node, consumed)) // `__` + 内容 + `__`
    }
}

#[doc(hidden)]
pub struct SubscriptScanner;

impl InlineRule for SubscriptScanner {
    const MARKER: char = '_';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let start = state.pos;
        if in_a_longer_run(state, start, b'_') {
            return None;
        }

        let after = state.src[start..state.pos_max].strip_prefix('_')?;
        // 双记号起头是下划线那条的活（它先试）；三个往上的一串整串退回文字
        if after.starts_with('_') {
            return None;
        }
        let end = after.find('_')?;
        let inner = &after[..end];
        // 字位级的记号：内容里任何空白都不认 —— `_2 3_`、`_2\n3_` 都是普通文字
        if inner.is_empty() || inner.chars().any(char::is_whitespace) {
            return None;
        }

        let (node, consumed) = enclosed(state, start, start + 1 + end, 1, Node::new(Subscript));
        Some((node, consumed)) // `_` + 内容 + `_`
    }
}

#[doc(hidden)]
pub struct SuperscriptScanner;

impl InlineRule for SuperscriptScanner {
    const MARKER: char = '^';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let start = state.pos;
        if in_a_longer_run(state, start, b'^') {
            return None;
        }

        let after = state.src[start..state.pos_max].strip_prefix('^')?;
        // 三个往上的一串（`^^^`）不认
        if after.starts_with('^') {
            return None;
        }
        let end = after.find('^')?;
        let inner = &after[..end];
        // 同下标：字位级的记号，内容里有空白不认
        if inner.is_empty() || inner.chars().any(char::is_whitespace) {
            return None;
        }

        let (node, consumed) = enclosed(state, start, start + 1 + end, 1, Node::new(Superscript));
        Some((node, consumed)) // `^` + 内容 + `^`
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    /// 整篇走一遍（块规则与行内规则都在这一次里跑）
    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    #[test]
    fn underline_comes_out_as_u() {
        let html = render("一个__下划线__记号。\n");
        assert!(html.contains("<u>下划线</u>"), "{html}");
    }

    /// 换了新身份的旧记号：`__` 与 `_` 不再出强调
    #[test]
    fn underscores_emphasize_no_more() {
        let html = render("__粗__ 与 _斜_\n");
        assert!(html.contains("<u>粗</u>"), "{html}");
        assert!(html.contains("<sub>斜</sub>"), "{html}");
        assert!(!html.contains("<strong"), "{html}");
        assert!(!html.contains("<em"), "{html}");
    }

    /// 星号那对原封不动 —— 老规矩照走
    #[test]
    fn asterisks_keep_the_commonmark_live() {
        let html = render("*斜星* 与 **粗星**\n");
        assert!(html.contains("<em>斜星</em>"), "{html}");
        assert!(html.contains("<strong>粗星</strong>"), "{html}");
    }

    /// 下标在词内：化学式就是为这个写的
    #[test]
    fn subscript_works_intraword() {
        let html = render("水的分子式是 H_2_O。\n");
        assert!(html.contains("H<sub>2</sub>O"), "{html}");
    }

    /// 上标在词内：E=mc^2^ 不用空格也能认出
    #[test]
    fn superscript_works_intraword() {
        let html = render("勾股定理与 E=mc^2^。\n");
        assert!(html.contains("mc<sup>2</sup>"), "{html}");
    }

    /// 字位级的记号：内容里有空白（含换行）就不认
    #[test]
    fn glyph_marks_take_no_whitespace_inside() {
        for src in [
            "记号 _2 3_ 只是文字\n",
            "记号 _2\n3_ 只是文字\n",
            "上标 ^a b^ 以及 ^a\nb^ 只是文字\n",
        ] {
            let html = render(src);
            assert!(!html.contains("<sub"), "{src:?} 不该变下标: {html}");
            assert!(!html.contains("<sup"), "{src:?} 不该变上标: {html}");
        }
    }

    /// 贴边的空白不认 —— `_ 5_`、`^5 ^` 只是普通文字
    #[test]
    fn marks_hug_their_content() {
        for src in ["下标 _ 5_ 不认\n", "上标 ^5 ^ 不认\n"] {
            let html = render(src);
            assert!(!html.contains("<sub"), "{src:?} 不该变下标: {html}");
            assert!(!html.contains("<sup"), "{src:?} 不该变上标: {html}");
        }
    }

    /// 成串的记号整串退回文字 —— `___` 与 `^^^^` 那种线不炸、不误吞
    #[test]
    fn a_run_of_markers_stays_prose() {
        for src in ["___a___\n", "^^^^a^^^^\n"] {
            let html = render(src);
            assert!(!html.contains("<u"), "{src:?} 不该变下划线: {html}");
            assert!(!html.contains("<sub"), "{src:?} 不该变下标: {html}");
            assert!(!html.contains("<sup"), "{src:?} 不该变上标: {html}");
        }
    }

    /// 下划线是短语级：跨软换行照样成对 —— 与星号那对同一条约定
    #[test]
    fn underline_pairs_across_a_soft_break() {
        let html = render("__跨\n行__\n");
        assert!(html.contains("<u>跨"), "{html}");
        assert!(html.contains("行</u>"), "{html}");
    }

    /// 中文句内夹着用 —— 词内也认是这三条的本分
    #[test]
    fn word_internal_uses_work_in_chinese() {
        let html = render("一__重点__二\n");
        assert!(html.contains("<u>重点</u>"), "{html}");
    }

    /// 下划线里能再写下标 —— 双记号先认对，内部的 `_` 由字位级的接走
    #[test]
    fn underline_can_hold_a_subscript() {
        let html = render("__重点_2_注__\n");
        assert!(html.contains("<u>重点<sub>2</sub>注</u>"), "{html}");
    }

    /// 不嵌套：第一对先认（`__a__b__c__` 是两段各自成文）
    #[test]
    fn the_first_pair_wins_for_underline_too() {
        let html = render("__a__b__c__\n");
        assert_eq!(html.matches("<u>").count(), 2, "{html}");
        assert!(html.contains("<u>a</u>b<u>c</u>"), "{html}");
    }

    /// 代码与转义里的记号原样留着 —— 照实显示的东西走反引号与反斜杠
    #[test]
    fn code_and_escapes_stay_literal() {
        let html = render("`H_2_O`、`__x__` 与\n\n```sh\nx_1_ 与 __y__\n```\n");
        assert!(!html.contains("<sub"), "{html}");
        assert!(!html.contains("<u"), "{html}");

        let html = render("\\_a\\_ 与 \\^c\\^\n");
        assert!(!html.contains("<sub"), "{html}");
        assert!(!html.contains("<sup"), "{html}");
        assert!(html.contains("_a_"), "{html}");
        assert!(html.contains("^c^"), "{html}");
    }
}
