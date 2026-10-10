//! 用户块的「填空」与过滤。
//!
//! 三件事各管一半：`{{}}` 把参数与块内容填进模板；CSS 收窄把样式关进**正文**；
//! `::html` 的正文净化转发给 [`crate::block::html`]。单独一个文件，是因为这里
//! 「能写的东西很多」：替换、CSS 收窄、HTML 净化各有一套规则。
//!
//! ## 页面里的一切都可能被模板页带着走
//!
//! [`source_of`] 是这一层的枢纽：`{{}}` 填空、CSS、HTML 三样**都要先问它要正文**，
//! 因为正文可能来自 `src=` 指的模板页，而不是块自己的内容。它是**唯一**一处
//! 「`src=` 指的是谁」的判断 —— 分散写就会三处对不上。

use crate::block::template::{self, TemplatePage};
use crate::registry::Block;

/// 把 `{{名字}}` 换成参数值，`{{body}}` 换成块内容。
///
/// 认不出的名字**原样留着** —— 让作者看见自己写错了，比悄悄换成空串好。
/// 没有闭合的 `{{` 也原样留着。
pub fn substitute(text: &str, block: &Block, body: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        if key == "body" {
            out.push_str(body);
        } else if let Some(value) = block.param(key) {
            out.push_str(value);
        } else {
            out.push_str("{{");
            out.push_str(&after[..end]);
            out.push_str("}}");
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// 这一块要处理的**内容**：没写 `src=` 就是块内容本身，写了就是模板页的正文。
///
/// 取不到时返回 `Err(原因)`：**「没有这一页」和「有，但此刻读不出来」要分得开** ——
/// 两种情况对作者来说是两件事，混成一句「取不到」最难查。
pub fn source_of(block: &Block) -> Result<String, String> {
    let Some(name) = block.param("src") else {
        return Ok(block.body.clone());
    };

    match template::page_for(name) {
        Some(TemplatePage::Ready(text)) => Ok(text),
        Some(TemplatePage::Unreadable { reason }) => Err(reason),
        None => {
            // 报的是**归一之后**的页名：写 `template:卡片` 时就别再叠一层前缀
            let page = template::page_key(name).unwrap_or_else(|| name.trim().to_string());
            Err(format!(
                "`{page}` 这一页不存在（名字按页面名写，不用带 `.md`）"
            ))
        }
    }
}

/// 开关式参数开没开：写了 `flag`、`flag=true`、`flag=yes`、`flag=on` 都算开了。
///
/// 只写 key 的写法（`js`）本来就是「打开」的意思 —— 要让开关式的参数能表达「关」，
/// 得写它专有的关值（比如 `rounded=off`），那由各个块自己说。
pub fn flag(block: &Block, key: &str) -> bool {
    block.params.iter().any(|(name, value)| {
        name == key && matches!(value.to_lowercase().as_str(), "" | "true" | "yes" | "on")
    })
}

/// `js` 参数是否显式打开了（`js` 或 `js=true`）
pub fn allows_js(block: &Block) -> bool {
    flag(block, "js")
}

/// 脚本原文里不能出现 `</script`：HTML 解析器见到它就提前收尾，
/// 后面的字会漏到页面上。转义成 `<\/script` —— JS 里这两种写法等价。
///
/// 只**插**一个反斜杠，不动原文的大小写：这一串也可能出现在字符串或正则里，
/// 改一个字母就是改了那一段代码的意思。
pub fn escape_script_end(code: &str) -> String {
    const NEEDLE: &str = "</script";
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    loop {
        let found = rest.char_indices().find(|(at, _)| {
            rest[*at..]
                .get(..NEEDLE.len())
                .is_some_and(|slice| slice.eq_ignore_ascii_case(NEEDLE))
        });
        let Some((at, _)) = found else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        // 只插一个反斜杠，其余原样搬 —— `\/` 与 `/` 在 JS 里是同一个字符
        out.push_str("<\\");
        out.push_str(&rest[at + 1..at + NEEDLE.len()]);
        rest = &rest[at + NEEDLE.len()..];
    }
}

/// `::html` 的正文净化 —— 转发给 [`crate::block::html`]。
///
/// 白名单净化在那边；这里只保证「想要 HTML 的人不用知道它住在另一个文件」。
pub fn sanitize_html(html: &str) -> String {
    crate::block::html::sanitize(html, &crate::engine::render_nested)
}

/// 收进正文时用的**范围前缀**（见 [`scope_css`]）。
pub const CONTENT_SCOPE: &str = ".mdpp-content";

/// 把用户写的 CSS 收进**正文**：每条选择器都加 [`CONTENT_SCOPE`] 前缀。
///
/// 为什么不原样注入：那样它作用于**整个页面** —— 一条 `* { }` 就能把标签栏、菜单、
/// 工具条一起改掉。收进正文之后，它只影响正文；而正文内部仍按正常层叠参与，
/// 注入的样式在文档里靠后，同优先级的规则由它胜出。
///
/// 页面里的容器 class 名要改，这里也要跟着改 —— 代价是与调用方的类名耦合，
/// 而那个类名本身就是一个「正文的哪一层」的约定。用 [`CONTENT_SCOPE`] 常量
/// 指出来就是为了让这个耦合只有一处。
///
/// 顺带处理两件事：`@import` 一律丢掉（会让页面去外部拉样式），
/// `@media` 之类的条件规则递归处理（里面的选择器同样要收）。
///
/// 不是完整的 CSS 解析器：花括号不配对时原样放行，字符串里的花括号也不特殊处理 ——
/// CSS 里这两种情况都很罕见，而真遇到时「原样放行」比「猜错」好。
pub fn scope_css(css: &str) -> String {
    scope_css_raw(css).trim().to_string()
}

/// 不做两端裁剪的版本：递归处理条件规则时要保留里面的空白
fn scope_css_raw(css: &str) -> String {
    let css = drop_imports(css);
    let mut out = String::with_capacity(css.len() + 16);
    let mut rest = css.as_str();

    while let Some(position) = rest.find('{') {
        let head = &rest[..position];
        let after = &rest[position..];

        // 找配对的 `}`
        let mut depth = 0usize;
        let mut close = None;
        for (index, ch) in after.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            out.push_str(rest);
            return out;
        };

        let body = &after[1..close];
        rest = &after[close + 1..];
        let trimmed = head.trim();

        if trimmed.starts_with('@') {
            let keyword = trimmed
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();
            match keyword.as_str() {
                // 条件规则：里面的选择器同样要收
                "@media" | "@supports" | "@layer" | "@container" | "@scope" => {
                    out.push_str(head);
                    out.push('{');
                    out.push_str(&scope_css_raw(body));
                    out.push('}');
                }
                // 关键帧、字体、分页：名字是全局的，选择器不是，原样保留
                _ => {
                    out.push_str(head);
                    out.push('{');
                    out.push_str(body);
                    out.push('}');
                }
            }
            continue;
        }

        // 保留选择器前后的空白（缩进、以及 `{` 前那个空格），中间那截换成收窄后的选择器
        let lead = &head[..head.len() - head.trim_start().len()];
        let trail = &head[head.trim_end().len()..];
        out.push_str(lead);
        let scoped: Vec<String> = split_selectors(trimmed)
            .iter()
            .map(|selector| scope_selector(selector))
            .filter(|selector| !selector.is_empty())
            .collect();
        out.push_str(&scoped.join(", "));
        out.push_str(trail);
        out.push('{');
        out.push_str(body);
        out.push('}');
    }
    out.push_str(rest);
    out
}

/// 丢掉 `@import …;`（大小写不敏感）
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

/// 按**顶层**逗号切开选择器（`:is(a, b)` 里的逗号不算）
fn split_selectors(selector: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for ch in selector.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    parts.push(current);
    parts
}

/// 给一条选择器加范围。
fn scope_selector(selector: &str) -> String {
    let selector = selector.trim();
    if selector.is_empty() || selector.starts_with(CONTENT_SCOPE) || selector.starts_with('&') {
        return selector.to_string();
    }
    // 写给根元素的规则收成正文本身：于是「给整篇正文设字体或变量」仍然写得出来
    for root in ["html", "body", ":root"] {
        if selector == root {
            return CONTENT_SCOPE.to_string();
        }
        if let Some(rest) = selector.strip_prefix(root) {
            if rest.starts_with('.') || rest.starts_with(':') || rest.starts_with('[') {
                return format!("{CONTENT_SCOPE}{rest}");
            }
        }
    }
    format!("{CONTENT_SCOPE} {selector}")
}

/// CSS 里不许出现 `</style`：它会提前闭合样式块，把后面的内容顶到页面上
pub fn sanitize_css(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
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

/// 找一段子串（大小写不敏感）。
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if from > haystack.len() {
        return None;
    }
    let hay = haystack[from..].to_lowercase();
    let needle = needle.to_lowercase();
    hay.find(&needle).map(|position| position + from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Block;

    fn block(name: &str, params: &[(&str, &str)], body: &str) -> Block {
        Block {
            name: name.to_string(),
            params: params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            body: body.to_string(),
        }
    }

    #[test]
    fn substitutes_params_and_body() {
        let item = block("x", &[("颜色", "红")], "块内容");
        assert_eq!(
            substitute("a {{颜色}} b {{body}} c", &item, "块内容"),
            "a 红 b 块内容 c"
        );
    }

    /// 认不出的名字原样留着：让作者看见自己写错了，比悄悄换成空串好
    #[test]
    fn unknown_or_unclosed_placeholders_stay_visible() {
        let item = block("x", &[], "");
        assert_eq!(substitute("{{没有这个}}", &item, ""), "{{没有这个}}");
        assert_eq!(substitute("{{没闭合", &item, ""), "{{没闭合");
    }

    #[test]
    fn source_is_the_block_body_when_there_is_no_src() {
        let item = block("x", &[], "块里的内容");
        assert_eq!(source_of(&item).as_deref(), Ok("块里的内容"));
    }

    /// 没有模板页来源时，`src=` 指的页一律「不存在」—— 而不是含糊的「取不到」
    #[test]
    fn a_src_pointing_nowhere_says_the_page_is_missing() {
        let item = block("css", &[("src", "没有这一页")], "");
        let why = source_of(&item).unwrap_err();
        assert!(why.contains("不存在"), "{why}");
        // 报的是归一之后的页名，不叠前缀
        assert!(why.contains("`没有这一页`"), "{why}");
    }

    #[test]
    fn scripts_are_filtered_by_default() {
        let html = sanitize_html("<p>正文</p><script>alert(1)</script>");
        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains("正文"), "{html}");
    }

    #[test]
    fn event_handlers_and_javascript_urls_are_filtered() {
        let html =
            sanitize_html(r#"<img src="x" onerror="alert(1)"><a href="javascript:alert(1)">x</a>"#);
        assert!(!html.contains("onerror"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        // 无辜的属性与内容不受影响
        assert!(html.contains(r#"src="x""#), "{html}");
    }

    /// 只有「属性位置上的 on…=」才该被拿掉 —— 正文里的 `one=1` 不是事件处理
    #[test]
    fn on_inside_ordinary_words_is_left_alone() {
        let html = sanitize_html("<p>one=1 与 onboarding=2</p>");
        assert!(html.contains("one=1"), "{html}");
        assert!(html.contains("onboarding=2"), "{html}");
    }

    #[test]
    fn css_is_scoped_to_the_content() {
        assert_eq!(
            scope_css(".a { color: red }"),
            ".mdpp-content .a { color: red }"
        );
        assert_eq!(
            scope_css(".a, .b { }"),
            ".mdpp-content .a, .mdpp-content .b { }"
        );
        // `:is(a, b)` 里的逗号不是选择器分隔符
        assert_eq!(scope_css(":is(a, b) { }"), ".mdpp-content :is(a, b) { }");
        // 写给根元素的收成正文本身
        assert_eq!(scope_css("body { }"), ".mdpp-content { }");
        assert_eq!(scope_css(":root { --x: 1 }"), ".mdpp-content { --x: 1 }");
        // 已经收过的不重复收
        assert_eq!(scope_css(".mdpp-content a { }"), ".mdpp-content a { }");
    }

    #[test]
    fn conditional_rules_are_scoped_inside() {
        assert_eq!(
            scope_css("@media (min-width: 1px) { .a { } }"),
            "@media (min-width: 1px) { .mdpp-content .a { } }"
        );
        // 关键帧里的百分比不是选择器，别去动它
        assert_eq!(
            scope_css("@keyframes spin { from { } to { } }"),
            "@keyframes spin { from { } to { } }"
        );
    }

    #[test]
    fn imports_are_dropped() {
        // 让页面去外部拉样式：既慢又不受控
        assert_eq!(
            scope_css("@import url(x.css); .a { }"),
            ".mdpp-content .a { }"
        );
    }

    #[test]
    fn style_cannot_be_closed_early() {
        let css = sanitize_css("a { } </style><b>x</b>");
        assert!(!css.contains("</style"), "{css}");
        assert!(css.contains("<b>x</b>"), "只去掉闭合标签，其余照旧：{css}");
    }

    #[test]
    fn script_end_cannot_close_its_own_tag() {
        assert_eq!(escape_script_end(r#"a</script>b"#), r#"a<\/script>b"#);
        // 大小写照原样留着：那可能是字符串里的内容
        assert_eq!(escape_script_end("</SCRIPT>"), r#"<\/SCRIPT>"#);
        assert_eq!(escape_script_end("没有"), "没有");
    }

    #[test]
    fn switches_read_the_usual_spellings() {
        for spelling in ["", "true", "yes", "on", "ON"] {
            let item = block("x", &[("js", spelling)], "");
            assert!(allows_js(&item), "js={spelling:?} 应当算打开");
        }
        for spelling in ["false", "off", "no", "0"] {
            let item = block("x", &[("js", spelling)], "");
            assert!(!allows_js(&item), "js={spelling:?} 不该算打开");
        }
        assert!(!allows_js(&block("x", &[], "")));
    }
}
