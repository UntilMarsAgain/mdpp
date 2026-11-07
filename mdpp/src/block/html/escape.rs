/// 文本节点的转义：`&` `<` `>`。
pub(super) fn escape_text(text: &str) -> String {
    html_escape::encode_text(text).into_owned()
}

/// 属性值的转义：在文本那三样之外再管 `"`（我们用双引号包值）。
pub(super) fn escape_attribute(value: &str) -> String {
    html_escape::encode_double_quoted_attribute(value).into_owned()
}
