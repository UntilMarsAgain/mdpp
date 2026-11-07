/// 文本节点的转义：`&` `<` `>`。
pub(super) fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 属性值的转义：文本那三样之外还要管 `"`（我们用双引号包值）。
///
/// 建在 [`escape_text`] 之上：`&<>` 那三条**只有一份**，这里只补「引号」这一条 ——
/// 先把 `&` 转成 `&amp;` 之后再处理 `"` 也不会误伤（`&amp;` 里没有引号）
pub(super) fn escape_attribute(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}
