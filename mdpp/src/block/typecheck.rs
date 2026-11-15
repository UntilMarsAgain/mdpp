//! 模板的**输出形态**：行内、单段落，还是块。
//!
//! 模板页不声明自己输出什么；调用处只说明这次需要什么（`::卡片 type=inline`
//! 或默认的块调用）。模板展开后，[`ScopedBlock`](crate::block::expand::ScopedBlock)
//! 先把产物收进缓冲区，再交给 [`classify`] 识别实际形态，最后与调用处比较：
//!
//! - 块调用收到纯行内产物：套一层 `<p>`；
//! - 块调用收到单段落 / 块产物：原样输出；
//! - 行内调用收到纯行内产物：原样输出；
//! - 行内调用收到单段落：剥掉 `<p>`，保留内容；
//! - 行内调用收到块产物：回滚并给一条行内诊断。
//!
//! 单段落单独列成 [`OutputKind::Paragraph`]，是因为 Markdown 模板的常态就是
//! 一个段落；它既有块形式（裸 `<p>`），也有行内形式（段落内部）。把它硬归成
//! block 会让“只写了一段文字”的模板无法行内调用，归成 inline 又会在块位置
//! 丢掉段落外壳。

use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// 模板调用的**期望**输出形态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TemplateType {
    /// 普通块：段落、列表、表格都可以。
    #[default]
    Block,
    /// 行内：只能落在 `<span>` / `<th>` 一类位置。
    Inline,
}

impl TemplateType {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "block" => Ok(Self::Block),
            "inline" => Ok(Self::Inline),
            other => Err(format!(
                "模板的 `type=` 只认 inline / block，写成了 `{other}`"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Inline => "inline",
        }
    }
}

/// 模板产物的**实际**形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputKind {
    /// 纯 phrasing 内容，没有段落外壳。
    Inline,
    /// 恰好一个裸 `<p>…</p>`：块形式是整段，行内形式是段落内部。
    Paragraph,
    /// 其他 flow 内容：多个段落、div、列表、表格……
    Block,
}

/// 一次输出分类的结果。
#[derive(Debug)]
pub(crate) struct Classified {
    pub kind: OutputKind,
    /// 原始（已 trim 的）HTML。
    pub html: String,
    /// 可以直接落进行内位置的 HTML；只有 Inline / Paragraph 有。
    pub inline: Option<String>,
}

/// 识别一段模板产物的实际形态。
///
/// 空内容按纯行内空串处理。`<head>` 里的内容（script/style 等）不是正文，直接
/// 报错，避免模板把脚本塞到页面中间。
pub(crate) fn classify(html: &str) -> Result<Classified, String> {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Ok(Classified {
            kind: OutputKind::Inline,
            html: String::new(),
            inline: Some(String::new()),
        });
    }

    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .one(trimmed.as_bytes());
    if let Some(head) = find_head(&dom.document)
        && head
            .children
            .borrow()
            .iter()
            .any(|node| !ignorable(node) && !is_asset_placeholder(node))
    {
        return Err(
            "模板只能输出正文内容，不能输出 `<script>` / `<style>` 这类 `<head>` 里的标签"
                .to_string(),
        );
    }
    let body = find_body(&dom.document).ok_or_else(|| "模板的产物里没有正文".to_string())?;
    let nodes: Vec<Handle> = body
        .children
        .borrow()
        .iter()
        .filter(|node| !ignorable(node) && !is_asset_placeholder(node))
        .cloned()
        .collect();

    if nodes.is_empty() {
        return Ok(Classified {
            kind: OutputKind::Inline,
            html: String::new(),
            inline: Some(String::new()),
        });
    }

    // 单段落是 Markdown 模板的常态：块形式保留 <p>，行内形式剥掉外壳。
    if let [only] = nodes.as_slice()
        && let NodeData::Element { name, attrs, .. } = &only.data
        && name.local.as_ref() == "p"
    {
        if !attrs.borrow().is_empty() || has_block_descendant(only) {
            return Ok(block(trimmed));
        }
        let Some(inner) = trimmed
            .strip_prefix("<p>")
            .and_then(|rest| rest.strip_suffix("</p>"))
        else {
            return Ok(block(trimmed));
        };
        if inner.contains("<p") || inner.contains("</p>") {
            return Ok(block(trimmed));
        }
        return Ok(Classified {
            kind: OutputKind::Paragraph,
            html: trimmed.to_string(),
            inline: Some(inner.trim().to_string()),
        });
    }

    for node in &nodes {
        match &node.data {
            NodeData::Text { contents } => {
                if contents.borrow().trim().is_empty() {
                    continue;
                }
            }
            NodeData::Comment { .. } => {}
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref();
                if !is_phrasing(tag) || has_block_descendant(node) {
                    return Ok(block(trimmed));
                }
            }
            _ => {}
        }
    }

    Ok(Classified {
        kind: OutputKind::Inline,
        html: trimmed.to_string(),
        inline: Some(trimmed.to_string()),
    })
}

fn block(html: &str) -> Classified {
    Classified {
        kind: OutputKind::Block,
        html: html.to_string(),
        inline: None,
    }
}

