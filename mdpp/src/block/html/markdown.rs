use std::fmt::Write;

pub(super) const SLOT_OPEN: char = '\u{E000}';
pub(super) const SLOT_CLOSE: char = '\u{E001}';

/// 把 `<markdown>…</markdown>` 那几段取出来，原处留下占位记号。
///
/// 返回 `(骨架, 各段的原文)`。**取出来的是原样的字节** —— markdown 还要回到自己的
/// 解析器手里，任何「顺手规范化一下」都是错的。没有闭合标签时那一段当普通文字留着。
pub(super) fn lift_markdown(source: &str) -> (String, Vec<String>) {
    let lower = source.to_ascii_lowercase();
    let mut out = String::with_capacity(source.len());
    let mut slots: Vec<String> = Vec::new();
    let mut cursor = 0usize;

    while let Some(open) = find_tag(&lower, cursor, "markdown") {
        let (body_from, close_tag) = match find_close(&lower, open) {
            Some(pair) => pair,
            None => break,
        };
        // 开标签本身整段丢掉（含它那些属性）：`<markdown>` 不该有属性
        let body_end = close_tag;
        let body = source[body_from..body_end].to_string();
        let slot = slots.len();
        slots.push(body);

        out.push_str(&source[cursor..open]);
        let _ = write!(out, "{SLOT_OPEN}{slot}{SLOT_CLOSE}");
        cursor = body_end;
    }
    out.push_str(&source[cursor..]);
    (out, slots)
}

/// 从 `from` 起找 `<markdown` 这个开标签，返回它 `<` 的位置。
fn find_tag(haystack: &str, from: usize, name: &str) -> Option<usize> {
    let mut at = from;
    while let Some(found) = haystack[at..].find('<') {
        let start = at + found;
        let rest = &haystack[start + 1..];
        if !rest.starts_with(name) {
            at = start + 1;
            continue;
        }
        // 标签名后面必须是空白、`>` 或 `/` —— 否则那是 `<markdowner>` 之类
        let after = rest[name.len()..].chars().next();
        if matches!(
            after,
            None | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some('>')
        ) || rest[name.len()..].starts_with('/')
        {
            return Some(start);
        }
        at = start + 1;
    }
    None
}

/// `<markdown>` 的正文从哪儿开始，以及与它配对的那个 `</markdown…>` 的 `<` 在哪儿。
///
/// **要数层数**：`<markdown>` 里可以再写 `<markdown>`，只找第一个闭合标签会把内层的
/// 开标签当正文、把外层的闭合当结束。
fn find_close(haystack: &str, open: usize) -> Option<(usize, usize)> {
    // 从开标签**之后**开始数，否则开标签被数两次，`depth` 永远归不了零
    let body_from = skip_tag(haystack, open);
    let mut depth = 0usize;
    let mut at = body_from;
    loop {
        let next_open = find_tag(haystack, at, "markdown");
        let next_close = find_tag(haystack, at, "/markdown");
        // 哪个先来就走哪个。拆成四臂而不用 `o < c` 的守卫，是为了在错误信息里
        // 看得清「四种组合都覆盖了」
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                at = skip_tag(haystack, o);
            }
            (Some(o), None) => {
                depth += 1;
                at = skip_tag(haystack, o);
            }
            // 闭合标签在前：出一层；`depth` 归零的那个就是与最外层配对的那个
            (_, Some(c)) => {
                at = c;
                if depth == 0 {
                    return Some((body_from, at));
                }
                depth -= 1;
                at = skip_tag(haystack, c);
            }
            (None, None) => return None,
        }
    }
}

/// 跳过一个标签：越过它的 `>`。
///
/// 找 `>` 时**要跳过属性里的引号** —— `class="a>b"` 里那个 `>` 不是标签的结束。
fn skip_tag(haystack: &str, start: usize) -> usize {
    let bytes = haystack.as_bytes();
    let mut at = start + 1;
    let mut quote: Option<u8> = None;
    while at < bytes.len() {
        let byte = bytes[at];
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return at + 1,
            None => {}
        }
        at += 1;
    }
    haystack.len()
}

// ---------------------------------------------------------------- 走树
