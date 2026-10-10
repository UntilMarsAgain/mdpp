//! 链接目标里的空格补成 `%20`。
//!
//! CommonMark 规定链接目标**不能有空格**：
//! `![屏幕截图 2026.png](屏幕截图 2026.png)` 会被整行当普通文字渲染，而不是图片
//! （要写就得包成 `<…>` 或写成 `%20`）。可是文件名里有空格太常见了 —— 截图默认就叫
//! 这个。作者照自己看见的名字写，就应当能用。
//!
//! ## 只动该动的地方
//!
//! - **只动链接/图片的目标**（`](…`），正文一个字不改；
//! - **跳过代码**：围栏代码块与行内代码里的字一个不改 —— 那里的写法是给人看的
//!   例子，改了就成了说谎。
//!
//! 认不出的写法原样留着（返回 `None`）：尖括号写法本来就对、跨行的目标不碰、
//! 带引号的多半还挂着标题 —— 那几种交给标准解析器，别自作聪明。

/// 把一段 markdown 里链接目标中的空格补成 `%20`。
pub fn encode_target_spaces(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    // 当前在不在围栏代码块里（记的是围栏用的记号：` 还是 ~）
    let mut fence: Option<char> = None;

    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|ch| *ch == '`' || *ch == '~');
        if let Some(marker) = marker {
            let run = trimmed.chars().take_while(|ch| *ch == marker).count();
            if run >= 3 {
                match fence {
                    Some(open) if open == marker => fence = None,
                    None => fence = Some(marker),
                    // 另一种记号：不配对，当普通一行
                    _ => {}
                }
                out.push_str(line);
                continue;
            }
        }

        if fence.is_some() {
            out.push_str(line);
        } else {
            out.push_str(&encode_line_targets(line));
        }
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
        assert_eq!(encode_target_spaces("![a](图 (1).png)"), "![a](图%20(1).png)");
    }

    /// `~~~` 围栏同样整个跳过 —— 只认 ``` 会漏掉它
    #[test]
    fn tilde_fences_are_skipped_too() {
        let src = "~~~\n![a](b c.png)\n~~~\n";
        assert_eq!(encode_target_spaces(src), src);
    }
}