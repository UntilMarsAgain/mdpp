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
use markdown_it::{Node, Renderer};

/// `::js` —— 把内容当**原始 JavaScript** 注入这一页，随页面加载执行。
///
/// 与 `::html` 一个规矩：内容原样，`{{param}}` 会替换。不过滤 ——
/// 写下 `::js` 本身就是一次显式的决定，与 `::html js` 是同一个意思。
///
/// 注入的脚本里若出现 `</script`，HTML 解析器会当场收尾，后面的字会漏到页面上，
/// 所以那一处先转义（见 [`fill::escape_script_end`]）。
///
/// 单页模式下同一份脚本按内容哈希去重后内联；站点模式下输出 `<script src="…">`，
/// 内容随 [`crate::Rendered::assets`] 返回交给调用方写盘。
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
    write_script(fmt, use_asset_with(AssetKind::Js, escaped, pol));
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

/// `::code lang=rust lines=off start=10 highlight=2-3` —— 像 markdown 的代码块，
/// 但能控制行号与要强调的行。
///
/// 这里**只输出代码原文与几个 `data-*` 提示**：行号列与强调色带由使用方落地 ——
/// 拆行会破坏 highlight.js 的分词（它的 span 可能跨行），而「界面怎么显示」属于界面
/// 那一层。
pub(super) fn render_code(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let language = block.param("lang").unwrap_or("").trim();

    let mut attrs: Vec<(&str, String)> = vec![("class", "mdpp-template-code".to_string())];
    if let Some(lines) = block.param("lines") {
        let off = matches!(lines.trim(), "off" | "false" | "no");
        attrs.push((
            "data-mdpp-lines",
            if off { "off" } else { "on" }.to_string(),
        ));
    }
    if let Some(start) = block.param("start") {
        attrs.push(("data-mdpp-line-start", start.trim().to_string()));
    }
    if let Some(highlight) = block.param("highlight") {
        attrs.push(("data-mdpp-highlight", highlight.trim().to_string()));
    }

    let code_attrs: Vec<(&str, String)> = if language.is_empty() {
        Vec::new()
    } else {
        vec![("class", format!("language-{language}"))]
    };

    fmt.cr();
    fmt.open("pre", &attrs);
    fmt.open("code", &code_attrs);
    // 原文照收：代码里的 markdown 记号**不该**被解析
    fmt.text(&block.body);
    fmt.close("code");
    fmt.close("pre");
    fmt.cr();
}

/// `::css` —— 把内容（或 `src="页面名"` 指的模板页）当 CSS 注入这一页。
///
/// 注入的 CSS 与正文在**同一个文档**里（作用域收在正文那一层，见
/// [`fill::scope_css`]），所以本库默认样式表里的那些 CSS 变量
/// （`--mdpp-accent`、`--mdpp-link` …）在这里用 `var()` 直接就能取到 ——
/// 这正是「自定义块能跟着主题走」的关键。
///
/// 内容里的 `{{param}}` 会被替换；`</style` 会被去掉，免得提前闭合样式块。
///
/// 单页模式下同一份 CSS 按内容哈希去重后内联；站点模式下输出 `<link>`，
/// 内容随 [`crate::Rendered::assets`] 返回交给调用方写盘。
///
/// 在变体作用域里（`::theme` / `::tabs` / 自定义组）时，输出通用惰性占位符，
/// 由页面脚本在选项激活时挂载。
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
    // 收进正文：不然一条 `* { }` 就能把整个页面改掉
    let css = fill::scope_css(&css);
    write_css(fmt, use_asset(AssetKind::Css, css));
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
        let js_content = extract_script_contents(&filled);
        if !js_content.is_empty() {
            let escaped = fill::escape_script_end(&js_content);
            let pol = if fill::flag(block, "once") {
                AssetPolicy::Once
            } else {
                AssetPolicy::EveryActivation
            };
            write_script(fmt, use_asset_with(AssetKind::Js, escaped, pol));
        }
        let html_without_scripts = remove_script_tags(&filled);
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

