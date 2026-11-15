//! `::html` 的净化器：交给 [`ammonia`]，先按 HTML5 规范解析，再按白名单重建。
//!
//! ## 为什么用现成的净化器
//!
//! 上一版是「html5ever 解析 + 自己拼 HTML」。解析这一步是对的，但拼的那一段要自己
//! 记住转义、布尔属性、空元素、`target="_blank"` 的 `rel`、每个标签的属性白名单 ——
//! 这些事 [`ammonia`] 已经做过一遍，而且跟着 HTML 规范与安全报告在维护。
//!
//! 白名单本身仍然在这里（`rules` 模块）：哪些标签、每个标签能带哪些属性、哪些标签连
//! 内容一起丢，是 Markdown++ 的产品决定，不是库能替我们定的。
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
//! 放一个自定义元素 `<mdpp-markdown data-i="N"></mdpp-markdown>`；余下的照常净化；
//! 净化完把占位元素换成真正渲染出来的 markdown，再净化一遍。
//!
//! 展开放在第一次净化之后，槽位输出因此也会过一遍白名单 —— `raw_html` 打开时也不
//! 例外。第二次净化同时把伪造的、没展开的占位元素拆掉。
//!
//! 用元素而不是文本记号：展开是字符串替换，若记号能出现在属性值里，属性值就会被
//! 替换成 HTML，等于多开一个注入口。元素名只出现在标签位置，属性值里的 `<` 会被
//! 序列化成 `&lt;`，匹配不上。

use std::collections::HashSet;

use ammonia::{Builder, UrlRelative};
use html5ever::serialize::{SerializeOpts, TraversalScope, serialize};
use html5ever::tendril::TendrilSink;
use html5ever::{Attribute, ParseOpts, QualName, local_name, ns};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

mod markdown;
mod rules;

#[cfg(test)]
mod tests;

use markdown::lift_markdown;

const MAX_NESTING: usize = 8;

/// 净化一段 HTML（`::html` 的正文）。
///
/// `render_markdown` 是 `<markdown>` 那几段的出口：净化器手上只有一段源码和一个输出
/// 缓冲，解析器由调用方递进来。
pub fn sanitize(source: &str, render_markdown: &dyn Fn(&str) -> String) -> String {
    let (skeleton, slots) = lift_markdown(source);
    // 第一遍：占位元素在允许名单里，于是它们能穿过这一遍
    let skeleton = purify(&skeleton, true);
    // 展开：把每个 `data-i` 换成真正渲染出来的 markdown
    let mut expanded = String::with_capacity(skeleton.len() + 64);
    let mut rest = skeleton.as_str();
    while let Some(at) = rest.find("<mdpp-markdown ") {
        expanded.push_str(&rest[..at]);
        let tail = &rest[at..];
        let index = read_marker(tail).and_then(|i| slots.get(i));
        let Some(body) = index else {
            // 认不出来：留着，让第二遍净化把它拆掉
            expanded.push_str("<mdpp-markdown ");
            rest = &tail["<mdpp-markdown ".len()..];
            continue;
        };
        expanded.push_str(&render_slot(body, render_markdown));
        let end = tail.find('>').map(|i| i + 1).unwrap_or(tail.len());
        rest = &tail[end..];
    }
    expanded.push_str(rest);
    // 第二遍：占位元素不在允许名单里，槽位输出与伪造的占位元素都在这里收尾
    let cleaned = purify(&expanded, false);
    add_noopener(&cleaned)
}

/// 净化，但**不展开** `<markdown>`：正文里写了那一段，输出里就没有它。
pub fn sanitize_without_markdown(source: &str) -> String {
    sanitize(source, &|_| String::new())
}

/// 用 ammonia 按 [`rules`] 的白名单净化一遍。
///
/// `allow_marker` 只给第一遍用：第二遍要把还没展开的 `<mdpp-markdown>` 当陌生标签
/// 拆掉，只留里面的东西（正常情况下里面是空的）。
fn purify(source: &str, allow_marker: bool) -> String {
    let mut builder = Builder::default();
    builder
        .tags(rules::tags(allow_marker))
        .tag_attributes(rules::attributes(allow_marker))
        .generic_attributes(HashSet::new())
        .url_schemes(["http", "https", "mailto"].into_iter().collect())
        .url_relative(UrlRelative::PassThrough)
        // 默认会把 `rel="noopener noreferrer"` 加到所有 `<a>` 上，而作者写的
        // `rel` 已经在属性表里，两者同时设置会 panic；新窗口的 `rel` 由
        // [`add_noopener`] 在最后补，只补 `target="_blank"` 的那些。
        .link_rel(None)
        .clean_content_tags(rules::clean_content())
        .strip_comments(true)
        .attribute_filter(rules::filter_attribute);
    builder.clean(source).to_string()
}

