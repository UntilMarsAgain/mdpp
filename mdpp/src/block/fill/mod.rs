//! 用户块的「填空」与过滤。
//!
//! 三件事：`{{}}` 把参数与块内容填进模板（[`substitute_checked`]；宽容版
//! [`substitute`] 留给底层调用方）；CSS 收窄把用户写的样式关进 **正文**
//! （[`scope_css`]）；`::html` 的正文净化转发给 [`crate::block::html`]。
//!
//! [`source_of`] 是这一层的枢纽：`{{}}`、CSS、HTML 三样都要先问它要正文，因为正文可能
//! 来自 `src=` 指的模板页而不是块自己的内容。它是**唯一**一处「`src=` 指的是谁」的
//! 判断。

use crate::block::template::{self, TemplatePage};
use crate::registry::Block;

/// 严格填空：`{{name}}` 没有对应参数就返回错误，`{{?name}}` 则是可选参数
/// （缺失时填空串）。
///
/// `{{@name}}` 是晚绑定引用，早填充这一层原样保留，交给参数阶段；写在正文里会
/// 字面显示，正文插值要用 `{{name}}`。
///
/// 认不出的必填名字、没闭合的 `{{` 都返回错误，而不是像 [`substitute`] 那样原样留下。
pub fn substitute_checked(text: &str, block: &Block, body: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(format!("占位符没有闭合：{}", &rest[start..]));
        };
        let raw = after[..end].trim();
        let (key, optional) = match raw.strip_prefix('?') {
            Some(key) => (key.trim(), true),
            None => (raw, false),
        };
        if key.is_empty() {
            return Err("占位符的名字不能为空".to_string());
        }
        if key.starts_with('@') {
            // 晚绑定引用留给块头分词之后的解析；正文里写它会原样显示。
            // 早填充这一层不替它猜作用域。
            out.push_str(&rest[start..start + 2 + end + 2]);
            rest = &after[end + 2..];
            continue;
        }
        let value = if key == "body" {
            Some(body.to_string())
        } else {
            block.param(key).map(str::to_string)
        };
        match value {
            Some(value) => {
                // 缩进按占位符在**原文**里的位置算：`rest` 是还没扫描的尾巴，
                // 直接用 `start` 会把前面的换行漏掉，多行值就落回行首。
                let at = text.len() - rest.len() + start;
                out.push_str(&reindent(&value, line_indent(text, at)));
            }
            None if optional => {}
            None => return Err(format!("模板参数 `{key}` 没有传入")),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// 宽容填空：`{{name}}` 换成参数值，`{{body}}` 换成块内容。
///
/// 认不出的名字**原样留着**，没有闭合的 `{{` 也原样留着 —— 这是给底层调用方
/// 保留的旧行为；模板展开与内置的 `::css` / `::js` / `::html` 走
/// [`substitute_checked`]。
pub fn substitute(text: &str, block: &Block, body: &str) -> String {
    // 核心扫描交给 [`substitute_placeholders`] —— 页外壳填占位符（`mdpp-cli/src/page.rs`）
    // 走的也是同一条路，两份各写一份的话，认不出的名字怎么处理迟早会对不上。
    // 这里的两条特例是：`body` 换成块内容、`参数` 换成参数值。
    substitute_placeholders(text, |key| {
        if key == "body" {
            Some(body.to_string())
        } else {
            block.param(key).map(str::to_string)
        }
    })
}

/// 双花括号占位符的通用替换：`{{key}}` 换成 `lookup(key)` 给出的值，
/// **认不出的名字原样留着**（`{{key}}` 一字不动），没有闭合的 `{{` 也原样留着 ——
/// 让作者看见自己写错了，比悄悄换成空串好。
///
/// 值是**多行**的时候，除第一行外的每一行都补上占位符所在行的行首缩进
/// （见 `reindent`）—— 模板里写的是
///
/// ```text
/// ::code lang=mdpp
///   {{body}}
/// ```
///
/// 块内容就得整体落在那两个空格里。只把第一行填进缩进位置的话，后面的行会
/// 落回行首，靠缩进划范围的块（`::code`、分栏的每一节）当场被截断，剩下的行
/// 漏成页面内容 —— 而页面上看起来只是「模板没生效」。
///
/// 这一份是用户块（[`substitute`]）与命令行页外壳（`mdpp-cli/src/page.rs`）共用的
/// 扫描核心：两处各写一遍，认不出的名字怎么处理迟早会对不上，而那是静悄悄的。
pub fn substitute_placeholders(text: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
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
        match lookup(key) {
            Some(value) => {
                // 占位符在**原文**里的位置：缩进要照作者写下的那一行算，
                // 不能照已经填过一部分的 `out` 算（前一个值可能带来换行）
                let at = text.len() - rest.len() + start;
                out.push_str(&reindent(&value, line_indent(text, at)));
            }
            None => {
                // 原样留回 `{{...}}`（含键名本身，保留作者写的空白）
                out.push_str(&rest[start..start + 2 + end + 2]);
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// `at` 所在那一行的行首空白（占位符在列 0 时是空串）。
///
/// 行中间有字也算这一行的缩进：`  前缀 {{body}}` 的后续行与 `前缀` 对齐，
/// 这是作者看着最自然的位置。
fn line_indent(text: &str, at: usize) -> &str {
    let line_start = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let line = &text[line_start..];
    let width = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..width]
}

/// 把多行值接到占位符后面：第一行就地接着，其余每行补上 `indent`。
///
/// 两种情形原样返回：缩进为空（占位符在列 0，页外壳填的就是这一类）、值只有一行。
/// **空行不补缩进** —— 补了就成了「只含空白的行」，markdown 会把它当成段落里的
/// 普通行，该断开的地方反而断不开。
fn reindent(value: &str, indent: &str) -> String {
    if indent.is_empty() || !value.contains('\n') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + indent.len() * value.matches('\n').count());
    for (index, line) in value.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
            if !line.trim().is_empty() {
                out.push_str(indent);
            }
        }
        out.push_str(line);
    }
    out
}

