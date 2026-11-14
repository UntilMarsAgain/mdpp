//! `::html` 的净化器：按 HTML5 规范解析，再按白名单逐个节点重建。
//!
//! ## 为什么用规范解析器而不是手写扫描
//!
//! 绕过手法远不止 `<script>` / `on*=` / `javascript:` 三样：大小写与属性顺序、
//! `java\nscript:`、`<svg>` 里的外来内容、`srcdoc`、畸形标签让净化器与浏览器对
//! 同一段文本产生不同理解。手写扫描补不完这些。
//!
//! 用 [`html5ever`]（WHATWG HTML5 规范）先把 HTML 变成树，再按白名单走一遍、把树
//! 重新拼成字符串，于是三件事同时成立：
//!
//! - 白名单外的东西出不去（默认方向是关，不是开）；
//! - 输出是自己拼的，属性值与文本都重新转义过；
//! - 与浏览器看的是同一棵树。
//!
//! ## 白名单的取舍
//!
//! **整棵子树丢掉**：`script` / `style` / `link` / `meta` / `base` / `object` /
//! `embed` / `applet` / `frame` / `frameset` / `form` 及其控件 / `template` /
//! `noscript` / `svg` / `math`。最后两个是**外来内容**（foreign content）—— 它们
//! 有自己的一套命名空间规则，不是因为「画图」才丢。
//!
//! **不保留 `style` 属性**：内联样式能 `url()` 出去、能藏东西。要样式用 `::css`。
//!
//! **URL 只认 `http` / `https` / `mailto`** 与相对地址（判定前先按浏览器的规矩把协议
//! 那一段里的空白与控制字符去掉）。
//!
//! **不给 `id`，也不给 `data-*`**：`id` 撞了就串 —— 同一页里两处都写 `id="x"`，
//! 锚点落谁身上说不清；`data-*` 是给脚本挂记号用的，正文作者不必有。锚点与查找
//! 一律走 `class`（白名单给的是 `class` / `title` / `lang` / `dir`，见 `RULES`）。
//!
//! ## `<markdown>`：让 HTML 里反过来用本库的方言
//!
//! ```text
//! ::html
//!   <div class="card">
//!     <markdown>
//!     这里仍然是 **markdown**：可以写列表、写 [[内部链接]]，
//!     写 ::note 提示框也可以。
//!     </markdown>
//!   </div>
//! ```
//!
//! 做法是**先把 `<markdown>` 那几段从原文里取出来**（原样，一个字节不改），在原处
//! 放一个占位记号；余下的照常解析、净化；拼字符串时碰到占位记号，就把那一段真的
//! 渲染一遍 markdown 塞进去。
//!
//! 不能改成「解析完再从树里取」：那等于让 HTML 解析器先把 markdown 当 HTML 过一遍
//! —— `&` 会被解成实体、`<div>` 会被当成标签吃掉、缩进与空行的意义全变了。

use std::fmt::Write;

use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

mod escape;
mod markdown;
mod rules;

#[cfg(test)]
mod tests;

use escape::{escape_attribute, escape_text};
use markdown::{SLOT_CLOSE, SLOT_OPEN, lift_markdown};
use rules::{BOOLEAN, allowed, finish_a_attrs, is_void, keep_attribute, skip_subtree};

const MAX_NESTING: usize = 8;

/// 净化一段 HTML（`::html` 的正文）。
///
/// `render_markdown` 是 `<markdown>` 那几段的出口：净化器手上只有一段源码和一个输出
/// 缓冲，解析器由调用方递进来。
pub fn sanitize(source: &str, render_markdown: &dyn Fn(&str) -> String) -> String {
    let (skeleton, slots) = lift_markdown(source);
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .one(skeleton.as_bytes());
    let mut out = String::with_capacity(skeleton.len() + 64);
    walk(&dom.document, &render_markdown, &slots, &mut out);
    out
}

/// 净化，但**不展开** `<markdown>`：正文里写了那一段，输出里就没有它。
pub fn sanitize_without_markdown(source: &str) -> String {
    sanitize(source, &|_| String::new())
}

// ---------------------------------------------------------------- <markdown> 那一段

/// 占位记号的两头：私有区的字符，HTML 文本里不会自然出现，也躲得开实体解码。
struct Frame {
    node: Handle,
    /// 有值表示这是「该写闭合标签」的那一帧。
    close: Option<String>,
}

