//! 模板的**输出形态**：块还是行内。
//!
//! 模板页本身不带类型；类型写在调用处（`::卡片 type=inline`）。展开时把它记进
//! [`TemplateVars`](crate::block::template::TemplateVars)，模板里用 `{{@type}}` 判断；
//! 渲染完再按这个类型检查产物。
//!
//! 行内检查不是看字符串前缀，而是让 html5ever 按浏览器的方式解析一遍：Markdown
//! 段落、列表、表格在树里是什么样，就以什么样判断。只认裸 `<p>…</p>` 这一种
//! 「块外壳」，剥掉它保留里面的行内内容；其余块级元素一律拒绝。

use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// 模板调用的输出形态。
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
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .one(trimmed.as_bytes());
    if let Some(head) = find_head(&dom.document)
        && head.children.borrow().iter().any(|node| !ignorable(node))
    {
        return Err(
            "行内模板只能输出正文内容，不能输出 `<script>` / `<style>` 这类 `<head>` 里的标签"
                .to_string(),
        );
    }
    let body = find_body(&dom.document).ok_or_else(|| "行内模板的产物里没有正文".to_string())?;
    let nodes: Vec<Handle> = body
        .children
        .borrow()
        .iter()
        .filter(|node| !ignorable(node))
        .cloned()
        .collect();

    if nodes.is_empty() {
        return Ok(String::new());
    }

    // 单段落是 Markdown 的常态，也是唯一允许的“块外壳”。
    if let [only] = nodes.as_slice()
        && let NodeData::Element { name, attrs, .. } = &only.data
        && name.local.as_ref() == "p"
    {
        if !attrs.borrow().is_empty() {
            return Err("行内模板的单段落不能带属性：只接受裸 `<p>…</p>`".to_string());
        }
        if has_block_descendant(only) {
            return Err("行内模板的段落里不能夹块级元素".to_string());
        }
        let Some(inner) = trimmed
            .strip_prefix("<p>")
            .and_then(|rest| rest.strip_suffix("</p>"))
        else {
            return Err("行内模板只接受裸 `<p>…</p>` 作为单段落外壳".to_string());
        };
        if inner.contains("<p") || inner.contains("</p>") {
            return Err("行内模板只能输出一个段落".to_string());
        }
        return Ok(inner.trim().to_string());
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
                if !is_phrasing(tag) {
                    return Err(format!(
                        "行内模板只能输出行内内容，不能输出 `<{tag}>`；需要块级内容时应使用块级容器"
                    ));
                }
                if has_block_descendant(node) {
                    return Err(format!(
                        "行内模板的 `<{tag}>` 里不能夹块级元素；需要块级内容时应使用块级容器"
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(trimmed.to_string())
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
        if let NodeData::Element { name, .. } = &child.data
            && !is_phrasing(name.local.as_ref())
        {
            return true;
        }
        stack.extend(child.children.borrow().iter().cloned());
    }
    false
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
    use super::check_inline;

    #[test]
    fn inline_text_is_kept() {
        assert_eq!(
            check_inline("一段<strong>重点</strong>文字").unwrap(),
            "一段<strong>重点</strong>文字"
        );
    }

    #[test]
    fn one_bare_paragraph_is_unwrapped() {
        assert_eq!(
            check_inline("<p>一段<em>文字</em></p>").unwrap(),
            "一段<em>文字</em>"
        );
    }

    #[test]
    fn head_content_is_rejected() {
        assert!(check_inline("<script>alert(1)</script>").is_err());
    }

    #[test]
    fn multiple_paragraphs_and_blocks_are_refused() {
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
}
