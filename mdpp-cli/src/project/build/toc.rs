//! 站点模式的侧边目录。
//!
//! 目录**不要求作者手写**：正文渲染完之后，标题已经由库里的
//! `UniqueHeadingAnchors` 挂上了全页唯一的 `id`。这里只从那份 HTML 里把
//! `h2` / `h3` 挑出来。
//!
//! 不挑 `h1`：站点页面正文的第一行通常就是 `# 页面标题`，它已经进 `<title>`、
//! 也摆在文章最上面，再收进目录只是把同一句话说两遍。只写了 `h1` 的页面因此
//! 没有侧边目录 —— 那本来就是「这一页叫什么」，不是「这一页有几节」。
//!
//! 从**渲染后的 HTML** 里挑，而不是再扫一遍源码：标题可能来自模板展开、嵌套块，
//! 锚点的去重也发生在渲染阶段。产物里那份带 `id` 的 HTML 才是唯一说得准的。
//!
//! 链接全是 `#锚点`：页面内跳转，不经过站点根，于是整站挂在哪个子路径下、
//! 配没配 `site_url` 都不影响它。
//!
//! ## 为什么是两份结构
//!
//! 宽屏是一列常驻的 `<aside>`，窄屏收成 `<details>`/`<summary>`。两者由 CSS 在
//! 同一个断点切换，**不依赖 JS** —— `details` 本来就点得开，禁掉脚本也能用。
//! 同一份链接因此出现过两次，但同一时刻只有一份 `display` 不是 `none`：读屏
//! 软件与 Tab 键都只会碰到当前那一份。

use crate::render::page::escape;

/// 参与目录的标题层级。`h1` 是这一页的标题，不是章节。
const LEVELS: [(&str, u8); 2] = [("h2", 2), ("h3", 3)];

/// 一条目录项
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// 2 或 3，只用来落一个 class（缩进靠它）
    level: u8,
    /// 标题上那个锚点（已解码的原文）
    id: String,
    /// 标题的纯文本（已解码，尚未转义）
    label: String,
}

/// 正文 HTML → 侧边目录的 HTML。没有可链接的标题就返回空串 —— 空目录不该占地方。
pub(super) fn sidebar(body: &str) -> String {
    let entries = entries(body);
    if entries.is_empty() {
        return String::new();
    }

    let list = list(&entries);
    // 宽屏那一列与窄屏那个折叠入口各是一份结构。两份都在 CSS 里按同一个断点
    // 切换显示，于是没有 JS 也能在两种宽度下都成立。
    format!(
        "<aside class=\"mdpp-toc mdpp-toc--wide\">\n\
         <nav class=\"mdpp-toc__nav\" aria-label=\"本页目录\">\n\
         {list}\n\
         </nav>\n\
         </aside>\n\
         <details class=\"mdpp-toc mdpp-toc--narrow\">\n\
         <summary class=\"mdpp-toc__summary\">本页目录</summary>\n\
         <nav class=\"mdpp-toc__nav\" aria-label=\"本页目录\">\n\
         {list}\n\
         </nav>\n\
         </details>"
    )
}

/// 一份清单。宽屏与窄屏两份都用它，内容因此不会长歪。
fn list(entries: &[Entry]) -> String {
    let mut items = String::new();
    for entry in entries {
        items.push_str(&format!(
            "<li class=\"mdpp-toc__item mdpp-toc__item--h{}\"><a href=\"#{}\">{}</a></li>\n",
            entry.level,
            escape(&entry.id),
            escape(&entry.label),
        ));
    }
    format!("<ol class=\"mdpp-toc__list\">\n{items}</ol>")
}

/// 按出现次序收下所有带 `id` 的 `h2` / `h3`。
///
/// 没有 `id` 的标题（作者手写的裸 HTML）**跳过**：没有能跳过去的锚点，
/// 放进目录就是一条点了不动的链接。
fn entries(body: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some((start, tag_end, tag, level)) = next_heading(body, at) {
        let Some(close) = close_start(body, tag_end + 1, tag) else {
            // 没闭合的标题 HTML 本身就不完整；跳过它，别让后面的正文都被吞掉
            at = tag_end + 1;
            continue;
        };
        let Some(id) = attribute(&body[start..=tag_end], "id") else {
            at = close + 2;
            continue;
        };
        let id = decode(&id);
        let label = text_of(&body[tag_end + 1..close]);
        if !id.trim().is_empty() && !label.is_empty() {
            out.push(Entry { level, id, label });
        }
        at = close + 2;
    }
    out
}