/// 遍历解析树并重新拼出净化后的 HTML。
///
/// 用显式栈而不是递归：HTML 的嵌套深度不受 markdown 的 `MAX_NESTING` 约束，
/// 一段几万层深的 `<div>` 就能把递归遍历的调用栈打爆。显式栈只吃堆内存。
fn walk(handle: &Handle, render: &dyn Fn(&str) -> String, slots: &[String], out: &mut String) {
    let mut stack = vec![Frame {
        node: handle.clone(),
        close: None,
    }];
    while let Some(frame) = stack.pop() {
        if let Some(close) = frame.close {
            out.push_str(&close);
            continue;
        }
        match &frame.node.data {
            NodeData::Document => push_children(&frame.node, &mut stack),

            // 注释与 doctype 一律不要：注释能藏条件渲染的 payload，doctype 只对老页面有意义
            NodeData::Comment { .. } | NodeData::Doctype { .. } => {}

            NodeData::Text { contents } => {
                write_text(&contents.borrow(), render, slots, out);
            }

            NodeData::Element { name, attrs, .. } => {
                let tag: String = name.local.to_string();
                // 先按小写找：HTML 标签名不区分大小写，而解析器已经把本地名归一过了
                if skip_subtree(&tag) {
                    continue;
                }
                let Some(rules) = allowed(&tag) else {
                    // 不在白名单里的标签：**标签本身丢掉，children 留下**。
                    // 这是净化器的常态 —— 作者写了个没人认得的自定义标签，里面的正文
                    // 不该跟着一起消失。
                    push_children(&frame.node, &mut stack);
                    continue;
                };
                let open = format!("<{tag}");
                out.push_str(&open);
                let mut kept_attrs: Vec<(String, String)> = Vec::new();
                for attr in attrs.borrow().iter() {
                    let attr_name: String = attr.name.local.to_string();
                    let attr_name = attr_name.to_ascii_lowercase();
                    let Some(kept) = keep_attribute(&attr_name, rules, &attr.value) else {
                        continue;
                    };
                    kept_attrs.push((attr_name, kept));
                }
                // `target="_blank"` 的新窗口要带上 `noopener`，否则被打开的页面
                // 能通过 `window.opener` 反向操作这一页。现代浏览器默认有，
                // 但产物不该把这件事交给浏览器版本。
                finish_a_attrs(&tag, &mut kept_attrs);
                for (attr_name, kept) in kept_attrs {
                    // 布尔属性只写名字：`<details open>` 不是 `<details open="">`，
                    // 而输出是我们自己拼的，就拼成作者写下的样子
                    if BOOLEAN.contains(&attr_name.as_str()) {
                        let _ = write!(out, " {attr_name}");
                    } else {
                        let _ = write!(out, " {attr_name}=\"{}\"", escape_attribute(&kept));
                    }
                }
                out.push('>');
                if !is_void(&tag) {
                    stack.push(Frame {
                        node: frame.node.clone(),
                        close: Some(format!("</{tag}>")),
                    });
                    push_children(&frame.node, &mut stack);
                }
            }

            // ProcessingInstruction 之类：HTML 里不该出现，丢了
            _ => {}
        }
    }
}

fn push_children(node: &Handle, stack: &mut Vec<Frame>) {
    let children = node.children.borrow();
    for child in children.iter().rev() {
        stack.push(Frame {
            node: child.clone(),
            close: None,
        });
    }
}

/// 文本节点：把占位记号认出来，认出来了就把那一段 markdown 真的渲染一遍塞进去。
fn write_text(text: &str, render: &dyn Fn(&str) -> String, slots: &[String], out: &mut String) {
    let mut rest = text;
    loop {
        let Some(open) = rest.find(SLOT_OPEN) else {
            out.push_str(&escape_text(rest));
            return;
        };
        let after = &rest[open + SLOT_OPEN.len_utf8()..];
        let Some(close) = after.find(SLOT_CLOSE) else {
            out.push_str(&escape_text(rest));
            return;
        };
        out.push_str(&escape_text(&rest[..open]));
        let index: String = after[..close].to_string();
        match index
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|i| slots.get(i))
        {
            Some(body) => render_slot(body, render, out),
            None => {
                // 记号对不上任何一段：当普通文字留着，别把作者写的东西弄丢。
                //
                // 切到 `SLOT_CLOSE` 的**末尾**，不是它后面一个字节：那个记号有 3 个
                // 字节，写 `+ 1` 会把终点切进它中间，而 `&str` 切在字符中间是当场 panic
                // —— 正文里自己写一对 `U+E000` / `U+E001` 就够触发了
                out.push_str(&escape_text(
                    &rest[open..open + SLOT_OPEN.len_utf8() + close + SLOT_CLOSE.len_utf8()],
                ));
            }
        }
        rest = &after[close + SLOT_CLOSE.len_utf8()..];
    }
}

/// 真的把那一段 markdown 渲染成 HTML，原样写进 `out`（**不再转义** —— 它已经是 HTML）。
fn render_slot(body: &str, render: &dyn Fn(&str) -> String, out: &mut String) {
    // 层数上限：模板自己套自己时在这里停
    if crate::context::depth() >= MAX_NESTING {
        crate::telemetry::warn!(limit = MAX_NESTING, "<markdown> 嵌套超过上限");
        out.push_str(&stopped(&format!(
            "&lt;markdown&gt; 套了超过 {MAX_NESTING} 层，已经停在这里（是不是自己套自己？）"
        )));
        return;
    }
    let html = crate::context::deeper(|| render(body));
    out.push_str(&html);
}

/// 「停在这里」那个框。**已经排好版的 HTML**，所以几处 `&lt;` 是直接写进去的。
fn stopped(why: &str) -> String {
    format!(
        "<div class=\"mdpp-template mdpp-template--problem\"><p class=\"mdpp-template__why\">{why}</p></div>"
    )
}
