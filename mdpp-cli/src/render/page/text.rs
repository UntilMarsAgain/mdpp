/// 进 HTML 属性或元素文字的转义。统一交给 `html-escape`，不再自己维护实体表。
pub fn escape(value: &str) -> String {
    html_escape::encode_quoted_attribute(value).into_owned()
}

/// 一趟扫过去替换占位符。
///
/// **一趟**是刻意的：先填标题再填正文的话，正文里写着 `{{title}}` 会被当成占位符改掉。
/// 认不出的名字与没写全的 `{{` 都原样留着 —— 让作者看见自己写错了，比悄悄吞掉好。
///
/// 扫描核心在库那边（[`mdpp::substitute_placeholders`]）—— 用户块的 `{{param}}` 填空
/// 走的是同一条路，两份各写一份的话，认不出的名字怎么处理迟早会对不上，而那是静悄悄的。
pub(super) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    mdpp::substitute_placeholders(template, |key| {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    })
}