/// 检查一段模板产物能否放进行内位置。
///
/// 成功时返回可以直接写进行内的 HTML：
/// - 一个裸 `<p>…</p>`：剥掉外壳；
/// - 纯行内内容：原样返回；
/// - 空内容：空串。
///
/// 失败时返回一句给作者看的原因。这里只做结构判断，不负责净化 —— 产物已经过
/// 各块自己的转义/净化。
pub(crate) fn check_inline(html: &str) -> Result<String, String> {
    let classified = classify(html)?;
    match classified.kind {
        OutputKind::Inline | OutputKind::Paragraph => Ok(classified.inline.unwrap_or_default()),
        OutputKind::Block => Err(
            "行内模板只能输出行内内容；单段裸 `<p>` 可以剥壳，其余块级内容应使用块级位置"
                .to_string(),
        ),
    }
}

fn find_head(node: &Handle) -> Option<Handle> {
    if let NodeData::Element { name, .. } = &node.data
        && name.local.as_ref() == "head"
    {
        return Some(node.clone());
    }
    for child in node.children.borrow().iter() {
        if let Some(head) = find_head(child) {
            return Some(head);
        }
    }
    None
}

fn find_body(node: &Handle) -> Option<Handle> {
    if let NodeData::Element { name, .. } = &node.data
        && name.local.as_ref() == "body"
    {
        return Some(node.clone());
    }
    for child in node.children.borrow().iter() {
        if let Some(body) = find_body(child) {
            return Some(body);
        }
    }
    None
}

/// 空白的文字与注释不参与结构判断。
fn ignorable(node: &Handle) -> bool {
    match &node.data {
        NodeData::Text { contents } => contents.borrow().trim().is_empty(),
        NodeData::Comment { .. } => true,
        _ => false,
    }
}

fn has_block_descendant(node: &Handle) -> bool {
    let mut stack: Vec<Handle> = node.children.borrow().iter().cloned().collect();
    while let Some(child) = stack.pop() {
        if is_asset_placeholder(&child) {
            continue;
        }
        if let NodeData::Element { name, .. } = &child.data
            && !is_phrasing(name.local.as_ref())
        {
            return true;
        }
        stack.extend(child.children.borrow().iter().cloned());
    }
    false
}

/// 变体作用域里的延迟资源是一个 `<template data-mdpp-asset=…>` 占位符。
/// 它不参与正文形态判断：否则 `::css` / `::js` 会像 `<head>` 内容一样被拒。
fn is_asset_placeholder(node: &Handle) -> bool {
    let NodeData::Element { name, attrs, .. } = &node.data else {
        return false;
    };
    name.local.as_ref() == "template"
        && attrs
            .borrow()
            .iter()
            .any(|attr| attr.name.local.as_ref() == "data-mdpp-asset")
}

/// 可以作为行内容器的标签。保守列表：不把 `button` / 表单控件算进来，
/// 它们虽然属于 phrasing content，但模板输出里不该顺手造交互控件。
fn is_phrasing(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "br"
            | "cite"
            | "code"
            | "data"
            | "del"
            | "dfn"
            | "em"
            | "i"
            | "img"
            | "ins"
            | "kbd"
            | "mark"
            | "q"
            | "rb"
            | "rp"
            | "rt"
            | "rtc"
            | "ruby"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "u"
            | "var"
            | "wbr"
    )
}

#[cfg(test)]
mod tests {
    use super::{OutputKind, check_inline, classify};

    #[test]
    fn inline_text_is_kept() {
        assert_eq!(
            check_inline("一段<strong>重点</strong>文字").unwrap(),
            "一段<strong>重点</strong>文字"
        );
    }

    #[test]
    fn one_bare_paragraph_is_a_paragraph() {
        let out = classify("<p>一段<em>文字</em></p>").unwrap();
        assert_eq!(out.kind, OutputKind::Paragraph);
        assert_eq!(out.inline.as_deref(), Some("一段<em>文字</em>"));
        assert_eq!(
            check_inline("<p>一段<em>文字</em></p>").unwrap(),
            "一段<em>文字</em>"
        );
    }

    #[test]
    fn pure_phrasing_is_inline() {
        let out = classify("<span>一行</span>").unwrap();
        assert_eq!(out.kind, OutputKind::Inline);
        assert_eq!(out.inline.as_deref(), Some("<span>一行</span>"));
    }

    #[test]
    fn flow_content_is_block() {
        for html in [
            "<div>块</div>",
            "<ul><li>项</li></ul>",
            "<p>一</p><p>二</p>",
            "<p class=\"x\">段</p>",
        ] {
            assert_eq!(classify(html).unwrap().kind, OutputKind::Block, "{html}");
        }
    }

    #[test]
    fn head_content_is_rejected() {
        assert!(check_inline("<script>alert(1)</script>").is_err());
        assert!(classify("<script>alert(1)</script>").is_err());
    }

    #[test]
    fn multiple_paragraphs_and_blocks_are_refused_inline() {
        for html in [
            "<p>一</p><p>二</p>",
            "<div>块</div>",
            "<ul><li>项</li></ul>",
            "<p class=\"x\">段</p>",
            "<span><div>夹块</div></span>",
        ] {
            assert!(check_inline(html).is_err(), "{html}");
        }
    }

    #[test]
    fn empty_is_inline_safe() {
        assert_eq!(check_inline(" \n ").unwrap(), "");
    }

    #[test]
    fn deferred_asset_placeholders_do_not_count_as_content() {
        let html = concat!(
            "<template data-mdpp-asset=\"css\" data-mdpp-group=\"g\" ",
            "data-mdpp-value=\"0\">.x {}</template>",
            "<span>行内</span>",
        );
        let out = classify(html).unwrap();
        assert_eq!(out.kind, OutputKind::Inline);
        assert_eq!(out.inline.as_deref(), Some(html));
    }
}