/// 从 `<mdpp-markdown data-i="N"` 读出 `N`。
fn read_marker(tail: &str) -> Option<usize> {
    let rest = tail.strip_prefix("<mdpp-markdown ")?;
    let rest = rest.strip_prefix("data-i=\"")?;
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

/// 展开一个槽位：把那段 markdown 渲染出来，**不再转义**（它已经是 HTML）。
fn render_slot(body: &str, render: &dyn Fn(&str) -> String) -> String {
    // 层数上限：模板自己套自己时在这里停
    if crate::context::depth() >= MAX_NESTING {
        crate::telemetry::warn!(limit = MAX_NESTING, "<markdown> 嵌套超过上限");
        return stopped(&format!(
            "<markdown> 套了超过 {MAX_NESTING} 层，已经停在这里（是不是自己套自己？）"
        ));
    }
    crate::context::deeper(|| render(body))
}

/// 「停在这里」那个框：写框与计数都在这里，原因由 [`problem_html`] 转义。
fn stopped(why: &str) -> String {
    crate::env::note_problem(
        "块的用法有问题",
        crate::block::dispatch::MISUSED_CLASS,
        "markdown",
        why,
    );
    crate::env::misused_problem();
    crate::block::dispatch::problem_html(crate::block::dispatch::MISUSED_CLASS, why)
}

// ---------------------------------------------------------------- noopener

/// `target="_blank"` 的 `a` 补上 `rel="noopener noreferrer"`；已有的 `rel`
/// 保留原词，只补缺的那两个。
///
/// ammonia 只能给**所有**链接加 `rel`，不能按 `target` 挑；这里用同一棵 html5ever
/// 树补完再序列化，输出因此仍是规范解析器的结果。
fn add_noopener(html: &str) -> String {
    if !html.contains("target=\"_blank\"") {
        return html.to_string();
    }
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .one(html.as_bytes());

    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data
            && name.local.as_ref() == "a"
        {
            let mut attrs = attrs.borrow_mut();
            let blank = attrs
                .iter()
                .any(|attr| attr.name.local.as_ref() == "target" && &*attr.value == "_blank");
            if blank {
                adjust_rel(&mut attrs);
            }
        }
        for child in node.children.borrow().iter() {
            stack.push(child.clone());
        }
    }

    let body = body_of(&dom.document).unwrap_or_else(|| dom.document.clone());
    let mut bytes = Vec::new();
    serialize(
        &mut bytes,
        &SerializableHandle::from(body),
        SerializeOpts {
            traversal_scope: TraversalScope::ChildrenOnly(None),
            ..SerializeOpts::default()
        },
    )
    .expect("序列化到内存不应失败");
    String::from_utf8(bytes).expect("html5ever 的输出必定是 UTF-8")
}

/// 给一个 `<a>` 的 `rel` 补 `noopener` / `noreferrer`。
fn adjust_rel(attrs: &mut Vec<Attribute>) {
    let Some(index) = attrs
        .iter()
        .position(|attr| attr.name.local.as_ref() == "rel")
    else {
        attrs.push(Attribute {
            name: QualName::new(None, ns!(html), local_name!("rel")),
            value: "noopener noreferrer".into(),
        });
        return;
    };
    let mut tokens: Vec<String> = attrs[index]
        .value
        .split_whitespace()
        .map(str::to_string)
        .collect();
    for wanted in ["noopener", "noreferrer"] {
        if !tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case(wanted))
        {
            tokens.push(wanted.to_string());
        }
    }
    attrs[index].value = tokens.join(" ").into();
}

/// 取 `<body>`；html5ever 总会补出 `html` / `body`，取不到时退回整棵树。
fn body_of(document: &Handle) -> Option<Handle> {
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Element { name, .. } = &node.data
            && name.local.as_ref() == "body"
        {
            return Some(node);
        }
        for child in node.children.borrow().iter() {
            stack.push(child.clone());
        }
    }
    None
}
