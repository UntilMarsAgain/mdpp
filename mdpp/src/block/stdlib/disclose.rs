//! 折叠原语：`::disclose`。
//!
//! 它是 `fold` 那一类块的底座：用 HTML 原生的 `<details>` / `<summary>`，没有脚本
//! 也能点开。**状态机是浏览器的**，所以这里不接变体组、不做延迟资源作用域。
//!
//! 与 `::variants` 的分工：
//!
//! - `::disclose`：**一份正文** + 布尔状态（开/关）。`fold` 属于这一类；
//! - `::variants`：**N 份正文** + 选中状态。`theme` / `tabs` 属于那一类。
//!
//! 参数（全部由模板传入，块自己不写死类名）：
//!
//! - `class=`：`<details>` 的类名；
//! - `button-class=`：`<summary>` 的类名；
//! - `title=` / `default-title=`：标题，缺省时用后者；
//! - `prefix=` / `prefix-class=`：标题前的记号（提示框的 ℹ / ★ …），
//!   记号带 `aria-hidden="true"`；
//! - `collapsible=`：`on` / `closed` / `off`。`on` 与 `closed` 走 `<details>`，
//!   `off` 走「标题 + 正文」的静态形状（提示框没开折叠时就是它）；不写则看
//!   `default=`；
//! - `default=`：`details` / `plain`，`collapsible=` 不写时的形状，默认 `details`；
//! - `open=`：`details` 形状的初始展开。认 `open` / `opened` / `true` / `yes` / `on` / `1`，
//!   其余（含 `closed` / `collapsed` / `off`）都按折着处理；
//! - `color=`：标题栏底色（只认 `#rgb` / `#rrggbb`），字色按亮度算；
//! - `name=`：出问题时诊断框里显示的名字（模板传 `fold` / `note` …）；
//! - `empty-why=`：正文为空时摆在诊断框里的那句话。
//!
//! 参数与颜色都在**写出任何标签之前**验完：非法 `color=` 不会留下半个
//! `<details>`。这是它留在 Rust 而不是纯模板里的主要理由。

use std::collections::HashSet;

