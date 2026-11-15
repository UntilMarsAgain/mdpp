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
use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

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
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .one(body.as_bytes());
    let mut out = Vec::new();
    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        let mut heading = false;
        if let NodeData::Element { name, attrs, .. } = &node.data {
            let level = match name.local.as_ref() {
                "h2" => 2,
                "h3" => 3,
                _ => 0,
            };
            if level != 0 {
                heading = true;
                let id = attrs
                    .borrow()
                    .iter()
                    .find(|attr| attr.name.local.as_ref() == "id")
                    .map(|attr| attr.value.to_string());
                if let Some(id) = id {
                    let label = text_content(&node);
                    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !id.trim().is_empty() && !label.is_empty() {
                        out.push(Entry { level, id, label });
                    }
                }
            }
        }
        // 标题里不会再套标题；文字已经由 `text_content` 收过了，不必再往下走。
        if !heading {
            for child in node.children.borrow().iter().rev() {
                stack.push(child.clone());
            }
        }
    }
    out
}

/// 元素里所有文字节点拼起来（`<code>` 这类行内标签也在内）。
///
/// 用显式栈：正文里的 HTML 深浅不受 markdown 约束。
fn text_content(root: &Handle) -> String {
    let mut out = String::new();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        match &node.data {
            NodeData::Text { contents } => out.push_str(&contents.borrow()),
            NodeData::Element { name, .. }
                if matches!(name.local.as_ref(), "script" | "style" | "template") =>
            {
                continue;
            }
            _ => {}
        }
        for child in node.children.borrow().iter().rev() {
            stack.push(child.clone());
        }
    }
    out
}
