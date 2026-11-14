//! 随库自带的那些块。
//!
//! **有什么块只写在这一个文件** —— 名字在 [`NAMES`]、挂载在 [`register`]，两者由
//! `the_two_tables_agree` 盯着。加一个块 = 写一个渲染器 + 在两处各加一行。
//!
//! 渲染器按**块内容怎么处理**分成几组：
//!
//! - `text`：原样文本原语，正文不解析 markdown，只做 HTML 转义。`fields` /
//!   `banner` / `aside` / `quote` / `title` / `signature` 等已迁到标准库模板。
//! - `media`：把一个文件摆出来（`image` / `video` / `audio`）。三者共用一套 `src=`
//!   校验、尺寸上限与图注规矩。
//! - `disclose`：一份正文 + 布尔状态的原生折叠原语，`fold` 由它加模板实现。
//! - `each`：把一段模板套在分节上重复渲染，类名与结构全由模板片段决定。
//! - `rows`：把一段模板套在「每行若干格」的原始行上，`fields` 用它做两列表格。
//! - `variants`：N 份正文 + 选中状态的变体组原语，`theme` / `tabs` 由它加模板实现。
//! - `navbox`：按 `[标签]` 分组的导航框；标签同时就是分组名。
//! - `embeds`：把内容**原样写出去**的块（`code` / `mermaid` / `math` / `css` /
//!   `html` / `js`），注册为 [`ContentMode::Raw`]，正文一个字符都不解析。
//! - `control`：控制流（`if`）。按条件渲染 `[then]` / `[else]`，不产生自己的 DOM。
//! - `include`：`noinclude` / `includeonly` —— 页面直接渲染与模板引入两种语境。
//! - `inline`：把正文按行内 markdown 渲染，不产生外层元素。
//! - `problem`：诊断框原语；写框的同时把这次错误记进渲染报告。
//! - `style-vars`：带受控 CSS 自定义属性的盒子；`--名字-bg` 自动补字色。
//! - `wrapper`：基础包装（`element` / `surface` / `style-vars`）。`element` 选标签、挂
//!   受控属性，`surface` 管单个盒子的外观，`style-vars` 管受控 CSS 自定义属性。
//!
//! `columns`、`fields`、`fold`、`theme`、`tabs`、`banner`、提示框 `note` / `tip` /
//! `warning` / `danger` / `error` 与 `aside` 已迁到标准库模板。
//!
//! 本文件另有三个多组共用的东西：`size_rule`（尺寸参数 → 一条 CSS 声明）、
//! `normalize_hex`（`color=` 只认十六进制）与 `text_on`（底色 → 字色）。

mod control;
mod demo;
mod disclose;
mod each;
mod embeds;
mod include;
mod inline;
mod media;
mod navbox;
mod problem;
mod rows;
mod text;
mod variants;
mod wrapper;

use markdown_it::Node;

use control::render_if;
use demo::render_demo;
use disclose::render_disclose;
use each::render_each;
use embeds::{render_code, render_css, render_html, render_js, render_math, render_mermaid};
use include::{render_includeonly, render_noinclude};
use inline::render_inline;
use media::{render_audio, render_image, render_video};
use navbox::render_navbox;
use rows::render_rows;
use text::render_text;
use variants::render_variants;
use wrapper::{render_element, render_style_vars, render_surface};

use crate::registry::{Block, BlockRegistry, BlockSpec, ContentMode};

/// 一条注册项：把函数指针接成渲染器。参数收**函数指针**而不是 trait 对象，是为了
/// 下面那串 `spec("disclose", Markdown, render_disclose)` 读起来就是一句话，不必每次装箱。
fn spec(
    name: &str,
    mode: ContentMode,
    renderer: fn(&Block, &Node, &mut dyn markdown_it::Renderer),
) -> BlockSpec {
    BlockSpec::plain(name, mode, renderer)
}

