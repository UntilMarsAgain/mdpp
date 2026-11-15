//! 把内容**原样写出去**的块：`code` / `mermaid` / `math` / `css` / `html` / `js`。
//!
//! 共同规矩是**正文一概不按 markdown 解析**（注册表里是
//! [`ContentMode::Raw`](crate::registry::ContentMode)），但理由分三种：
//!
//! - `code` / `mermaid` / `math`：内容是**另一种语言**，里面的 `*`、`_`、`\`、`-->`、
//!   `[]` 都是它自己的记号。
//! - `css` / `js`：内容是**要执行的东西**，还要先过一遍 `{{param}}` 填空与过滤
//!   （见 [`fill`](crate::block::fill)）。
//! - `html`：内容是**要落进页面的标记**，默认过滤掉会执行脚本的东西，除非显式写了 `js`。
//!
//! 这一组都不碰 `node.children`：块正文走 `block.body`（原文）。

use super::super::dispatch::render_problem;
use super::super::fill;
use crate::assets::{AssetKind, AssetPolicy, AssetUse, use_asset, use_asset_with};
use crate::registry::Block;
use html5ever::serialize::{SerializeOpts, TraversalScope, serialize};
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, QualName, local_name, ns};
use markdown_it::{Node, Renderer};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

/// `::js` —— 把内容当**原始 JavaScript** 注入这一页，随页面加载执行。
///
/// 与 `::html` 一个规矩：内容原样，`{{param}}` 会替换。不过滤 ——
/// 写下 `::js` 本身就是一次显式的决定，与 `::html js` 是同一个意思。
///
/// 注入的脚本里若出现 `</script`，HTML 解析器会当场收尾，后面的字会漏到页面上，
/// 所以那一处先转义（见 [`fill::escape_script_end`]）。
///
/// 普通脚本不在调用点落地，而是收集到 [`crate::Rendered::scripts`]，统一放到
/// 片段末尾；站点模式下外链标签同样进 `scripts`，内容随
/// [`crate::Rendered::assets`] 返回交给调用方写盘。
///
/// 在变体作用域里（`::theme` / `::tabs` / 自定义组）时，输出通用惰性占位符，
/// 由页面脚本在选项激活时挂载。所有脚本都带 `data-mdpp-version` 与
/// `data-mdpp-mode`，`once` 参数可以把策略标成“只执行一次”。
pub(super) fn render_js(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let source = match fill::source_of(block) {
        Ok(s) => s,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let filled = match fill::substitute_checked(&source, block, &block.body) {
        Ok(filled) => filled,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let pol = if fill::flag(block, "once") {
        AssetPolicy::Once
    } else {
        AssetPolicy::EveryActivation
    };
    let escaped = fill::escape_script_end(&filled);
    settle_script(fmt, use_asset_with(AssetKind::Js, escaped, pol));
}

/// `::mermaid` —— 画一张图（[mermaid](https://mermaid.js.org) 的语法）。
///
/// 这里只把**原文**放进 `<pre class="mdpp-mermaid">`，画图在前端（mermaid 是 JS）。
/// 与 `::code` 同一个道理：图定义里的 `-->`、`[]`、`{}` 都是它自己的记号，
/// 绝不能按 markdown 解析。
///
/// 原文同时留一份在 `data-mdpp-source` 上：mermaid 画完会把元素内容换成 SVG，
/// 切主题要重画时就靠这份原文还原。
pub(super) fn render_mermaid(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let source = block.body.trim();
    if source.is_empty() {
        render_problem(block, fmt, "里面还没有图 —— 写一段 mermaid 定义");
        return;
    }

    fmt.cr();
    fmt.open("div", &[("class", "mdpp-diagram".to_string())]);
    fmt.cr();
    fmt.open(
        "pre",
        &[
            ("class", "mdpp-mermaid".to_string()),
            ("data-mdpp-source", source.to_string()),
        ],
    );
    fmt.text(source);
    fmt.close("pre");
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// `::math` —— 独立成段（或多行）的公式，TeX 原文，前端用 KaTeX 排版。
///
/// 正文**不按 markdown 解析**：公式里的 `*`、`_`、`\` 都是数学的一部分
/// （与 `::code`、`::mermaid` 同一条规矩）。
/// 只写一行的公式也可以用 `$…$` / `$$…$$`，那是 [`crate::syntax::math`] 认的。
pub(super) fn render_math(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let tex = block.body.trim();
    if tex.is_empty() {
        render_problem(block, fmt, "里面还没有公式 —— 写一段 TeX");
        return;
    }

    fmt.cr();
    fmt.open(
        "div",
        &[
            ("class", "mdpp-math mdpp-math--display".to_string()),
            ("data-mdpp-tex", tex.to_string()),
        ],
    );
    // 排版之前看到的是原文，取不到 KaTeX 也不至于是一片空白
    fmt.text(tex);
    fmt.close("div");
    fmt.cr();
}

/// `::code lang=rust lines=on start=10 highlight=2-3 fold=on source=…`
/// —— 像 markdown 的代码块，但能控制行号、强调行、折叠与源码入口。
///
/// 后端只输出**语义结构**和几个 `data-mdpp-*` 提示；行号、强调色带、折叠
/// 展开后的补全由 `mdpp.js` 落地。拆行必须在前端做，因为 highlight.js 的
/// span 可能跨行；后端拆行会破坏它的分词。
///
/// `lines=` 默认关；`start=` 会自动打开行号。`highlight=` 用的是**显示行号**
/// （已经算上 `start`），所以 `start=10 highlight=10-11` 强调头两行。
/// `source=` 是来源 URL；`source-text=` 是链接文字，默认“查看源码”。
/// `fold=on|open` 会套一层 `<details>`；`on` 初始折叠，`open` 初始展开。
pub(super) fn render_code(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let language = block.param("lang").unwrap_or("").trim();

    let fold = match block.param("fold").map(str::trim) {
        None | Some("") | Some("off") | Some("false") | Some("no") => None,
        Some("on") | Some("closed") => Some(false),
        Some("open") => Some(true),
        Some(other) => {
            render_problem(
                block,
                fmt,
                &format!("fold= 只认 on / open / off，写成了 `{other}`"),
            );
            return;
        }
    };

    let source = match block.param("source").map(str::trim) {
        Some("") => {
            render_problem(block, fmt, "source= 要给出源码地址，或者整项删掉");
            return;
        }
        Some(value) if !safe_source(value) => {
            render_problem(
                block,
                fmt,
                "source= 用的是不允许的协议（javascript / data / vbscript 会被拒绝）",
            );
            return;
        }
        Some(value) => Some(value),
        None => None,
    };

    let source_text = block
        .param("source-text")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("查看源码");
    if source.is_none() && block.param("source-text").is_some() {
        render_problem(block, fmt, "source-text= 必须和 source= 一起用");
        return;
    }

    let lines_param = block.param("lines").map(str::trim);
    let lines_on = match lines_param {
        None => false,
        Some("on" | "true" | "yes" | "1") => true,
        Some("off" | "false" | "no" | "0") => false,
        Some(other) => {
            render_problem(
                block,
                fmt,
                &format!("lines= 只认 on / off，写成了 `{other}`"),
            );
            return;
        }
    };

    let start_param = block.param("start").map(str::trim);
    let start = match start_param {
        None => 1,
        Some(value) => match value.parse::<usize>() {
            Ok(number) if number > 0 => number,
            _ => {
                render_problem(block, fmt, "start= 要写一个从 1 开始的正整数");
                return;
            }
        },
    };
    if start_param.is_some() && lines_param == Some("off") {
        render_problem(block, fmt, "start= 会打开行号，不能同时写 lines=off");
        return;
    }
    let lines_on = lines_on || start_param.is_some();

    let line_count = block.body.lines().count().max(1);
    let highlight = match block.param("highlight").map(str::trim) {
        None => None,
        Some(value) => match normalize_highlight(value, start, line_count) {
            Ok(value) => Some(value),
            Err(why) => {
                render_problem(block, fmt, &why);
                return;
            }
        },
    };

    let mut attrs: Vec<(&str, String)> = vec![("class", "mdpp-template-code".to_string())];
    if lines_param.is_some() || start_param.is_some() {
        attrs.push((
            "data-mdpp-lines",
            if lines_on { "on" } else { "off" }.to_string(),
        ));
    }
    if start_param.is_some() {
        attrs.push(("data-mdpp-line-start", start.to_string()));
    }
    if let Some(value) = &highlight {
        attrs.push(("data-mdpp-highlight", value.clone()));
    }

    let code_attrs: Vec<(&str, String)> = if language.is_empty() {
        Vec::new()
    } else {
        vec![("class", format!("language-{language}"))]
    };

    let wrapper = source.is_some() || fold.is_some();
    if wrapper {
        fmt.cr();
        fmt.open("figure", &[("class", "mdpp-code".to_string())]);
        fmt.cr();
        if let Some(source) = source {
            fmt.open("figcaption", &[("class", "mdpp-code__head".to_string())]);
            fmt.open("span", &[("class", "mdpp-code__title".to_string())]);
            fmt.text("代码");
            fmt.close("span");
            fmt.open(
                "a",
                &[
                    ("class", "mdpp-code__source".to_string()),
                    ("href", source.to_string()),
                    ("target", "_blank".to_string()),
                    ("rel", "noopener noreferrer".to_string()),
                ],
            );
            fmt.text(source_text);
            fmt.close("a");
            fmt.close("figcaption");
            fmt.cr();
        }
    }

    if let Some(open) = fold {
        let mut detail_attrs = vec![("class", "mdpp-code__fold".to_string())];
        if open {
            detail_attrs.push(("open", "open".to_string()));
        }
        fmt.open("details", &detail_attrs);
        fmt.open("summary", &[("class", "mdpp-code__summary".to_string())]);
        fmt.text("查看代码");
        fmt.close("summary");
        fmt.cr();
    }

    fmt.open("pre", &attrs);
    fmt.open("code", &code_attrs);
    // 原文照收：代码里的 markdown 记号**不该**被解析
    fmt.text(&block.body);
    fmt.close("code");
    fmt.close("pre");
    fmt.cr();

    if fold.is_some() {
        fmt.close("details");
        fmt.cr();
    }
    if wrapper {
        fmt.close("figure");
        fmt.cr();
    }
}

/// `source=` 只挡能执行或能外传数据的三种协议；相对地址照常放行。
fn safe_source(value: &str) -> bool {
    let probe: String = value
        .chars()
        .filter(|ch| !ch.is_control() && !ch.is_whitespace())
        .collect();
    let lower = probe.to_ascii_lowercase();
    !["javascript:", "data:", "vbscript:"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// 把 `2-3,5` 规范化成 `2-3,5`，并按**显示行号**检查范围。
fn normalize_highlight(value: &str, start: usize, line_count: usize) -> Result<String, String> {
    if value.is_empty() {
        return Err("highlight= 要写行号，例如 `2-3,5`".to_string());
    }
    let end = start.saturating_add(line_count).saturating_sub(1);
    let mut parts = Vec::new();
    for piece in value.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            return Err("highlight= 里有空段，写成 `2-3,5` 这样".to_string());
        }
        let (from, to) = match piece.split_once('-') {
            Some((from, to)) => (from.trim(), Some(to.trim())),
            None => (piece, None),
        };
        let from = parse_line_number(from, "highlight=")?;
        let to = match to {
            Some(to) => parse_line_number(to, "highlight=")?,
            None => from,
        };
        if to < from {
            return Err(format!("highlight= 的区间 `{piece}` 结尾小于开头"));
        }
        if from < start || to > end {
            return Err(format!(
                "highlight= 的 `{piece}` 超出代码范围：当前显示第 {start}–{end} 行"
            ));
        }
        parts.push(if from == to {
            from.to_string()
        } else {
            format!("{from}-{to}")
        });
    }
    Ok(parts.join(","))
}

fn parse_line_number(value: &str, what: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(number) if number > 0 => Ok(number),
        _ => Err(format!("{what} 里的 `{value}` 不是正整数")),
    }
}
/// `::css` —— 把内容（或 `src="页面名"` 指的模板页）登记为**整页** CSS。
///
/// 样式不再收进正文：它由 [`crate::env`] 收集，去重后由调用方放在页面级
/// `<style>` 里（片段在开头，整页/站点在 `<head>`）。因此组件样式要自己带
/// `var(--token, fallback)`，不能假设主题表一定在。
///
/// 内容里的 `{{param}}` 会被替换；`</style` 会被去掉（`@import` 与 `url()` 原样留着）。
///
/// 在变体作用域里（`::theme` / `::tabs` / 自定义组）时仍是延迟资源：
/// 输出通用惰性占位符，由页面脚本在选项激活时挂载。
pub(super) fn render_css(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let source = match fill::source_of(block) {
        Ok(s) => s,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let filled = match fill::substitute_checked(&source, block, &block.body) {
        Ok(filled) => filled,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let css = fill::sanitize_css(&filled);
    if crate::variants::current().is_some() {
        // 变体里的样式要等选项激活才生效，仍走延迟资源。
        write_css(fmt, use_asset(AssetKind::Css, css));
    } else {
        crate::env::register_style(css);
    }
}

/// `::html` —— 把内容当**原始 HTML** 注入。
///
/// 默认**过滤**掉会执行脚本的东西（`<script>`、`on*=` 事件属性、`javascript:` 协议）；
/// 只有显式写了 `js`（或 `js=true`）才原样放行。默认安全，要开就得自己写出来。
///
/// 单页模式下，写了 `js` 的块里那些脚本按内容哈希去重后内联；站点模式下它们被
/// 提取成资源、替换为 `<script src="…">`，内容随 [`crate::Rendered::assets`] 返回。
///
/// 在变体作用域里（`::theme` / `::tabs` / 自定义组），脚本会变成延迟资源，
/// 由页面脚本在选项激活时挂载；其余 HTML 正常输出，显隐由外层容器控制。
pub(super) fn render_html(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let source = match fill::source_of(block) {
        Ok(s) => s,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let filled = match fill::substitute_checked(&source, block, &block.body) {
        Ok(filled) => filled,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    // 写了 js：脚本提取成资源，其余 HTML 正常输出
    if fill::allows_js(block) {
        let (html_without_scripts, js_content) = take_scripts(&filled);
        if !js_content.is_empty() {
            let escaped = fill::escape_script_end(&js_content);
            let pol = if fill::flag(block, "once") {
                AssetPolicy::Once
            } else {
                AssetPolicy::EveryActivation
            };
            settle_script(fmt, use_asset_with(AssetKind::Js, escaped, pol));
        }
        if !html_without_scripts.is_empty() {
            fmt.text_raw(&html_without_scripts);
        }
        fmt.cr();
        return;
    }

    let html = fill::sanitize_html(&filled);
    fmt.cr();
    fmt.text_raw(&html);
    fmt.cr();
}

/// 把一份 JS 资源登记到页面末尾；变体作用域里的延迟资源仍留在原位。
///
/// 普通脚本不在调用点输出：先收集，最后由渲染器统一放到片段末尾（整页/站点
/// 同样由外层拼在正文后）。这样 `::js` 不会把脚本插在段落中间，顺序也稳定。
fn settle_script(fmt: &mut dyn Renderer, use_: AssetUse) {
    match use_ {
        AssetUse::Inline(content) => {
            let tag = inline_script_tag(&content);
            if !crate::env::register_script(tag.clone()) {
                fmt.cr();
                fmt.text_raw(&tag);
                fmt.cr();
            }
        }
        AssetUse::Linked(url) => {
            let tag = linked_script_tag(&url);
            if !crate::env::register_script(tag.clone()) {
                fmt.cr();
                fmt.text_raw(&tag);
                fmt.cr();
            }
        }
        AssetUse::Deferred(_) | AssetUse::Duplicate => write_script(fmt, use_),
    }
}

/// 内联脚本标签；内容已由调用方转义 `</script`。
fn inline_script_tag(content: &str) -> String {
    format!(
        "<script class=\"mdpp-js\" data-mdpp-version=\"{}\" data-mdpp-mode=\"inline\" data-mdpp-runtime=\"asset\">\n{}\n</script>\n",
        escape_attr(crate::VERSION),
        content
    )
}

/// 外链脚本标签。
fn linked_script_tag(url: &str) -> String {
    format!(
        "<script src=\"{}\" class=\"mdpp-js\" data-mdpp-version=\"{}\" data-mdpp-mode=\"linked\" data-mdpp-runtime=\"asset\"></script>\n",
        escape_attr(url),
        escape_attr(crate::VERSION)
    )
}

/// 把一份 JS 资源落成页面里的标签。
///
/// 所有脚本都带 `data-mdpp-version`，让页面脚本/诊断能知道这是哪一版生成的；
/// 延迟资源由 [`AssetUse::Deferred`] 写成 `<template>`，等前端激活。
fn write_script(fmt: &mut dyn Renderer, use_: AssetUse) {
    match use_ {
        AssetUse::Inline(content) => {
            fmt.cr();
            fmt.open(
                "script",
                &[
                    ("class", "mdpp-js".to_string()),
                    ("data-mdpp-version", crate::VERSION.to_string()),
                    ("data-mdpp-mode", "inline".to_string()),
                    ("data-mdpp-runtime", "asset".to_string()),
                ],
            );
            fmt.text_raw(&content);
            fmt.close("script");
            fmt.cr();
        }
        AssetUse::Linked(url) => {
            fmt.cr();
            fmt.text_raw(&format!(
                "<script src=\"{}\" class=\"mdpp-js\" data-mdpp-version=\"{}\" data-mdpp-mode=\"linked\" data-mdpp-runtime=\"asset\"></script>",
                escape_attr(&url),
                escape_attr(crate::VERSION)
            ));
            fmt.cr();
        }
        AssetUse::Deferred(asset) => {
            fmt.cr();
            fmt.text_raw(&asset.html());
            fmt.cr();
        }
        AssetUse::Duplicate => {}
    }
}

/// 把一份 CSS 资源落成页面里的标签。
fn write_css(fmt: &mut dyn Renderer, use_: AssetUse) {
    match use_ {
        AssetUse::Inline(content) => {
            fmt.cr();
            fmt.open("style", &[]);
            fmt.text_raw(&content);
            fmt.close("style");
            fmt.cr();
        }
        AssetUse::Linked(url) => {
            fmt.cr();
            fmt.text_raw(&format!(
                "<link rel=\"stylesheet\" href=\"{}\">",
                escape_attr(&url)
            ));
            fmt.cr();
        }
        AssetUse::Deferred(asset) => {
            fmt.cr();
            fmt.text_raw(&asset.html());
            fmt.cr();
        }
        AssetUse::Duplicate => {}
    }
}

/// 进 HTML 属性值的转义。链接是我们自己拼的（前缀 + 哈希文件名），
/// 但前缀来自调用方，转义一下更稳妥。
fn escape_attr(value: &str) -> String {
    html_escape::encode_double_quoted_attribute(value).into_owned()
}

/// 把 `<script>` 从 HTML 里摘出来：返回 `(剩余 HTML, 脚本内容)`。
///
/// 用 HTML 解析器而不是字符串找 `<script`：注释里、属性值里、`<scripture>` 里都
/// 可能出现这几个字母，字符串扫描会把它们当成脚本。
fn take_scripts(html: &str) -> (String, String) {
    let dom = html5ever::parse_fragment(
        RcDom::default(),
        ParseOpts::default(),
        QualName::new(None, ns!(html), local_name!("body")),
        Vec::new(),
        true,
    )
    .from_utf8()
    .one(html.as_bytes());

    let mut scripts = String::new();
    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        let mut children = node.children.borrow_mut();
        children.retain(|child| {
            let is_script = matches!(
                &child.data,
                NodeData::Element { name, .. } if &*name.local == "script"
            );
            if is_script {
                scripts.push_str(&text_content(child));
                scripts.push('\n');
            }
            !is_script
        });
        for child in children.iter().rev() {
            stack.push(child.clone());
        }
    }

    let mut bytes = Vec::new();
    serialize(
        &mut bytes,
        &SerializableHandle::from(dom.document),
        SerializeOpts {
            traversal_scope: TraversalScope::ChildrenOnly(None),
            ..SerializeOpts::default()
        },
    )
    .expect("序列化到内存不应失败");
    let remaining = String::from_utf8(bytes).expect("html5ever 的输出必定是 UTF-8");
    (remaining, scripts)
}

/// 元素里所有文字节点拼起来。
fn text_content(root: &Handle) -> String {
    let mut out = String::new();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Text { contents } = &node.data {
            out.push_str(&contents.borrow());
        }
        for child in node.children.borrow().iter().rev() {
            stack.push(child.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// `::js`：内容原样进 `<script>`，只是 `</script` 会被转义掉
    #[test]
    fn js_goes_in_raw_and_cannot_close_its_own_tag() {
        let html = render("::js\n  console.log(\"你好\");\n");
        assert!(html.contains("<script"), "{html}");
        assert!(html.contains("console.log(\"你好\");"), "{html}");

        let tricky = render("::js\n  const s = \"</script>\";\n");
        assert!(
            !tricky.contains("</script>\";"),
            "不能让它提前闭合：{tricky}"
        );
        assert!(tricky.contains("<\\/script>"), "{tricky}");

        // 大小写照原样留着：那可能是字符串里的内容，改一个字母就改了它的意思
        let shouted = render("::js\n  const s = \"</SCRIPT>\";\n");
        assert!(shouted.contains("<\\/SCRIPT>"), "{shouted}");
    }

    /// 注入的 CSS 与正文在同一份页面里：主题的那些变量 `var()` 直接就取得到，
    /// 片段开头会统一收集`<style>`
    #[test]
    fn css_template_injects_style_and_our_variables() {
        let html = render("::css\n  .我有自己的样式 { color: var(--mdpp-accent); }\n");
        assert!(html.contains("<style>"), "{html}");
        assert!(html.contains("var(--mdpp-accent)"), "{html}");
        // 不再套正文作用域
        assert!(!html.contains(".mdpp-content"), "{html}");
    }

    /// 正文里的 `</style` 只是普通文字 —— 让它提前闭合，后面的字就会漏到页面上
    #[test]
    fn css_cannot_close_the_style_block_early() {
        let html = render("::css\n  a { } </style><p>顶出来了</p>\n");
        assert_eq!(html.matches("</style>").count(), 1, "{html}");
        assert!(
            html.contains("<p>顶出来了</p>"),
            "它仍然只是普通正文：{html}"
        );
    }

    /// 默认过滤掉会执行脚本的东西，其余标记原样落进页面
    #[test]
    fn html_template_filters_scripts_by_default() {
        let html = render("::html\n  <b>粗</b><script>alert(1)</script>\n");
        assert!(html.contains("<b>粗</b>"), "{html}");
        assert!(!html.contains("<script"), "默认必须过滤掉脚本：{html}");
    }

    /// 显式写了 `js` 才放行脚本 —— 默认安全，要开就得自己写出来
    #[test]
    fn html_template_allows_scripts_only_when_asked() {
        let html = render("::html js\n  <script>alert(1)</script>\n");
        // 脚本被提取成资源后内联，带上 mdpp-js 类；内容还在
        assert!(html.contains("alert(1)"), "{html}");
        assert!(html.contains("mdpp-js"), "{html}");
        assert!(html.contains("<script"), "{html}");
    }

    /// `{{param}}` 在写出去之前被填掉 —— 要执行的那几个块都得过这一遍
    #[test]
    fn placeholders_are_filled_from_params() {
        let html = render("::css color=red\n  .a { color: {{color}}; }\n");
        assert!(html.contains("color: red"), "{html}");
    }

    /// 单页模式：重复的 `::css` 按内容去重，只收集一次
    #[test]
    fn duplicate_css_is_inlined_once() {
        let html = render("::css\n  .a { color: red }\n\n::css\n  .a { color: red }\n");
        assert_eq!(html.matches("<style>").count(), 1, "{html}");
    }

    /// 所有由本库生成的脚本都带生成器版本与挂载方式
    #[test]
    fn scripts_carry_generator_environment() {
        let html = render("::js\n  console.log(1)\n");
        assert!(html.contains(r#"data-mdpp-version="0.1.0""#), "{html}");
        assert!(html.contains(r#"data-mdpp-mode="inline""#), "{html}");

        let html = render("::html js\n  <script>alert(1)</script>\n");
        assert!(html.contains(r#"data-mdpp-version="0.1.0""#), "{html}");
        assert!(html.contains(r#"data-mdpp-mode="inline""#), "{html}");
    }

    /// 脚本不在调用点出现，而是统一放到片段末尾；普通正文在它前面
    #[test]
    fn js_is_moved_to_the_end_of_the_fragment() {
        let html = render("正文\n\n::js\n  console.log(1)\n");
        let body = html.find("<p>正文</p>").expect("正文");
        let script = html.find("<script").expect("脚本");
        assert!(script > body, "{html}");
    }

    /// 单页模式：重复的 `::js` 只内联一次
    #[test]
    fn duplicate_js_is_inlined_once() {
        let html = render("::js\n  console.log(1)\n\n::js\n  console.log(1)\n");
        assert_eq!(html.matches("<script").count(), 1, "{html}");
    }

    /// 内容不同的 CSS 不会被误去重
    #[test]
    fn different_css_is_not_deduped() {
        let html = render("::css\n  .a { }\n\n::css\n  .b { }\n");
        assert_eq!(html.matches("<style>").count(), 2, "{html}");
    }

    /// 站点模式：CSS 走 styles，脚本仍然外链并进资源清单
    #[test]
    fn linked_mode_returns_assets_and_links() {
        use crate::{AssetKind, Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::css\n  .a { color: red }\n\n::js\n  console.log(1)\n",
            &RenderContext::new(),
            "assets",
        );
        assert_eq!(rendered.styles, vec![".a { color: red }"]);
        let html = rendered.html_with_styles();
        assert!(html.contains("<script src=\"assets/mdpp-"), "{}", html);
        assert!(html.contains("data-mdpp-version=\"0.1.0\""), "{}", html);
        assert_eq!(rendered.assets.len(), 1, "{:?}", rendered.assets);
        assert_eq!(rendered.assets[0].kind, AssetKind::Js);
    }

    /// 站点模式：重复资源只出现一次链接、清单里只有一份
    #[test]
    fn linked_mode_dedupes() {
        use crate::{Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::css\n  .a { }\n\n::css\n  .a { }\n",
            &RenderContext::new(),
            "assets",
        );
        assert_eq!(rendered.styles, vec![".a { }"]);
        assert!(rendered.assets.is_empty(), "{:?}", rendered.assets);
    }

    /// 前缀按调用方给的原样用（嵌套页面传 `../../assets`）
    #[test]
    fn linked_mode_uses_the_given_prefix() {
        use crate::{Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::js\n  console.log(1)\n",
            &RenderContext::new(),
            "../../assets",
        );
        let html = rendered.html_with_styles();
        assert!(html.contains("src=\"../../assets/mdpp-"), "{}", html);
    }

    /// 行号、起始行与强调行只作为 `data-*` 提示交出去，怎么摆出来是界面那一层的事
    #[test]
    fn code_template_carries_the_frontend_hints() {
        let html = render(
            "::code lang=rust lines=on start=10 highlight=10-11\n  fn main() {}\n  // 注释\n",
        );
        assert!(
            html.contains(r#"<pre class="mdpp-template-code""#),
            "{html}"
        );
        assert!(html.contains(r#"data-mdpp-lines="on""#), "{html}");
        assert!(html.contains(r#"data-mdpp-line-start="10""#), "{html}");
        assert!(html.contains(r#"data-mdpp-highlight="10-11""#), "{html}");
        assert!(html.contains(r#"<code class="language-rust">"#), "{html}");
        assert!(html.contains("fn main() {}"), "{html}");
    }

    /// start= 会自动开行号；强调范围按显示行号校验并规范化
    #[test]
    fn code_start_implies_line_numbers_and_highlights_are_normalized() {
        let html = render("::code start=10 highlight=\"10-11, 13\"\n  a\n  b\n  c\n  d\n");
        assert!(html.contains(r#"data-mdpp-lines="on""#), "{html}");
        assert!(html.contains(r#"data-mdpp-line-start="10""#), "{html}");
        assert!(html.contains(r#"data-mdpp-highlight="10-11,13""#), "{html}");
    }

    /// source / fold 才会生成 figure；source 链接安全，fold 默认展开与否可指定
    #[test]
    fn code_source_and_fold_are_structural() {
        let html = render(
            "::code lang=rust source=https://example.com/a.rs source-text=源码 fold=open\n  fn main() {}\n",
        );
        assert!(html.contains(r#"<figure class="mdpp-code">"#), "{html}");
        assert!(
            html.contains(r#"<figcaption class="mdpp-code__head">"#),
            "{html}"
        );
        assert!(html.contains(r#"<a class="mdpp-code__source""#), "{html}");
        assert!(
            html.contains(r#"href="https://example.com/a.rs""#),
            "{html}"
        );
        assert!(html.contains(r#"target="_blank""#), "{html}");
        assert!(html.contains(r#"rel="noopener noreferrer""#), "{html}");
        assert!(html.contains("源码"), "{html}");
        assert!(
            html.contains(r#"<details class="mdpp-code__fold" open="open">"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<summary class="mdpp-code__summary">"#),
            "{html}"
        );

        let closed = render("::code fold=on\n  fn main() {}\n");
        assert!(
            closed.contains(r#"<details class="mdpp-code__fold">"#),
            "{closed}"
        );
        assert!(!closed.contains(r#"open="open""#), "{closed}");
    }

    /// 没有 source / fold 时保持裸 `<pre>`，不额外套 figure
    #[test]
    fn code_without_source_or_fold_stays_a_plain_pre() {
        let html = render("::code lines=on\n  fn main() {}\n");
        assert!(!html.contains("<figure"), "{html}");
        assert!(!html.contains("mdpp-code__fold"), "{html}");
    }

    /// 参数错误要在后端一次说清楚
    #[test]
    fn code_refuses_bad_options() {
        let cases = [
            ("::code fold=maybe\n  a\n", "fold="),
            ("::code source=javascript:alert(1)\n  a\n", "source="),
            ("::code source-text=源码\n  a\n", "source-text="),
            ("::code lines=maybe\n  a\n", "lines="),
            ("::code start=0\n  a\n", "start="),
            ("::code lines=off start=2\n  a\n", "start="),
            ("::code highlight=0-1\n  a\n", "highlight="),
            ("::code highlight=2-1\n  a\n", "highlight="),
            ("::code highlight=5\n  a\n", "highlight="),
            ("::code highlight=\n  a\n", "highlight="),
        ];
        for (source, why) in cases {
            let html = render(source);
            assert!(
                html.contains("mdpp-template--problem"),
                "应拒绝 {source:?}: {html}"
            );
            assert!(html.contains(why), "错误信息应提到 {why}: {html}");
        }
    }

    /// 代码里的记号一个都不动 —— 被 markdown 解析过就不是代码块了
    #[test]
    fn code_template_does_not_parse_markdown() {
        // 代码里的记号不该被当成 markdown（这是「像代码块」的关键）
        let html = render("::code\n  **不该变粗**\n");
        assert!(html.contains("**不该变粗**"), "{html}");
        assert!(!html.contains("<strong>"), "{html}");
    }

    /// `::mermaid`：原文照收（图定义里的记号不该被 markdown 解析），
    /// 另存一份在 `data-mdpp-source` 上给「切主题重画」用
    #[test]
    fn mermaid_keeps_its_source_verbatim() {
        let html =
            render("::mermaid\n  graph TD\n    A[开始] --> B{行不行}\n     B -->|行| C[干活]\n");
        assert!(html.contains(r#"<pre class="mdpp-mermaid""#), "{html}");
        assert!(
            html.contains("A[开始] --&gt; B{行不行}"),
            "箭头照原样：{html}"
        );
        assert!(html.contains("data-mdpp-source="), "原文要留一份：{html}");
        // 图定义里的 `-->` `{}` 不该变成 HTML 标签
        assert!(!html.contains("<b>"), "{html}");
    }

    /// 里面还没有图就如实说，而不是摆一个画不出东西的空框
    #[test]
    fn mermaid_without_content_says_so() {
        let html = render("::mermaid\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("里面还没有图"), "{html}");
    }

    /// `::math`：TeX 原文进 `data-mdpp-tex`，元素里留着原文当兜底
    #[test]
    fn math_template_hands_the_source_to_the_frontend() {
        let html = render("::math\n  \\frac{a}{b} = c_1 * d\n");
        assert!(html.contains("math--display"), "{html}");
        assert!(
            html.contains(r#"data-mdpp-tex="\frac{a}{b} = c_1 * d""#),
            "{html}"
        );
        // `*` 与 `_` 是公式的一部分，不该被当成强调
        assert!(!html.contains("<em>"), "{html}");
    }

    /// 多行的 TeX 里那些 markdown 记号（`#`、`*`、`_`、行首的 `-`）都不该动
    #[test]
    fn multi_line_tex_is_left_alone() {
        let html = render(
            "::math\n  \\begin{aligned}\n  a &= b_1 \\\\\n  # 行内的井号\n  \\end{aligned}\n",
        );
        assert!(html.contains("# 行内的井号"), "{html}");
        assert!(!html.contains("<h1"), "{html}");
        assert!(!html.contains("<em>"), "{html}");
    }
}
