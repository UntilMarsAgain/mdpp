//! 随库自带的那些块。
//!
//! **有什么块只写在这一个文件** —— 名字在 [`NAMES`]、挂载在 [`register`]，两者由
//! `the_two_tables_agree` 盯着。加一个块 = 写一个渲染器 + 在两处各加一行。
//!
//! 渲染器按**块内容怎么处理**分成五组：
//!
//! - `text`：只写文字的块（`quote` / `aside` / `title` …）。内容**照常按 markdown
//!   解析**，参数只管外框的样子。
//! - `media`：把一个文件摆出来（`image` / `video` / `audio`）。三者共用一套 `src=`
//!   校验、尺寸上限与图注规矩。
//! - `panels`：**按 `[标签]` 分节**的块（`tabs` / `theme` / `columns` / `navbox`）。
//!   它们不读
//!   块正文，而是各节已解析好的子节点；`navbox` 的标签同时就是它的分组名。
//! - `embeds`：把内容**原样写出去**的块（`code` / `mermaid` / `math` / `css` /
//!   `html` / `js`），注册为 [`ContentMode::Raw`]，正文一个字符都不解析。
//! - `callouts`：提示框（`note` / `tip` / `warning` / `danger` / `error`）。形状完全
//!   一样，差别只是「哪一种」。
//!
//! 本文件另有三个多组共用的东西：`size_rule`（尺寸参数 → 一条 CSS 声明）、
//! `normalize_hex`（`color=` 只认十六进制）与 `text_on`（底色 → 字色）。

mod callouts;
mod embeds;
mod media;
mod navbox;
mod panels;
mod text;

use markdown_it::Node;

use callouts::{render_danger, render_error, render_note, render_tip, render_warning};
use embeds::{render_code, render_css, render_html, render_js, render_math, render_mermaid};
use media::{render_audio, render_image, render_video};
use navbox::render_navbox;
use panels::{render_columns, render_tabs, render_theme};
use text::{
    render_aside, render_banner, render_fields, render_quote, render_signature, render_title,
};

use crate::registry::{Block, BlockRegistry, BlockSpec, ContentMode};

/// 一条注册项：把函数指针接成渲染器。参数收**函数指针**而不是 trait 对象，是为了
/// 下面那串 `spec("quote", Markdown, render_quote)` 读起来就是一句话，不必每次装箱。
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
    "quote",
    "code",
    "aside",
    "fields",
    "banner",
    "title",
    "tabs",
    "theme",
    "columns",
    "image",
    "video",
    "audio",
    "css",
    "html",
    "js",
    "mermaid",
    "math",
    "signature",
    "note",
    "tip",
    "warning",
    "danger",
    "error",
    "navbox",
];

/// 把内置块灌进注册表。
pub fn register(registry: &mut BlockRegistry) {
    registry
        .register(spec("quote", ContentMode::Markdown, render_quote))
        .register(spec("code", ContentMode::Raw, render_code))
        .register(spec("aside", ContentMode::Markdown, render_aside))
        .register(spec("fields", ContentMode::Raw, render_fields))
        .register(spec("banner", ContentMode::Markdown, render_banner))
        .register(spec("title", ContentMode::Markdown, render_title))
        .register(spec("tabs", ContentMode::Sections, render_tabs))
        .register(spec("theme", ContentMode::Sections, render_theme))
        .register(spec("columns", ContentMode::Sections, render_columns))
        .register(spec("image", ContentMode::Raw, render_image))
        .register(spec("video", ContentMode::Raw, render_video))
        .register(spec("audio", ContentMode::Raw, render_audio))
        .register(spec("css", ContentMode::Raw, render_css))
        .register(spec("html", ContentMode::Raw, render_html))
        .register(spec("js", ContentMode::Raw, render_js))
        .register(spec("mermaid", ContentMode::Raw, render_mermaid))
        .register(spec("math", ContentMode::Raw, render_math))
        .register(spec("signature", ContentMode::Markdown, render_signature))
        .register(spec("note", ContentMode::Markdown, render_note))
        .register(spec("tip", ContentMode::Markdown, render_tip))
        .register(spec("warning", ContentMode::Markdown, render_warning))
        .register(spec("danger", ContentMode::Markdown, render_danger))
        .register(spec("error", ContentMode::Markdown, render_error))
        .register(spec("navbox", ContentMode::Sections, render_navbox));
}

/// 全部内置块的名字（诊断与命令行帮助按它列出来）
pub fn names() -> Vec<&'static str> {
    NAMES.to_vec()
}

/// 尺寸参数 → 一条 CSS 声明。只认「数字 + 可选白名单单位」，别的写法一律不认。
///
/// 严格是有理由的：这个值会被写进 `style` 属性，宽松了就等于允许往里面塞任意声明
/// （比如 `width="1px; background: url(//追踪地址)"`）。
pub(crate) fn size_rule(name: &str, value: &str) -> Option<String> {
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
    let unit = if unit.is_empty() { "px" } else { unit };
    Some(format!("{name}: {digits}{unit};"))
}

/// `#abc` / `#aabbcc` → 规范的 `#aabbcc`；别的写法一律不认。
///
/// 放在这一层而不是 [`text`] 里：`color=` 有两组块认它（`::banner` 与提示框那一族），
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
/// 校验本身已经是 [`normalize_hex`] 一份了，可这句话还有四个地方要说（`::banner`、
/// 标题、提示框那一族、导航框的三个底色）—— 措辞各写一份必然漂移，而作者是照着
/// 这句话去改的：少说一种写法，他就会去试那一种。第二个参数是**举个例子**
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
                "tabs" => format!("::{name}\n  [甲]\n  正文一行\n"),
                "theme" => format!("::{name}\n  [light]\n  正文一行\n"),
                // 分栏与选项卡一样，每节都得有 [标签]，标签前不许有正文
                "columns" => format!("::{name}\n  [甲]\n  正文一行\n"),
                // navbox 要一个标题与一行分组，否则那个样子是「用法有问题」的框
                "navbox" => format!("::{name} title=某导航框\n  [分类]\n  甲\n"),
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
            registry.get("tabs").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
        assert_eq!(
            registry.get("columns").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
        assert_eq!(
            registry.get("quote").map(BlockSpec::mode),
            Some(ContentMode::Markdown)
        );
        for name in ["code", "math", "html", "css", "js", "mermaid", "fields"] {
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
