//! 分节：`[标签]` 与它下面缩进的内容。
//!
//! 规则只有一条：**标签在基础缩进上，正文必须比标签更深**。基础缩进上出现
//! 没有标签头的正文是错误，而不是一个匿名的 `label = ""` 节。
//!
//! ```text
//! [甲]
//!   甲的内容
//! [乙]
//!   乙的内容
//! ```
//!
//! 这条规则让围栏、HTML 块里的 `[x]` 天然安全：它们在正文缩进里，不在标签层。
//! 于是分节这一层不再需要 `raw_zones` 去猜哪些方括号是给人看的字。

use markdown_it::{Node, NodeValue, Renderer};

use crate::block::dispatch;
use crate::registry::Block;

/// 一节：标签 + 已解析的子节点。
///
/// `label` 一定非空；第一个标签之前不允许有正文。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Section {
    /// `[标签]` 里的那几个字
    pub label: String,
    /// 这一节的原文，已去掉标签下的公共缩进。
    ///
    /// `ContentMode::Sections` 仍会把它解析成 `children`；需要原文的块（`each` /
    /// `rows` / 自定义块）直接拿这一份，不必再自己切一遍。
    pub body: String,
}

impl NodeValue for Section {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        // 兜底：万一某个渲染器忘了拆节、直接把内容写出来，也不该整段消失
        fmt.contents(&node.children);
    }
}

/// 取 `[__template__]` / `[__data__]` 两节：`rows` 这类固定两节的模板块。
///
/// `what` 是块名（例如 `rows`），只用于错误消息。顺序固定，多一节少一节都报错。
/// `each` 支持类型模板，自己解析这三类节。
pub(crate) fn template_and_data<'a>(
    node: &'a Node,
    what: &str,
) -> Result<(&'a Section, &'a Section), String> {
    let sections: Vec<&Section> = node
        .children
        .iter()
        .filter_map(|child| child.cast::<Section>())
        .collect();
    let labels: Vec<&str> = sections
        .iter()
        .map(|section| section.label.as_str())
        .collect();
    if labels.as_slice() != ["__template__", "__data__"] {
        return Err(format!(
            "`::{what}` 只认 [__template__] 和 [__data__] 两节，顺序也固定；这里读到的是 {labels:?}"
        ));
    }
    Ok((sections[0], sections[1]))
}

/// 分节扫描器的输出：标签 + **原文**，还没按 markdown 解析。
///
/// 普通分节块随后把 `body` 交给 `parse_markdown`；`each` 这类块则直接拿原文
/// 当模板用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawSection {
    pub(crate) label: String,
    pub(crate) body: String,
}

/// 一个分节块的结构错误。扫描阶段就会把它摆成 `template--problem`。
#[derive(Debug)]
pub(crate) struct SectionProblem {
    block: Block,
    why: String,
}

impl SectionProblem {
    pub(crate) fn new(block: Block, why: String) -> Self {
        SectionProblem { block, why }
    }
}

impl NodeValue for SectionProblem {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        dispatch::render_problem(&self.block, fmt, &self.why);
        fmt.cr();
    }
}

/// 把块正文切成 `[标签]` 节。
///
/// 输入是**已经按块头削平过**的正文；输出里的每一节 `body` 已经去掉正文的公共
/// 缩进，可以直接当 markdown 解析，也可以直接当 `each` 的模板。
pub(crate) fn parse(source: &str) -> Result<Vec<RawSection>, String> {
    let mut sections = Vec::new();
    let mut current: Option<Builder> = None;
    let mut base: Option<usize> = None;

    for (number, line) in source.split('\n').enumerate() {
        let number = number + 1;
        let line = line.strip_suffix('\r').unwrap_or(line);

        if line.trim().is_empty() {
            if let Some(current) = current.as_mut() {
                current.lines.push(String::new());
            }
            continue;
        }

        let indent = leading_indent(line);
        let content = &line[indent..];

        match base {
            None => {
                let Some(label) = section_label(content) else {
                    return Err(format!(
                        "分节的第一行必须是 [标签]：标签前的内容没有归属（第 {number} 行）"
                    ));
                };
                base = Some(indent);
                current = Some(Builder::new(label));
            }
            Some(base) => {
                if indent < base {
                    return Err(format!(
                        "缩进不足：这一行比 [标签] 还浅，不能归入任何一节（第 {number} 行）"
                    ));
                }
                if indent == base {
                    let Some(label) = section_label(content) else {
                        return Err(format!(
                            "这一行在标签层却没有标签头：分节内容必须缩进在 [标签] 之下（第 {number} 行）"
                        ));
                    };
                    if let Some(done) = current.take() {
                        sections.push(done.finish());
                    }
                    current = Some(Builder::new(label));
                } else {
                    let Some(current) = current.as_mut() else {
                        return Err(format!("这一行没有归属（第 {number} 行）"));
                    };
                    current.lines.push(line.to_string());
                }
            }
        }
    }

    if let Some(done) = current.take() {
        sections.push(done.finish());
    }
    if sections.is_empty() {
        return Err("分节是空的：至少要写一个 [标签]".to_string());
    }
    Ok(sections)
}

/// 半成品：标签与还没削平缩进的正文行。
struct Builder {
    label: String,
    lines: Vec<String>,
}

