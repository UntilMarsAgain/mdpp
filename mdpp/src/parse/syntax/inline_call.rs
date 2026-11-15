//! 行内模板调用：`::名字[参数]{正文}`。
//!
//! 它只在行内上下文里出现；块扫描器看到名字后面紧跟 `[` 或 `{` 时会主动让开，
//! 交给段落的行内解析。展开发生在解析期，模板本身的正文仍按块 Markdown 解析，
//! 到渲染时再由 `ScopedBlock` 把它收成行内。
//!
//! `::problem{...}` 是行内诊断的快捷形式：它不找模板，直接生成一个行内
//! problem 节点；这样行内位置上的错误不会被外层再折叠成一句泛泛的
//! “此处需要行内”。

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{MarkdownIt, Node};

use crate::block::dispatch::{self, ProblemCount, ProblemNode};
use crate::block::expand;
use crate::block::template::{self, TemplatePage};
use crate::registry::Block;
use crate::runtime::env::ProblemDetail;

/// 挂到渲染解析器上。
pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<InlineCallScanner>();
}

struct InlineCallScanner;

impl InlineRule for InlineCallScanner {
    const MARKER: char = ':';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let rest = state.src.get(state.pos..state.pos_max)?;
        let after = rest.strip_prefix("::")?;
        let name_end = after
            .find(|ch: char| ch.is_whitespace() || ch == '[' || ch == '{')
            .unwrap_or(after.len());
        if name_end == 0 {
            return None;
        }
        // 没有 `[` / `{` 的是块语法（或普通文字），不归行内规则。
        if !matches!(after.as_bytes().get(name_end), Some(b'[') | Some(b'{')) {
            return None;
        }
        let name = &after[..name_end];
        let mut cursor = name_end;
        let mut params_raw = None;
        if after.as_bytes().get(cursor) == Some(&b'[') {
            match scan_group(after, cursor, b'[', b']') {
                Ok((inner, next)) => {
                    params_raw = Some(inner);
                    cursor = next;
                }
                Err(()) => return Some((malformed(name, "参数没有闭合的 `]`"), line_end(rest))),
            }
        }
        let mut body = String::new();
        if after.as_bytes().get(cursor) == Some(&b'{') {
            match scan_group(after, cursor, b'{', b'}') {
                Ok((inner, next)) => {
                    body = unescape_braces(inner);
                    cursor = next;
                }
                Err(()) => return Some((malformed(name, "正文没有闭合的 `}`"), line_end(rest))),
            }
        }
        let params = match parse_params(name, params_raw) {
            Ok(params) => params,
            Err(why) => return Some((malformed(name, &why), line_end(rest))),
        };
        // `::problem{...}` / `::problem[why=...]`：行内诊断的快捷入口。
        if name == "problem" {
            let shown = params.clone();
            let block = node_block(name, params, body.clone());
            return Some((problem(block, &shown, &body), 2 + cursor));
        }

        let Some(page) = template::page_for(name) else {
            let block = node_block(name, params, body.clone());
            let detail = ProblemDetail {
                badge: "未知块".to_string(),
                class: dispatch::UNKNOWN_CLASS.to_string(),
                name: name.to_string(),
                why: format!("行内调用找不到模板 `{name}`"),
            };
            return Some((
                Node::new(ProblemNode::new(
                    block,
                    detail,
                    false,
                    true,
                    ProblemCount::Unknown,
                )),
                2 + cursor,
            ));
        };

        let node = match page {
            TemplatePage::Unreadable { reason } => problem(
                node_block(name, params, body.clone()),
                &[],
                &format!("模板页 `{name}` 读不出来：{reason}"),
            ),
            page => expand::expand_inline_block(name, params, body, page, state.md),
        };
        Some((node, 2 + cursor))
    }
}

fn node_block(name: &str, params: Vec<(String, String)>, body: String) -> Block {
    Block {
        name: name.to_string(),
        params,
        body,
    }
}

