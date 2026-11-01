//! 行内高亮：`==重点==` → `<mark>`。
//!
//! 规矩与 [`super::math`] 的 `$…$` 同一条「宁可漏认，也别把正文里的字吞掉」：
//!
//! - 内容**紧贴**两个 `==`（`== 5 ==` 是普通文字）；
//! - 三个往上的一串 `=` 不认（`x === y`、`====` 都是普通文字，见 `run` 里的拒绝）；
//! - 不嵌套 —— 第一对先认：`==a==b==c==` 高亮 a 与 c，各自成档；
//! - 内部**照常按行内规则解析**：粗体、`[[内部链接]]`、`$公式$` 都能带，做法同
//!   cmark 链接 —— 换上自己的节点，把子窗口递回 `tokenize` 再换回来；
//!
//! `<mark>` 本来就在 [`crate::block::html`] 的净化白名单里，此前正文只能借 `::html`
//! 绕着写；这里给的是一条不碰净化的正路。代码块与行内代码里的 `==` 不受影响 ——
//! 那是更外层的规则先接走的。

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

/// 一段行内高亮。
///
/// 不带字段也不带属性：内容在 `children` 里（同 cmark 的强调），样式交给
/// `<mark>` 原生的黄底；要换样子直接给 `mark` 写样式。
#[derive(Debug, Clone, PartialEq)]
pub struct Mark;

impl NodeValue for Mark {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("mark", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("mark");
    }
}

/// 挂到解析器上
pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<MarkScanner>();
}

#[doc(hidden)]
pub struct MarkScanner;

impl InlineRule for MarkScanner {
    const MARKER: char = '=';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let start = state.pos;
        // 只看这一行剩下的部分：高亮不跨行（`pos_max` 就是行尾）
        let after = state.src[start..state.pos_max].strip_prefix("==")?;

        // 剩下还是 `=`，说明写下的是三连或更多（`===`、`====`）—— 不是开
        if after.starts_with('=') {
            return None;
        }

        let end = after.find("==")?;
        let inner = &after[..end];

        // 贴边有空白就不认：数学那条用同一把尺
        if inner.is_empty()
            || inner.starts_with(char::is_whitespace)
            || inner.ends_with(char::is_whitespace)
        {
            return None;
        }

        // 内部照常按行内规则解析。做法同 cmark 链接（`full_link`）：把自己的节点
        // 挂到 `state.node` 上，把窗口收成高亮的内部，递回 tokenize，再换回来 ——
        // 换回来之后外层才会把这个节点收走，里外不会串。
        let old_node = std::mem::replace(&mut state.node, Node::new(Mark));
        let max = state.pos_max;
        state.pos = start + 2;
        state.pos_max = start + 2 + end;
        state.md.inline.tokenize(state);
        state.pos = start;
        state.pos_max = max;
        let node = std::mem::replace(&mut state.node, old_node);

        // `==` + 内容 + `==`
        Some((node, 4 + end))
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
    fn the_basics_render_as_mark() {
        let html = render("这一段里有个==重点==。\n");
        assert!(html.contains("<mark>重点</mark>"), "{html}");
    }

    /// 内部照常按行内规则解析：粗体走得进去
    #[test]
    fn inner_content_is_parsed_inline() {
        let html = render("==**加粗**的重点==\n");
        assert!(html.contains("<mark><strong>加粗</strong>的重点</mark>"), "{html}");
    }

    /// `[[内部链接]]` 也走得进去 —— 这一段没有任何解析器递进来，落红链，
    /// 但红链仍然是个链接节点，还在 mark 里
    #[test]
    fn a_wikilink_inside_stays_a_link() {
        let html = render("==见[[某页]]==\n");
        assert!(html.contains("wikilink--missing"), "{html}");
        assert!(html.contains("<mark>见<a"), "{html}");
    }

    /// 该是普通文字的那个 `==`：不闭合、贴边留白、三连起头都不认；
    /// 这几类误伤里带等号的正文最多，宁可漏认
    #[test]
    fn ordinary_equal_signs_stay_prose() {
        for src in [
            "没有闭合的==一对\n",
            "x == y 是等式\n",
            "x === y 也是\n",
            "====\n",
            "判等写成 == 5 == 的样子\n",
        ] {
            let html = render(src);
            assert!(!html.contains("<mark"), "{src:?} 不该被当成高亮：{html}");
        }
    }

    /// 不嵌套：第一对先认，后面那对各自成档
    #[test]
    fn the_first_pair_wins_and_marks_do_not_nest() {
        let html = render("==a==b==c==\n");
        assert_eq!(html.matches("<mark>").count(), 2, "{html}");
        assert!(html.contains("<mark>a</mark>b<mark>c</mark>"), "{html}");
    }

    /// 代码里的 `==` 原样留着 —— 那里的写法是给人看的例子，改了就成了说谎
    #[test]
    fn code_keeps_its_equals() {
        let html = render("`a == b` 认不出，对不对？\n\n```sh\nif a == b; then\n```\n");
        assert!(!html.contains("<mark"), "{html}");
        assert!(html.contains("a == b"), "{html}");
    }

    /// 公式与删除线这些老规矩不被这一条影响 —— 等号与波浪线各管各的
    #[test]
    fn neighbours_are_unaffected() {
        let html = render("$a = 1$ 与 ~~删掉~~ 与 ==高亮==\n");
        assert!(html.contains(r#"data-tex="a = 1""#), "{html}");
        assert!(html.contains("<s>删掉</s>"), "{html}");
        assert!(html.contains("<mark>高亮</mark>"), "{html}");
    }
}