/// 从 `from` 起找下一个 `<h2 …>` / `<h3 …>`，返回 `(起点, '>' 的位置, 标签名, 层级)`。
fn next_heading(html: &str, from: usize) -> Option<(usize, usize, &'static str, u8)> {
    let bytes = html.as_bytes();
    let mut at = from;
    while at < bytes.len() {
        if bytes[at] == b'<' {
            for (tag, level) in LEVELS {
                let name = tag.as_bytes();
                let end = at + 1 + name.len();
                if end <= bytes.len()
                    && bytes[at + 1..end].eq_ignore_ascii_case(name)
                    && (end == bytes.len()
                        || bytes[end].is_ascii_whitespace()
                        || bytes[end] == b'>')
                {
                    if let Some(close) = tag_end(html, end) {
                        return Some((at, close, tag, level));
                    }
                }
            }
        }
        at += 1;
    }
    None
}

/// 从 `from` 起找 `</tag`。大小写都认；后面必须是空白或 `>`（别把 `</h20>` 认成 `h2`）。
fn close_start(html: &str, from: usize, tag: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    let name = tag.as_bytes();
    let mut at = from;
    while at + 2 + name.len() <= bytes.len() {
        if bytes[at] == b'<' && bytes[at + 1] == b'/' {
            let end = at + 2 + name.len();
            if bytes[at + 2..end].eq_ignore_ascii_case(name)
                && (end == bytes.len() || bytes[end].is_ascii_whitespace() || bytes[end] == b'>')
            {
                return Some(at);
            }
        }
        at += 1;
    }
    None
}

/// 找到开标签的 `>`。引号里的 `>` 不算 —— `id="a>b"` 不能把标签截断。
fn tag_end(html: &str, from: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut quote = 0u8;
    let mut at = from;
    while at < bytes.len() {
        let byte = bytes[at];
        if quote != 0 {
            if byte == quote {
                quote = 0;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = byte;
        } else if byte == b'>' {
            return Some(at);
        }
        at += 1;
    }
    None
}

/// 从一个开标签里取某个属性（大小写不敏感）。没有这个属性就是 `None`。
fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut at = 1;
    // 跳过标签名（h2 / h3）
    while at < bytes.len()
        && !bytes[at].is_ascii_whitespace()
        && bytes[at] != b'>'
        && bytes[at] != b'/'
    {
        at += 1;
    }
    while at < bytes.len() {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= bytes.len() || bytes[at] == b'>' || bytes[at] == b'/' {
            break;
        }
        let name_start = at;
        while at < bytes.len()
            && !bytes[at].is_ascii_whitespace()
            && bytes[at] != b'='
            && bytes[at] != b'>'
            && bytes[at] != b'/'
        {
            at += 1;
        }
        let name = &tag[name_start..at];
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let mut value = None;
        if at < bytes.len() && bytes[at] == b'=' {
            at += 1;
            while at < bytes.len() && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            if at < bytes.len() && (bytes[at] == b'"' || bytes[at] == b'\'') {
                let quote = bytes[at];
                at += 1;
                let start = at;
                while at < bytes.len() && bytes[at] != quote {
                    at += 1;
                }
                value = Some(&tag[start..at]);
                if at < bytes.len() {
                    at += 1;
                }
            } else {
                let start = at;
                while at < bytes.len() && !bytes[at].is_ascii_whitespace() && bytes[at] != b'>' {
                    at += 1;
                }
                value = Some(&tag[start..at]);
            }
        }
        if name.eq_ignore_ascii_case(wanted) {
            return Some(value.unwrap_or("").to_string());
        }
    }
    None
}

/// 标题里的行内标记去掉，只留文字。标签本身不是标题的一部分。
fn text_of(inner: &str) -> String {
    let mut text = String::with_capacity(inner.len());
    let mut at = 0;
    while let Some(found) = inner[at..].find('<') {
        let start = at + found;
        text.push_str(&inner[at..start]);
        if inner[start..].starts_with("<!--") {
            if let Some(end) = inner[start + 4..].find("-->") {
                at = start + 4 + end + 3;
                continue;
            }
        }
        match tag_end(inner, start + 1) {
            Some(end) => at = end + 1,
            None => {
                at = inner.len();
                break;
            }
        }
    }
    if at < inner.len() {
        text.push_str(&inner[at..]);
    }
    let decoded = html_escape::decode_html_entities(&text);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 属性值里的实体先解回原文，再在写进 `href` / 元素文字时统一转义。
fn decode(value: &str) -> String {
    html_escape::decode_html_entities(value).into_owned()
}