fn malformed(name: &str, why: &str) -> Node {
    problem(node_block(name, Vec::new(), String::new()), &[], why)
}

/// 生成一个强制行内的问题节点。
fn problem(block: Block, params: &[(String, String)], why: &str) -> Node {
    let mut name = None;
    let mut reason = None;
    for (key, value) in params {
        match key.as_str() {
            "name" if name.is_none() => name = Some(value.clone()),
            "why" if reason.is_none() => reason = Some(value.clone()),
            _ => {}
        }
    }
    let reason = reason
        .filter(|value| !value.is_empty())
        .or_else(|| (!why.is_empty()).then(|| why.to_string()))
        .unwrap_or_else(|| "行内调用写错了".to_string());
    let detail = ProblemDetail {
        badge: "行内内容有问题".to_string(),
        class: dispatch::MISUSED_CLASS.to_string(),
        name: name.unwrap_or_else(|| block.name.clone()),
        why: reason,
    };
    Node::new(ProblemNode::new(
        block,
        detail,
        false,
        true,
        ProblemCount::Misused,
    ))
}

/// 把 `[参数]` 里的写法交给块头解析器；返回规范化参数。
fn parse_params(name: &str, raw: Option<&str>) -> Result<Vec<(String, String)>, String> {
    let raw = raw.unwrap_or("").trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let line = format!("::{name} {raw}");
    let Some((parsed_name, params)) = Block::parse_header(&line) else {
        return Err("行内调用的参数写法有问题".to_string());
    };
    if parsed_name != name {
        return Err("行内调用的名字解析不一致".to_string());
    }
    template::check_header(name, &line, &params)?;
    Ok(params)
}

/// 扫一个成对括号；返回内部文本与结束后的下标。
fn scan_group(src: &str, start: usize, open: u8, close: u8) -> Result<(&str, usize), ()> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, ch) in src[start..].char_indices() {
        let index = start + offset;
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if (ch == '"' || ch == '\'') && open == b'[' {
            quote = Some(ch);
            continue;
        }
        if ch as u32 == open as u32 {
            depth += 1;
        } else if ch as u32 == close as u32 {
            depth -= 1;
            if depth == 0 {
                return Ok((&src[start + 1..index], index + 1));
            }
        }
    }
    Err(())
}

fn unescape_braces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some('{') | Some('}') => {
                    out.push(chars.next().expect("peeked"));
                    continue;
                }
                _ => {}
            }
        }
        out.push(ch);
    }
    out
}