/// 解析模板文本里的晚绑定引用：`{{@名字}}` / `{{?@名字}}`。
///
/// `item_prefix` 是 `each` 当前项的名字（`item`、`col` …）。给了它时：
///
/// - `{{@item.xxx}}` 必须从**当前项**取，缺了就是错误；
/// - 别的 `{{@名字}}` 先从外层作用域取（模板参数、`@body` 等）；
/// - 外层也没有、但名字带 `.` 的，原样留下给内层 `each`；
/// - 外层也没有、名字不带 `.` 的，按缺参数处理。
///
/// 插入的值**不再扫第二遍**，与早绑定填空一致；多行值按占位符所在行的缩进补缩进。
pub(crate) fn resolve_late_refs(text: &str, item_prefix: Option<&str>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(format!("占位符没有闭合：{}", &rest[start..]));
        };
        let raw = after[..end].trim();
        let (raw, optional) = match raw.strip_prefix('?') {
            Some(raw) => (raw.trim(), true),
            None => (raw, false),
        };
        let Some(name) = raw.strip_prefix('@') else {
            // 不是晚绑定：原样留回，交给早绑定那一趟或作者自己
            out.push_str(&rest[start..start + 2 + end + 2]);
            rest = &after[end + 2..];
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            return Err("模板变量名不能为空".to_string());
        }
        let is_item = item_prefix.is_some_and(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'))
        });
        match crate::context::template_var(name) {
            Some(value) => {
                let at = text.len() - rest.len() + start;
                out.push_str(&reindent(&value, line_indent(text, at)));
            }
            None if item_prefix.is_some() && name.contains('.') && !is_item => {
                // 留给内层 each：外层只认自己的前缀
                out.push_str(&rest[start..start + 2 + end + 2]);
            }
            None if optional => {}
            None if is_item => {
                return Err(format!("当前项没有 `{name}` 这个变量"));
            }
            None => return Err(format!("模板变量 `{name}` 不存在")),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
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
                "`{page}` 这一页不存在（名字按页面名写，不用带后缀）"
            ))
        }
    }
}

/// 开关式参数开没开：写了 `flag`、`flag=true`、`flag=yes`、`flag=on` 都算开了。
///
/// 裸 flag 在解析时已经落成 `on`；显式 `flag=` 是空值，不算打开。
/// 要让开关式的参数表达「关」，写它专有的关值（比如 `rounded=off`），
/// 那由各个块自己说。
pub fn flag(block: &Block, key: &str) -> bool {
    block.params.iter().any(|(name, value)| {
        name == key && matches!(value.to_lowercase().as_str(), "true" | "yes" | "on")
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
    // `<markdown>` 里的那一段也要净化。不能直接把它交给当前解析器：`raw_html`
    // 打开时，嵌套 markdown 会把 `<script>` 原样透传，净化器就再也看不到它了。
    // 这里把槽位输出**再净化一遍**，于是 `::html` 的「安全」不依赖于外层开关。
    crate::block::html::sanitize(html, &|body| {
        let rendered = crate::engine::render_nested(body);
        sanitize_html(&rendered)
    })
}

mod css;

#[cfg(test)]
mod tests;

pub use css::{CONTENT_SCOPE, sanitize_css, scope_css};
