//! 链接目标里的空格补成 `%20`。
//!
//! CommonMark 规定链接目标**不能有空格** —— `![屏幕截图 2026.png](屏幕截图 2026.png)`
//! 会被整行当普通文字渲染（要写就得包成 `<…>` 或写成 `%20`）。可是文件名里有空格太常见
//! 了：截图默认就叫这个。作者照自己看见的名字写，就应当能用。
//!
//! 只动链接/图片的目标（`](…`），正文一个字不改；围栏代码块与行内代码里一个字不改
//! —— 那里的写法是给人看的例子，改了就成了说谎。
//!
//! 认不出的写法原样留着（返回 `None`）：尖括号写法本来就对、跨行的目标不碰、带引号的
//! 多半还挂着标题 —— 那几种交给标准解析器。

use crate::raw::raw_zones;

/// 把一段 markdown 里链接目标中的空格补成 `%20`。
///
/// 原始区（围栏、缩进代码、HTML 块、行内代码）由 [`raw_zones`] 给出真实字节
/// 区间：那是一次探针解析的 `srcmap`，块引用与列表前缀也算在里面。区间内一个
/// 字都不改，区间外才走 [`encode_line_targets`]。
pub fn encode_target_spaces(markdown: &str) -> String {
    // 绝大多数正文没有链接/图片目标：省掉一次探针解析
    if !markdown.contains("](") {
        return markdown.to_string();
    }
    let zones = raw_zones(markdown);
    let mut out = String::with_capacity(markdown.len());
    let mut cursor = 0usize;

    for zone in zones {
        let start = zone.start.min(markdown.len());
        let end = zone.end.min(markdown.len());
        if start < cursor {
            continue;
        }
        if start > cursor {
            out.push_str(&encode_span(&markdown[cursor..start]));
        }
        if end > start {
            out.push_str(&markdown[start..end]);
        }
        cursor = end;
    }
    if cursor < markdown.len() {
        out.push_str(&encode_span(&markdown[cursor..]));
    }
    out
}

/// 区间外那段：按行处理。探针给的区间可能落在行中间（行内代码），
/// 所以不能假设切出来的段一定从行首开始。
fn encode_span(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        out.push_str(&encode_line_targets(line));
    }
    out
}

/// 一行里的目标（`](…)`）补空格编码；行内代码整段跳过
fn encode_line_targets(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut index = 0;

    while index < chars.len() {
        // 行内代码：连反引号一起原样搬走（里面的 `](` 不是目标）
        if chars[index] == '`' {
            let run = chars[index..].iter().take_while(|ch| **ch == '`').count();
            let close = (index + run..chars.len())
                .find(|at| chars[*at..].iter().take_while(|ch| **ch == '`').count() >= run);
            match close {
                Some(close) => {
                    let end = close + chars[close..].iter().take_while(|ch| **ch == '`').count();
                    out.extend(&chars[index..end]);
                    index = end;
                    continue;
                }
                None => {
                    // 没闭合：后面都当普通文字
                    out.extend(&chars[index..]);
                    break;
                }
            }
        }

        // `](` —— 图片与链接的目标都从这里开始
        if chars[index] == ']' && chars.get(index + 1) == Some(&'(') {
            if let Some((end, target)) = encode_target(&chars, index + 2) {
                out.push_str("](");
                out.push_str(&target);
                out.push(')');
                index = end + 1;
                continue;
            }
        }

        out.push(chars[index]);
        index += 1;
    }
    out
}

/// 认出一个目标（`(` 之后到配对的 `)`），把里面的空格编码掉。
///
/// 认不出就返回 `None`（原样留着）。
fn encode_target(chars: &[char], start: usize) -> Option<(usize, String)> {
    if chars.get(start) == Some(&'<') {
        return None;
    }

    let mut text = String::new();
    let mut level = 0usize;
    let mut index = start;
    while index < chars.len() {
        match chars[index] {
            '\n' | '\r' => return None,
            '\'' | '"' => return None,
            '(' => level += 1,
            ')' if level == 0 => break,
            ')' => level -= 1,
            _ => {}
        }
        text.push(chars[index]);
        index += 1;
    }

    // 没闭合、没有空格、或者一上来就是空白：都不是我们要处理的
    if index >= chars.len() || !text.contains(' ') || text.starts_with(char::is_whitespace) {
        return None;
    }
    Some((index, text.replace(' ', "%20")))
}

#[cfg(test)]
mod tests {
    use super::encode_target_spaces;

    /// 作者照文件名写，就该渲染成图片
    #[test]
    fn spaces_in_targets_become_percent_twenty() {
        assert_eq!(
            encode_target_spaces("![屏幕截图 20260925 144347.png](屏幕截图 20260925 144347.png)\n"),
            "![屏幕截图 20260925 144347.png](屏幕截图%2020260925%20144347.png)\n"
        );
        assert_eq!(
            encode_target_spaces("[看看](我的 图.png)\n"),
            "[看看](我的%20图.png)\n"
        );
    }

    /// 代码里的写法**一个不改**：那是给人看的例子
    #[test]
    fn code_keeps_its_literals() {
        assert_eq!(
            encode_target_spaces("```\n![a](b c.png)\n```\n"),
            "```\n![a](b c.png)\n```\n"
        );
        assert_eq!(
            encode_target_spaces("写法是 `![a](b c.png)` 这样\n"),
            "写法是 `![a](b c.png)` 这样\n"
        );
    }

    /// 本来就对、或者不能确定意图的写法：原样留着
    #[test]
    fn other_targets_are_left_alone() {
        // 尖括号写法标准解析器自己会处理
        assert_eq!(encode_target_spaces("![a](<b c.png>)"), "![a](<b c.png>)");
        // 没有空格的目标不必动
        assert_eq!(encode_target_spaces("![a](b.png)"), "![a](b.png)");
        // 带引号的多半挂着标题：不猜
        assert_eq!(
            encode_target_spaces("![a](b.png \"标题 字\")"),
            "![a](b.png \"标题 字\")"
        );
        // 跨行的目标不存在
        assert_eq!(encode_target_spaces("![a](b\nc.png)"), "![a](b\nc.png)");
        // 括号配对的不受影响（文件名里有括号）
        assert_eq!(
            encode_target_spaces("![a](图 (1).png)"),
            "![a](图%20(1).png)"
        );
    }

    /// `~~~` 围栏同样整个跳过 —— 只认 ``` 会漏掉它
    #[test]
    fn tilde_fences_are_skipped_too() {
        let src = "~~~\n![a](b c.png)\n~~~\n";
        assert_eq!(encode_target_spaces(src), src);
    }

    /// 块引用/列表里的围栏也是原始区 —— 只看行首三个反引号会把 `> ` 前缀漏掉
    #[test]
    fn fences_inside_container_blocks_are_skipped_too() {
        let quote = "> ```\n> ![a](b c.png)\n> ```\n";
        assert_eq!(encode_target_spaces(quote), quote);

        let list = "- 项\n  ```\n  ![a](b c.png)\n  ```\n";
        assert_eq!(encode_target_spaces(list), list);
    }

    /// 围栏外面的目标照常编码 —— 不能因为容器里有围栏就整段不动
    #[test]
    fn targets_outside_the_fence_are_still_encoded() {
        let src = "> ![a](b c.png)\n>\n> ```\n> ![a](b c.png)\n> ```\n";
        let out = encode_target_spaces(src);
        assert!(out.contains("> ![a](b%20c.png)"), "{out}");
        assert!(out.contains("> ![a](b c.png)"), "{out}");
    }
}