/// 全部内置块的名字，按作者写下来的顺序。它与 [`register`] 里挂的那一批必须一致 ——
/// `the_two_tables_agree` 盯着这件事。
pub const NAMES: &[&str] = &[
    "code",
    "rows",
    "image",
    "video",
    "audio",
    "css",
    "html",
    "js",
    "mermaid",
    "math",
    "demo",
    "disclose",
    "each",
    "element",
    "if",
    "includeonly",
    "inline",
    "noinclude",
    "problem",
    "style-vars",
    "surface",
    "text",
    "navbox",
    "variants",
];

/// 把内置块灌进注册表。
pub fn register(registry: &mut BlockRegistry) {
    registry
        .register(spec("code", ContentMode::Raw, render_code))
        .register(spec("image", ContentMode::Raw, render_image))
        .register(spec("video", ContentMode::Raw, render_video))
        .register(spec("audio", ContentMode::Raw, render_audio))
        .register(spec("css", ContentMode::Raw, render_css))
        .register(spec("html", ContentMode::Raw, render_html))
        .register(spec("js", ContentMode::Raw, render_js))
        .register(spec("mermaid", ContentMode::Raw, render_mermaid))
        .register(spec("math", ContentMode::Raw, render_math))
        .register(spec("demo", ContentMode::Raw, render_demo))
        .register(spec("disclose", ContentMode::Markdown, render_disclose))
        .register(spec("each", ContentMode::Sections, render_each))
        .register(spec("element", ContentMode::Markdown, render_element))
        .register(spec("if", ContentMode::Sections, render_if))
        .register(spec(
            "includeonly",
            ContentMode::Markdown,
            render_includeonly,
        ))
        .register(spec("inline", ContentMode::Raw, render_inline))
        .register(spec("noinclude", ContentMode::Markdown, render_noinclude))
        .register(spec("problem", ContentMode::Raw, problem::render_problem))
        .register(spec("style-vars", ContentMode::Markdown, render_style_vars))
        .register(spec("surface", ContentMode::Markdown, render_surface))
        .register(spec("text", ContentMode::Raw, render_text))
        .register(spec("navbox", ContentMode::Sections, render_navbox))
        .register(spec("rows", ContentMode::Sections, render_rows))
        .register(spec("variants", ContentMode::Sections, render_variants));
}

/// 全部内置块的名字（诊断与命令行帮助按它列出来）
pub fn names() -> Vec<&'static str> {
    NAMES.to_vec()
}

/// 非空白的参数值（前后空白去掉）；空串等于没写。
pub(crate) fn present(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// 模板变量前缀：不能为空，也不能含空白/控制字符或模板语法字符。
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || matches!(ch, '.' | '@' | '{' | '}'))
}

/// 尺寸值：数字 + 白名单单位。`default_px` 决定空单位是否补 `px`。
///
/// 严格是有理由的：这个值会被写进 `style` 属性，宽松了就等于允许往里面塞任意声明
/// （比如 `width="1px; background: url(//追踪地址)"`）。
fn size_value(value: &str, default_px: bool) -> Option<String> {
    let trimmed = value.trim();
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    if digits.is_empty() || digits.parse::<f64>().is_err() {
        return None;
    }
    let unit = &trimmed[digits.len()..];
    if !matches!(unit, "" | "px" | "%" | "em" | "rem" | "vh" | "vw") {
        return None;
    }
    let unit = if unit.is_empty() && default_px {
        "px"
    } else {
        unit
    };
    Some(format!("{digits}{unit}"))
}

/// 尺寸参数 → 一条 CSS 声明；空单位按 `px` 处理。
pub(crate) fn size_rule(name: &str, value: &str) -> Option<String> {
    Some(format!("{name}: {};", size_value(value, true)?))
}

/// 自定义属性里的尺寸值：保留空单位（`--gap: 2` 可能给 `calc()` 用）。
pub(crate) fn css_size(value: &str) -> Option<String> {
    size_value(value, false)
}