use super::super::dispatch::render_problem;
use super::{hex_why, normalize_hex, text_on};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::disclose` 认的全部参数。
const PARAMS: &[&str] = &[
    "class",
    "button-class",
    "title",
    "default-title",
    "prefix",
    "prefix-class",
    "collapsible",
    "default",
    "open",
    "color",
    "name",
    "empty-why",
];

/// 标题 + 正文的两种形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `<details>` / `<summary>`：可以折叠
    Details,
    /// `<p>` 标题 + 正文：静态形状
    Plain,
}

/// 解析后的参数。全部借用 `block`，渲染时不再回头翻参数表。
struct Params<'a> {
    class: Option<&'a str>,
    button_class: Option<&'a str>,
    title: &'a str,
    default_title: &'a str,
    prefix: &'a str,
    prefix_class: Option<&'a str>,
    collapsible: Option<&'a str>,
    default: Shape,
    open: bool,
    color: Option<&'a str>,
    name: &'a str,
    empty_why: &'a str,
}

/// 非空白的参数值（前后空白去掉）；空串等于没写。
fn present(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// `open=` 的宽松解释：只有明确写成开的那几个词才算开。
///
/// 不在这里报错：`fold` 过去就把认不出的值当折着，作者写 `open=也许` 时看到的是
/// 一个折着的块，而不是一个报错框 —— 保持这个行为。
fn is_open(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "open" | "opened" | "true" | "yes" | "on" | "1"
    )
}

/// 认一遍参数。重复键与不认识的键都报出来 —— 类名写错不该静悄悄落空。
fn parse(block: &Block) -> Result<Params<'_>, String> {
    let mut seen = HashSet::new();
    let mut class = None;
    let mut button_class = None;
    let mut title = "";
    let mut default_title = "";
    let mut prefix = "";
    let mut prefix_class = None;
    let mut collapsible = None;
    let mut default = Shape::Details;
    let mut open = false;
    let mut color = None;
    let mut name = "disclose";
    let mut empty_why = "内容是空的：::disclose 要包裹点东西";

    for (key, value) in &block.params {
        if !seen.insert(key.as_str()) {
            return Err(format!("`::disclose` 的参数 `{key}` 写了两次"));
        }
        match key.as_str() {
            "class" => class = present(value),
            "button-class" => button_class = present(value),
            "title" => title = value.trim(),
            "default-title" => default_title = value.trim(),
            "prefix" => prefix = value.trim(),
            "prefix-class" => prefix_class = present(value),
            "collapsible" => collapsible = present(value),
            "default" => {
                default = match value.trim().to_lowercase().as_str() {
                    "details" => Shape::Details,
                    "plain" => Shape::Plain,
                    other => {
                        return Err(format!(
                            "`::disclose` 的 default= 只认 details / plain，写成了 `{other}`"
                        ));
                    }
                };
            }
            "open" => open = is_open(value),
            "color" => color = present(value),
            "name" => name = present(value).unwrap_or("disclose"),
            "empty-why" => {
                empty_why = present(value).unwrap_or("内容是空的：::disclose 要包裹点东西")
            }
            other => {
                return Err(format!(
                    "`::disclose` 不认参数 `{other}`：只允许 {}",
                    PARAMS.join(" / ")
                ));
            }
        }
    }

    Ok(Params {
        class,
        button_class,
        title,
        default_title,
        prefix,
        prefix_class,
        collapsible,
        default,
        open,
        color,
        name,
        empty_why,
    })
}

/// 诊断框里要显示模板调用方给的名字（`fold` / `note` …），而不是 `disclose`。
fn named(block: &Block, name: &str) -> Block {
    let mut block = block.clone();
    block.name = name.to_string();
    block
}

/// `collapsible=` 定形状；不写时用 `default=`。
///
/// `on` / `closed` 都是 `<details>`，差别只在初始开合；`off` 是静态的
/// 「标题 + 正文」，提示框没开折叠时走的就是它。
fn shape_of(params: &Params<'_>) -> Result<(Shape, bool), String> {
    let Some(collapsible) = params.collapsible else {
        return Ok((params.default, params.open));
    };
    match collapsible.trim().to_lowercase().as_str() {
        "on" | "true" | "yes" | "1" | "open" | "opened" => Ok((Shape::Details, true)),
        "closed" | "collapsed" => Ok((Shape::Details, false)),
        "off" | "false" | "no" | "0" => Ok((Shape::Plain, false)),
        other => Err(format!(
            "`::disclose` 的 collapsible= 只认 on / closed / off，写成了 `{other}`"
        )),
    }
}

/// 标题那一行：可选的记号 + 字面标题。`details` 里是 `<summary>`，
/// 静态形状里是 `<p>`，两处共用这一份，免得记号的写法漂成两套。
fn write_title(
    fmt: &mut dyn Renderer,
    tag: &str,
    attrs: &[(&str, String)],
    prefix: &str,
    prefix_class: Option<&str>,
    title: &str,
) {
    fmt.open(tag, attrs);
    if !prefix.is_empty() {
        let mut prefix_attrs: Vec<(&str, String)> = Vec::new();
        if let Some(class) = prefix_class {
            prefix_attrs.push(("class", class.to_string()));
        }
        // 记号是纯装饰；读屏软件念标题就够了
        prefix_attrs.push(("aria-hidden", "true".to_string()));
        fmt.open("span", &prefix_attrs);
        fmt.text(prefix);
        fmt.close("span");
    }
    fmt.text(title);
    fmt.close(tag);
}

/// `::disclose class=… button-class=… title=… open=… color=…`
pub(super) fn render_disclose(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let params = match parse(block) {
        Ok(params) => params,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    if node.children.is_empty() {
        let block = named(block, params.name);
        render_problem(&block, fmt, params.empty_why);
        return;
    }

    // 颜色先验完：等 `<details>` 开出去再发现颜色不对，页面上就留了半个框
    let mut title_style = String::new();
    if let Some(color) = params.color {
        let Some(hex) = normalize_hex(color) else {
            let block = named(block, params.name);
            render_problem(&block, fmt, &hex_why("color", None));
            return;
        };
        title_style = format!("background: {hex}; color: {};", text_on(&hex));
    }

    let title = if params.title.is_empty() {
        params.default_title
    } else {
        params.title
    };

    let (shape, open) = match shape_of(&params) {
        Ok(shape) => shape,
        Err(why) => {
            let block = named(block, params.name);
            render_problem(&block, fmt, &why);
            return;
        }
    };

    let mut title_attrs: Vec<(&str, String)> = Vec::new();
    if let Some(class) = params.button_class {
        title_attrs.push(("class", class.to_string()));
    }
    if !title_style.is_empty() {
        title_attrs.push(("style", title_style));
    }

    fmt.cr();
    match shape {
        Shape::Details => {
            let mut detail_attrs: Vec<(&str, String)> = Vec::new();
            if let Some(class) = params.class {
                detail_attrs.push(("class", class.to_string()));
            }
            if open {
                detail_attrs.push(("open", String::new()));
            }
            fmt.open("details", &detail_attrs);
            fmt.cr();
            write_title(
                fmt,
                "summary",
                &title_attrs,
                params.prefix,
                params.prefix_class,
                title,
            );
            fmt.cr();
            fmt.contents(&node.children);
            fmt.cr();
            fmt.close("details");
            fmt.cr();
        }
        Shape::Plain => {
            write_title(
                fmt,
                "p",
                &title_attrs,
                params.prefix,
                params.prefix_class,
                title,
            );
            fmt.cr();
            fmt.contents(&node.children);
            fmt.cr();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 最基本的形状：details / summary / 标题 / 正文
    #[test]
    fn renders_details_summary_and_body() {
        let html = render("::disclose title=草稿 class=fold button-class=fold--custom\n  正文\n");
        assert!(html.contains(r#"<details class="fold">"#), "{html}");
        assert!(
            html.contains(r#"<summary class="fold--custom">草稿</summary>"#),
            "{html}"
        );
        assert!(html.contains("正文"), "{html}");
    }

    /// 标题缺省时用 `default-title`；标题是字面文本，不按 markdown 解析
    #[test]
    fn title_falls_back_and_stays_literal() {
        let html = render("::disclose default-title=更多\n  正文\n");
        assert!(html.contains(">更多</summary>"), "{html}");

        let literal = render("::disclose title=**不是粗体**\n  正文\n");
        assert!(literal.contains("**不是粗体**"), "{literal}");
        assert!(!literal.contains("<strong>不是粗体</strong>"), "{literal}");
    }

    /// `open=` 认那几个开值；别的写法按折着处理
    #[test]
    fn open_accepts_the_open_words_only() {
        for value in ["open", "opened", "true", "yes", "on", "1"] {
            let html = render(&format!("::disclose open={value}\n  正文\n"));
            assert!(
                html.contains(r#"<details open="">"#),
                "open={value}: {html}"
            );
        }
        for value in ["closed", "collapsed", "off", "no", "false", "0", "也许"] {
            let html = render(&format!("::disclose open={value}\n  正文\n"));
            assert!(!html.contains(" open=\""), "open={value}: {html}");
        }
    }

    /// 标题前的记号带 `aria-hidden`，并可以用单独的类
    #[test]
    fn prefix_is_a_hidden_span() {
        let html = render("::disclose title=说明 prefix=ℹ prefix-class=callout__mark\n  正文\n");
        assert!(
            html.contains(
                r#"<span class="callout__mark" aria-hidden="true">ℹ</span>说明</summary>"#
            ),
            "{html}"
        );
    }

    /// 底色只认十六进制；字色跟着亮度走
    #[test]
    fn colour_is_validated_before_anything_is_written() {
        let good = render("::disclose color=#0055a4 title=深蓝\n  正文\n");
        assert!(
            good.contains(r#"<summary style="background: #0055a4; color: #f5f5f5;">"#),
            "{good}"
        );

        let bad = render("::disclose color=red title=乱写\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(
            !bad.contains("<details"),
            "坏颜色不该留下半个 details：{bad}"
        );
    }

    /// 空正文说清楚；诊断框用调用方给的名字
    #[test]
    fn an_empty_body_is_reported_with_the_caller_name() {
        let html = render("::disclose name=fold empty-why=没有内容\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("没有内容"), "{html}");
        assert!(
            html.contains(r#"<code class="template__name">fold</code>"#),
            "{html}"
        );
    }

    /// `collapsible=off` / `default=plain` 是「标题 + 正文」，没有 details
    #[test]
    fn plain_shape_has_a_paragraph_title_and_no_details() {
        let plain = render("::disclose default=plain title=说明 prefix=ℹ\n  正文\n");
        assert!(!plain.contains("<details"), "{plain}");
        assert!(
            plain.contains("<p><span aria-hidden=\"true\">ℹ</span>说明</p>"),
            "没有 button-class 时不该凭空冒类名：{plain}"
        );

        let off =
            render("::disclose collapsible=off button-class=callout__title title=说明\n  正文\n");
        assert!(!off.contains("<details"), "{off}");
        assert!(off.contains(r#"<p class="callout__title">"#), "{off}");
    }

    /// `collapsible=on` / `closed` 走 details；`closed` 不带 open
    #[test]
    fn collapsible_controls_details_and_initial_state() {
        let open = render("::disclose collapsible=on title=说明\n  正文\n");
        assert!(open.contains(r#"<details open="">"#), "{open}");

        let closed = render("::disclose collapsible=closed title=说明\n  正文\n");
        assert!(closed.contains("<details"), "{closed}");
        assert!(!closed.contains(" open=\""), "{closed}");

        let bad = render("::disclose collapsible=maybe\n  正文\n");
        assert!(bad.contains("template--problem"), "{bad}");
        assert!(!bad.contains("<details"), "{bad}");
    }

    /// 未知参数与重复参数都报出来
    #[test]
    fn bad_params_are_reported() {
        for source in [
            "::disclose title=x title=y\n  正文\n",
            "::disclose tite=x\n  正文\n",
            "::disclose default=折叠\n  正文\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{source:?}: {html}");
        }
    }
}