fn line_end(src: &str) -> usize {
    src.find('\n').unwrap_or(src.len())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::block::template::{TemplatePage, TemplateSource};
    use crate::{Mdpp, RenderContext};

    #[derive(Debug, Default)]
    struct Pages(HashMap<String, TemplatePage>);

    impl TemplateSource for Pages {
        fn page(&self, name: &str) -> Option<TemplatePage> {
            self.0.get(name).cloned()
        }
    }

    fn render(pages: &[(&str, &str)], source: &str) -> crate::Rendered {
        let mut map = Pages::default();
        for (name, body) in pages {
            map.0.insert(
                (*name).to_string(),
                TemplatePage::Ready((*body).to_string()),
            );
        }
        let context = RenderContext::new().with_templates(Arc::new(map));
        Mdpp::new().render_report(source, &context)
    }

    #[test]
    fn inline_calls_take_params_and_body() {
        let rendered = render(
            &[("卡片", "::element tag=span\n  {{title}}：{{body}}\n")],
            "前 ::卡片[title=你好]{正文} 后\n",
        );
        assert!(rendered.html.contains("<p>前 <span>"), "{}", rendered.html);
        assert!(rendered.html.contains("你好：正文"), "{}", rendered.html);
        assert!(
            rendered.html.contains("</span> 后</p>"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.misused, 0);
    }

    #[test]
    fn the_four_empty_forms_are_accepted() {
        let rendered = render(
            &[("空", "::element tag=span\n  {{?name}}{{?body}}固定\n")],
            "::空[]\n\n::空[name=甲]\n\n::空{乙}\n\n::空[name=甲]{乙}\n",
        );
        assert!(rendered.html.contains("固定"), "{}", rendered.html);
        assert!(rendered.html.contains("甲固定"), "{}", rendered.html);
        assert!(rendered.html.contains("乙固定"), "{}", rendered.html);
        assert!(rendered.html.contains("甲乙固定"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 0);
    }

    #[test]
    fn a_missing_inline_template_is_one_unknown_span() {
        let rendered = render(&[], "前 ::不存在[] 后\n");
        assert!(
            rendered
                .html
                .contains("未知块：不存在。行内调用找不到模板 `不存在`"),
            "{}",
            rendered.html
        );
        assert!(
            !rendered.html.contains("mdpp-template--unknown"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.unknown, 1);
        assert_eq!(rendered.problems.misused, 0);
    }

    #[test]
    fn a_guard_keeps_the_reason_it_swallowed() {
        let rendered = render(&[("坏", "::guard\n  ::problem why=里面坏了\n")], "::坏[]\n");
        assert!(rendered.html.contains("里面坏了"), "{}", rendered.html);
        assert!(
            !rendered.html.contains("mdpp-template--problem"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.misused, 1);
    }

    #[test]
    fn a_block_output_inside_an_inline_call_keeps_an_inner_reason() {
        let rendered = render(
            &[("外壳", "::element tag=div\n  ::不存在[]\n")],
            "::外壳[]\n",
        );
        assert!(
            rendered.html.contains("未知块：不存在"),
            "{}",
            rendered.html
        );
        assert!(
            !rendered.html.contains("行内调用收到块级输出"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.unknown, 1);
    }

    #[test]
    fn inline_problem_can_be_written_directly() {
        let rendered = render(&[], "前 ::problem{这里不能这么写} 后\n");
        assert!(
            rendered
                .html
                .contains("行内内容有问题：problem。这里不能这么写"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.misused, 1);
    }

    #[test]
    fn inline_problem_takes_why_from_params() {
        let rendered = render(&[], "前 ::problem[why=参数说明] 后\n");
        assert!(rendered.html.contains("参数说明"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 1);
    }

    #[test]
    fn an_unknown_inline_call_with_params_still_reports_unknown() {
        let rendered = render(&[], "前 ::不存在[why=随便] 后\n");
        assert!(
            rendered.html.contains("未知块：不存在"),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.problems.unknown, 1);
        assert_eq!(rendered.problems.misused, 0);
    }

    #[test]
    fn a_naked_call_stays_a_block_call() {
        let rendered = render(&[("段落", "::element tag=span\n  一段\n")], "::段落\n");
        assert!(rendered.html.contains("<p><span>"), "{}", rendered.html);
        assert!(rendered.html.contains("一段"), "{}", rendered.html);
    }

    #[test]
    fn inline_calls_work_in_headings_and_table_cells() {
        let rendered = render(
            &[("小", "::element tag=span\n  {{body}}\n")],
            "# 标 ::小[]{题}\n\n| 列 |\n| --- |\n| ::小[]{格} |\n",
        );
        assert!(rendered.html.contains("标 <span>"), "{}", rendered.html);
        assert!(rendered.html.contains("题"), "{}", rendered.html);
        assert!(rendered.html.contains("<td><span>"), "{}", rendered.html);
        assert!(rendered.html.contains("格"), "{}", rendered.html);
        assert_eq!(rendered.problems.misused, 0);
    }

    #[test]
    fn escaped_braces_stay_literal() {
        let rendered = render(
            &[("花括号", "::element tag=span\n  {{body}}\n")],
            "::花括号{甲 \\{ 乙 \\}}\n",
        );
        assert!(rendered.html.contains("甲 { 乙 }"), "{}", rendered.html);
    }
}