/// `#abc` / `#aabbcc` → 规范的 `#aabbcc`；别的写法一律不认。
///
/// 放在这一层而不是某个块里：`surface`、`fold` 与 `navbox` 都认 `color=`，
/// 留在任一组都会让另一组跨界来取 —— 与 [`size_rule`] 是同一个理由。
pub(crate) fn normalize_hex(text: &str) -> Option<String> {
    let body = text.trim().strip_prefix('#')?;
    let expanded: String = match body.len() {
        3 => body.chars().flat_map(|ch| [ch, ch]).collect(),
        6 => body.to_string(),
        _ => return None,
    };
    if !expanded.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", expanded.to_lowercase()))
}

/// [`normalize_hex`] 认不出来时摆给作者看的那一句，写在**一处**。
///
/// 校验本身已经是 [`normalize_hex`] 一份了，可这句话还有几个地方要说（`surface`、
/// `fold`、导航框的几个底色）—— 措辞各写一份必然漂移，而作者是照着这句话去改的：
/// 少说一种写法，他就会去试那一种。第二个参数是**举个例子**
/// （`"group-bg"` → 末尾补 `，比如 group-bg=#0055a4`），`None` 就只说两种写法
pub(crate) fn hex_why(key: &str, example: Option<&str>) -> String {
    let mut why = format!("{key}= 只认 #rgb 或 #rrggbb 这两种写法");
    if let Some(example) = example {
        why.push_str(&format!("，比如 {key}={example}"));
    }
    why
}