impl Builder {
    fn new(label: String) -> Self {
        Builder {
            label,
            lines: Vec::new(),
        }
    }

    fn finish(self) -> RawSection {
        let mut lines = self.lines;
        while lines.first().is_some_and(|line| line.trim().is_empty()) {
            lines.remove(0);
        }
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }

        // 正文的公共缩进：标签下面至少还有一层，但作者可能缩进两格或四格。
        // 取最小缩进削平，相对层级就保住了。
        let shared = lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| leading_indent(line))
            .min()
            .unwrap_or(0);
        let mut body = String::new();
        for line in lines {
            if line.trim().is_empty() {
                body.push('\n');
            } else {
                body.push_str(&line[shared..]);
                body.push('\n');
            }
        }

        RawSection {
            label: self.label,
            body,
        }
    }
}

/// 行首空白（空格 / 制表符）的字节数。
fn leading_indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// 一整行是不是 `[标签]`。只认整行，且标签里不能有方括号。
fn section_label(line: &str) -> Option<String> {
    let inner = line.trim_end().strip_prefix('[')?.strip_suffix(']')?;
    let label = inner.trim();
    if label.is_empty() || label.contains(['[', ']']) {
        return None;
    }
    Some(label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(sections: &[RawSection]) -> Vec<&str> {
        sections
            .iter()
            .map(|section| section.label.as_str())
            .collect()
    }

    /// 标签在基础缩进，正文缩进一层；正文的公共缩进被削掉
    #[test]
    fn labels_split_at_the_base_and_content_is_dedented() {
        let sections = parse("[甲]\n  甲文\n    更深的列表\n[乙]\n  乙文\n").unwrap();
        assert_eq!(labels(&sections), ["甲", "乙"]);
        assert_eq!(sections[0].body, "甲文\n  更深的列表\n");
        assert_eq!(sections[1].body, "乙文\n");
    }

    /// 第一个标签之前不许有正文；匿名节不再存在
    #[test]
    fn content_before_the_first_label_is_an_error() {
        let error = parse("没写标签\n[甲]\n  甲文\n").expect_err("应当报错");
        assert!(error.contains("第一行"), "{error}");
    }

    /// 标签层上出现没有标签头的行也是错误
    #[test]
    fn a_base_level_paragraph_without_a_label_is_an_error() {
        let error = parse("[甲]\n  甲文\n这一行和标签同列\n").expect_err("应当报错");
        assert!(error.contains("没有标签头"), "{error}");
        assert!(error.contains("缩进"), "{error}");
    }

    /// 正文不能和标签同列
    #[test]
    fn content_must_be_deeper_than_its_label() {
        let error = parse("[甲]\n和标签同列的正文\n").expect_err("应当报错");
        assert!(error.contains("没有标签头"), "{error}");
    }

    /// 缩进里的 `[x]` 是正文，不是新的一节
    #[test]
    fn an_indented_bracket_line_stays_in_the_body() {
        let sections = parse("[甲]\n  [这不是标签]\n  [group99]\n").unwrap();
        assert_eq!(labels(&sections), ["甲"]);
        assert!(sections[0].body.contains("[这不是标签]"));
        assert!(sections[0].body.contains("[group99]"));
    }

    /// 围栏里的 `[x]` 也在正文缩进里，不需要 raw zone 特判
    #[test]
    fn a_fence_is_protected_by_indentation() {
        let sections = parse("[甲]\n  ```text\n  [不是标签]\n  ```\n 收尾\n").unwrap();
        assert_eq!(labels(&sections), ["甲"]);
        assert!(sections[0].body.contains("[不是标签]"));
        assert!(sections[0].body.contains("收尾"));
    }

    /// 正文里再深一层的 `[x]` 只是内容（嵌套选项卡靠这条）
    #[test]
    fn a_deeper_label_is_content() {
        let sections = parse("[外]\n  ::tabs\n    [内]\n    内文\n").unwrap();
        assert_eq!(labels(&sections), ["外"]);
        assert!(sections[0].body.contains("[内]"));
    }

    /// 空节允许存在；块自己判断它是不是用法问题
    #[test]
    fn an_empty_section_is_allowed() {
        let sections = parse("[then]\n[else]\n  乙\n").unwrap();
        assert_eq!(labels(&sections), ["then", "else"]);
        assert!(sections[0].body.is_empty());
    }

    /// 重复标签合法；navbox 的分组名会重复
    #[test]
    fn duplicate_labels_are_allowed() {
        let sections = parse("[header]\n  甲\n[header]\n  乙\n").unwrap();
        assert_eq!(labels(&sections), ["header", "header"]);
    }

    /// 标签写坏（没闭合、带嵌套方括号）在标签层报错
    #[test]
    fn malformed_labels_are_errors() {
        for source in ["[甲\n  内容\n", "[甲[乙]]\n  内容\n", "[]\n  内容\n"] {
            assert!(parse(source).is_err(), "{source:?}");
        }
    }

    /// 空正文报错；只有空行也算空
    #[test]
    fn an_empty_source_is_an_error() {
        for source in ["", "   \n\n"] {
            let error = parse(source).expect_err("应当报错");
            assert!(error.contains("空的"), "{error}");
        }
    }
}
