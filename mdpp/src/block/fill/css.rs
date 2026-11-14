//! 内容 CSS 的净化。
//!
//! `::css` 现在把样式交给整页收集器，不再套 `.mdpp-content`。这里只做两件
//! 必须做的事：丢掉 `@import`（不让正文把页面带去外部取样式），以及去掉
//! `</style`（不让一段 CSS 提前闭合样式块）。

/// CSS 里不许出现 `</style`：它会提前闭合样式块，把后面的内容顶到页面上。
///
/// 同时丢掉 `@import`：样式可以影响整页，但不应让页面在打开时去外部拉资源。
pub fn sanitize_css(css: &str) -> String {
    let css = drop_imports(css);
    let mut out = String::with_capacity(css.len());
    let mut rest = css.as_str();
    while let Some(position) = find_ci(rest, "</style", 0) {
        out.push_str(&rest[..position]);
        // 连它后面到 `>` 的部分一起去掉
        let tail = &rest[position..];
        let end = find_ci(tail, ">", 0).map(|i| i + 1).unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// 丢掉 `@import …;`（大小写不敏感）。
fn drop_imports(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(position) = find_ci(rest, "@import", 0) {
        out.push_str(&rest[..position]);
        let tail = &rest[position..];
        match tail.find(';') {
            Some(end) => rest = &tail[end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// 找一段子串（大小写不敏感）。
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if from > haystack.len() {
        return None;
    }
    let hay = haystack[from..].to_lowercase();
    let needle = needle.to_lowercase();
    hay.find(&needle).map(|position| position + from)
}