/// 底色 → 该配什么颜色的字。
///
/// 作者只给底色，字色由这里定：深底配深字是最常见的「自己给自己挖坑」。
/// 用 sRGB 亮度的常见近似，够用且一眼能看懂为什么这么算。
pub(crate) fn text_on(background: &str) -> &'static str {
    let Some(body) = background.strip_prefix('#') else {
        return "var(--mdpp-text)";
    };
    let Ok(value) = u32::from_str_radix(body, 16) else {
        return "var(--mdpp-text)";
    };
    let (r, g, b) = ((value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff);
    let luminance = (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0;
    if luminance > 0.6 {
        "#101010"
    } else {
        "#f5f5f5"
    }
}

/// 取一行做成的图注：`caption=` 优先，其次块里写着的那一行（缩进与空行都不算）。
///
/// 媒体那一组共用这一份 —— 分开写就会各抄一份，而两处迟早会对不上。
pub(crate) fn caption_of(block: &Block) -> Option<String> {
    match block.param("caption") {
        Some(caption) => Some(caption.trim().to_string()).filter(|text| !text.is_empty()),
        None => {
            let text = block
                .body
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Some(text).filter(|text| !text.is_empty())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Mdpp;

    /// 两张表一旦对不上，写下的名字会被报成「未知块」，而「要不要分节」却已经按
    /// 另一张表决定了 —— 这种半截状态最难查
    #[test]
    fn the_two_tables_agree() {
        let registry = BlockRegistry::builtin();
        let mut registered = registry.names();
        registered.sort_unstable();
        let mut named = NAMES.to_vec();
        named.sort_unstable();
        assert_eq!(registered, named, "挂载的块与列出来的块对不上");
        assert_eq!(
            registered.len(),
            named.len(),
            "两张表里有重名：{registered:?}"
        );
    }

    /// 每个内置块都得能渲染出东西 —— 否则是「表里有、实际不工作」，
    /// 而那比「表里没有」更难发现
    #[test]
    fn every_builtin_block_actually_renders() {
        for name in NAMES {
            // 这几个要额外的东西才写得出来，给一份最小的可用写法
            let source = match *name {
                "image" | "video" | "audio" => format!("::{name} src=x.png\n  注\n"),
                // 变体组至少要 control= 与一节 [标签]
                "variants" => format!("::{name} control=choice\n  [甲]\n    正文一行\n"),
                // 折叠原语要一个标题；正文不能为空
                "disclose" => format!("::{name} title=更多\n  正文一行\n"),
                // each 要模板节与至少一条数据
                "each" => format!(
                    "::{name}\n  [__template__]\n    {{{{@item.body}}}}\n  [__data__]\n    [甲]\n      正文一行\n"
                ),
                // rows 要模板节与至少一行数据
                "rows" => format!(
                    "::{name}\n  [__template__]\n    ::text\n      {{{{@row.0}}}}\n  [__data__]\n    甲|乙\n"
                ),
                // element 需要 class= 参数
                "element" => format!("::{name} class=foo\n  内容\n"),
                // includeonly 在页面直接渲染时是空输出，但不算用法问题
                "includeonly" | "noinclude" => format!("::{name}\n  正文一行\n"),
                "inline" => format!("::{name}\n  正文一行\n"),
                // problem 本身就是问题框；它有效时当然会输出 problem 类
                "problem" => {
                    let html = Mdpp::new().render("::problem name=示例 why=演示\n");
                    assert!(html.contains("template--problem"), "{html}");
                    assert!(html.contains("示例"), "{html}");
                    continue;
                }
                // style-vars 要一个颜色变量与正文
                "style-vars" => format!("::{name} --demo-bg=#0055a4\n  正文一行\n"),
                // if 至少要一节 [then]；用字面量条件，不依赖模板作用域
                "if" => format!("::{name} condition-exist=yes\n  [then]\n    内容\n"),
                // surface 包一层带 class 的盒子
                "surface" => format!("::{name} class=foo\n  内容\n"),
                // navbox 要一个标题与一行分组，否则那个样子是「用法有问题」的框
                "navbox" => format!("::{name} title=某导航框\n  [分类]\n    甲\n"),
                _ => format!("::{name}\n  正文一行\n"),
            };
            let html = Mdpp::new().render(&source);
            assert!(
                !html.contains("template--unknown"),
                "{name} 渲染成了未知块：{html}"
            );
            assert!(
                !html.contains("template--problem"),
                "{name} 渲染成了用法错误：{html}"
            );
        }
    }

    /// 分节档、原始档与普通档是三种不同的形状，钉住它们的登记
    #[test]
    fn the_content_modes_are_what_they_look_like() {
        let registry = BlockRegistry::builtin();
        assert_eq!(
            registry.get("variants").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
        assert_eq!(
            registry.get("if").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
        assert_eq!(
            registry.get("each").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
        assert_eq!(
            registry.get("rows").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
        assert_eq!(
            registry.get("disclose").map(BlockSpec::mode),
            Some(ContentMode::Markdown)
        );
        for name in [
            "code", "math", "html", "css", "js", "mermaid", "inline", "problem", "text",
        ] {
            assert_eq!(
                registry.get(name).map(BlockSpec::mode),
                Some(ContentMode::Raw),
                "{name} 应当是原始档"
            );
        }
    }

    /// 这个值会被写进 `style` 属性 —— 只认「数字 + 白名单单位」，别的写法一律不认
    #[test]
    fn size_rules_only_accept_numbers_and_whitelisted_units() {
        assert_eq!(
            size_rule("max-width", "320"),
            Some("max-width: 320px;".into())
        );
        assert_eq!(
            size_rule("max-width", "50%"),
            Some("max-width: 50%;".into())
        );
        assert_eq!(size_rule("max-width", "1px; background:url(//evil)"), None);
        assert_eq!(size_rule("max-width", "很久"), None);
    }

    /// `#abc` 补成六位再交出去；认不出的（`red`、位数不对的）拒绝而不猜
    #[test]
    fn hex_colors_are_normalised_or_refused() {
        assert_eq!(normalize_hex("#0af").as_deref(), Some("#00aaff"));
        assert_eq!(normalize_hex("#AABBCC").as_deref(), Some("#aabbcc"));
        assert_eq!(normalize_hex("red"), None);
        assert_eq!(normalize_hex("#12345"), None);
    }

    /// 字色跟着底色的亮度走；认不出底色时退回正文色，而不是瞎配一个
    #[test]
    fn text_colour_follows_the_background_brightness() {
        assert_eq!(text_on("#0055a4"), "#f5f5f5", "深底配浅字");
        assert_eq!(text_on("#ffe680"), "#101010", "浅底配深字");
        assert_eq!(text_on("不是颜色"), "var(--mdpp-text)");
    }
}