/// 从 HTML 字符串中提取所有 <script>...</script> 的内容，拼接返回。
fn extract_script_contents(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let after_tag = match rest[start..].find('>') {
            Some(i) => start + i + 1,
            None => break,
        };
        if let Some(end) = rest[after_tag..].find("</script>") {
            out.push_str(&rest[after_tag..after_tag + end]);
            out.push('\n');
            rest = &rest[after_tag + end + 9..];
        } else {
            break;
        }
    }
    out
}

/// 从 HTML 字符串中移除所有 <script>...</script> 标签及其内容。
fn remove_script_tags(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        out.push_str(&rest[..start]);
        let after_tag = match rest[start..].find('>') {
            Some(i) => start + i + 1,
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        };
        if let Some(end) = rest[after_tag..].find("</script>") {
            rest = &rest[after_tag + end + 9..];
        } else {
            rest = &rest[after_tag..];
        }
    }
    out.push_str(rest);
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

    /// 注入的 CSS 与正文在同一份文档里：主题的那些变量 `var()` 直接就取得到，
    /// 作用域则收在正文那一层
    #[test]
    fn css_template_injects_style_and_our_variables() {
        let html = render("::css\n  .我有自己的样式 { color: var(--mdpp-accent); }\n");
        assert!(html.contains("<style>"), "{html}");
        assert!(html.contains("var(--mdpp-accent)"), "{html}");
        // 作用域收在正文那一层
        assert!(html.contains(".mdpp-content .我有自己的样式"), "{html}");
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

    /// 单页模式：重复的 `::css` 按内容哈希去重，只内联一次
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

    /// 站点模式：HTML 里是链接，资源内容随 `Rendered::assets` 返回
    #[test]
    fn linked_mode_returns_assets_and_links() {
        use crate::{AssetKind, Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::css\n  .a { color: red }\n\n::js\n  console.log(1)\n",
            &RenderContext::new(),
            "assets",
        );
        assert!(
            rendered
                .html
                .contains("<link rel=\"stylesheet\" href=\"assets/mdpp-"),
            "{}",
            rendered.html
        );
        assert!(
            rendered.html.contains("<script src=\"assets/mdpp-"),
            "{}",
            rendered.html
        );
        assert!(
            rendered.html.contains("data-mdpp-version=\"0.1.0\""),
            "{}",
            rendered.html
        );
        assert_eq!(rendered.assets.len(), 2, "{:?}", rendered.assets);
        assert!(
            rendered
                .assets
                .iter()
                .any(|asset| asset.kind == AssetKind::Css)
        );
        assert!(
            rendered
                .assets
                .iter()
                .any(|asset| asset.kind == AssetKind::Js)
        );
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
        assert_eq!(rendered.assets.len(), 1, "{:?}", rendered.assets);
        assert_eq!(
            rendered.html.matches("<link").count(),
            1,
            "{}",
            rendered.html
        );
    }

    /// 前缀按调用方给的原样用（嵌套页面传 `../../assets`）
    #[test]
    fn linked_mode_uses_the_given_prefix() {
        use crate::{Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::css\n  .a { }\n",
            &RenderContext::new(),
            "../../assets",
        );
        assert!(
            rendered.html.contains("href=\"../../assets/mdpp-"),
            "{}",
            rendered.html
        );
    }

    /// 行号、起始行与强调行只作为 `data-*` 提示交出去，怎么摆出来是界面那一层的事
    #[test]
    fn code_template_carries_the_frontend_hints() {
        let html = render(
            "::code lang=rust lines=off start=10 highlight=2-3\n  fn main() {}\n  // 注释\n",
        );
        assert!(
            html.contains(r#"<pre class="mdpp-template-code""#),
            "{html}"
        );
        assert!(html.contains(r#"data-mdpp-lines="off""#), "{html}");
        assert!(html.contains(r#"data-mdpp-line-start="10""#), "{html}");
        assert!(html.contains(r#"data-mdpp-highlight="2-3""#), "{html}");
        assert!(html.contains(r#"<code class="language-rust">"#), "{html}");
        assert!(html.contains("fn main() {}"), "{html}");
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
