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
/// 不是完整的 CSS 解析器：字符串里的花括号不特殊处理。花括号不配对时，未闭合的
/// 那一段会被丢掉 —— 原样放行会让它绕过 [`CONTENT_SCOPE`]，改到整页。
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
            // 没有配对的 `}`：后面的选择器无法收进正文。原样放行会被浏览器
            // 当成「到文件结尾才闭合」的规则，直接作用于整页，所以只能丢。
            out.push_str("/* mdpp: 未闭合的 CSS 块已丢弃 */");
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
