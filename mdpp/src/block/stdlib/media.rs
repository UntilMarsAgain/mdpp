//! 媒体类：`image` / `video` / `audio` —— 把一个文件摆出来。
//!
//! 三者共用同一套规矩：
//!
//! - `src=` 必给，且协议只挡能执行或能外传数据的三种（`javascript:` / `data:` /
//!   `vbscript:`）；
//! - `width=` / `height=` 是**上限**（`max-width` / `max-height`），数字与单位都受限
//!   （见 [`size_rule`]），免得有人拿尺寸参数往 `style` 里塞别的东西；
//! - `mode=` 决定摆放方式；图片另有 `inline` 模式，像图标一样嵌进行内；
//! - 图注两种写法：`caption="…"`，或者**在块里写一行**。
//!
//! 区别只在最后落成什么标签：`::image` 是一张图（懒加载），`::video` / `::audio` 是
//! 播放器，所以后两者共用 [`render_media`]，只差一个标签名。
//!
//! `src` 写的是**正文作者看得见的那一个名字**（`片子.mp4`）；换成能取到字节的地址是
//! 渲染之后的事。

use super::super::dispatch::render_problem;
use super::{caption_of, size_rule};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// `::image src=…` —— 插入图片。`src` 必给；尺寸、模式、注释都可选：
///
/// - `mode=left|center|right`（默认居中）决定图片在正文里的位置；
/// - `mode=inline` 输出一个行内 `<img>`，高度是 `1em`，随所在行的字号变化，
///   适合当图标；此时不生成 `figure`，也不接受图注；
/// - `width=` / `height=` —— 都是**上限**（`max-width` / `max-height`），图片不会被拉变形，
///   数量与单位都受限（见 [`size_rule`]），免得有人拿尺寸参数往 `style` 里塞别的东西。
///   行内模式忽略这两个上限，避免把图标定死成固定尺寸；
/// - 注释有两种写法：**图片下面写一行**（最自然），或者 `caption="…"`。
///
/// 协议只挡能执行或能外传数据的三种：`javascript:` / `data:` / `vbscript:`。
pub(super) fn render_image(block: &Block, _node: &Node, fmt: &mut dyn Renderer) {
    let Some(source) = checked_source(block, fmt, "图片模板至少要给出图片地址") else {
        return;
    };

    let Some(mode) = mode_of(block, fmt, true) else {
        return;
    };

    if mode == "inline" {
        let has_caption = block
            .param("caption")
            .is_some_and(|caption| !caption.trim().is_empty())
            || !block.body.trim().is_empty();
        if has_caption {
            render_problem(block, fmt, "mode=inline 的图片不能写图注");
            return;
        }

        // 不写 cr：行内图片要跟相邻文字贴在同一行里。
        fmt.self_close(
            "img",
            &[
                ("class", "mdpp-image mdpp-image--inline".to_string()),
                ("src", source),
                ("alt", block.param("alt").unwrap_or("").trim().to_string()),
                ("loading", "lazy".to_string()),
            ],
        );
        return;
    }

    let style = size_style(block);

    fmt.cr();
    fmt.open(
        "figure",
        &[("class", format!("mdpp-image mdpp-image--{mode}"))],
    );
    fmt.cr();
    let mut attrs: Vec<(&str, String)> = vec![
        ("src", source),
        ("alt", block.param("alt").unwrap_or("").trim().to_string()),
        // 长文里的图片不该抢首屏带宽
        ("loading", "lazy".to_string()),
    ];
    if !style.is_empty() {
        attrs.push(("style", style));
    }
    fmt.self_close("img", &attrs);
    fmt.cr();
    // 注释：`caption=` 优先于块里写着的那一行
    caption_line(block, fmt);
    fmt.close("figure");
    fmt.cr();
}

/// `::video src=片子.mp4` —— 摆一个播放器。
///
/// 与 `::image` 同一套参数：`src` 必给，`width=` / `height=` 是**上限**，
/// 注释写在块里一行或由 `caption=` 给出。`mode=left|center|right` 决定位置；
/// 视频和音频没有 `inline` 模式。
pub(super) fn render_video(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    render_media(block, node, fmt, "video");
}

/// `::audio src=录音.mp3` —— 见 [`render_video`]。
pub(super) fn render_audio(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    render_media(block, node, fmt, "audio");
}

/// 视频与音频只差一个标签名：源、尺寸、注释、位置的规矩完全一样
fn render_media(block: &Block, _node: &Node, fmt: &mut dyn Renderer, kind: &str) {
    let Some(source) = checked_source(block, fmt, "至少要给出文件名字") else {
        return;
    };

    let Some(mode) = mode_of(block, fmt, false) else {
        return;
    };

    let style = size_style(block);

    fmt.cr();
    fmt.open(
        "figure",
        &[(
            "class",
            format!("mdpp-media mdpp-media--{kind} mdpp-media--{mode}"),
        )],
    );
    fmt.cr();
    let mut attrs: Vec<(&str, String)> = vec![
        ("src", source),
        // 播放器该有的控件一个不少；预加载只取头，别让长片子一打开就拖满带宽
        ("controls", "controls".to_string()),
        ("preload", "metadata".to_string()),
    ];
    if !style.is_empty() {
        attrs.push(("style", style));
    }
    fmt.self_close(kind, &attrs);
    fmt.cr();
    caption_line(block, fmt);
    fmt.close("figure");
    fmt.cr();
}

/// `width=` / `height=` → 那条**上限**声明（`max-width` / `max-height`）。
///
/// 给的是上限不是定死尺寸：图与播放器各自的原始比例不归这里管。
/// 认不出的值**整条丢掉**（`size_rule` 只认「数字 + 白名单单位」）—— 这个值会写进
/// `style` 属性，宽松了就等于允许往里面塞任意声明
fn size_style(block: &Block) -> String {
    let mut style = String::new();
    for (param, rule) in [("width", "max-width"), ("height", "max-height")] {
        if let Some(ok) = block.param(param).and_then(|value| size_rule(rule, value)) {
            style.push_str(&ok);
        }
    }
    style
}

/// 注释那一行：`caption=` 优先，没有就取块里写着的那一行。两处都不要时**一个字不写**
/// （连 `<figcaption>` 都不摆 —— 空注释格是没必要的输出）
fn caption_line(block: &Block, fmt: &mut dyn Renderer) {
    if let Some(caption) = caption_of(block) {
        fmt.open("figcaption", &[]);
        fmt.text(&caption);
        fmt.close("figcaption");
        fmt.cr();
    }
}

/// 取 `src=` 并验一遍协议。挡下就报用法问题并返回 `None`（调用方直接返回）。
///
/// 「缺 src」与「src 是危险协议」是**两件事**，所以提示词不同 ——
/// 混成一句「取不到地址」的话，作者会先去查文件存不存在，而文件好好的。
fn checked_source(block: &Block, fmt: &mut dyn Renderer, missing: &str) -> Option<String> {
    let Some(source) = block.param("src").map(str::trim).filter(|s| !s.is_empty()) else {
        render_problem(block, fmt, &format!("缺少 src=…：{missing}"));
        return None;
    };
    let source = source.trim();
    // 浏览器在解析地址时会去掉协议段里的空白与控制字符，所以
    // `java\nscript:` 与 ` javascript:` 都会被它当成 `javascript:`。
    // 这里按同样的规矩先把那一段收干净，再判前缀。
    let probe: String = source
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        .collect();
    if ["javascript:", "data:", "vbscript:"]
        .iter()
        .any(|bad| probe.starts_with(bad))
    {
        render_problem(
            block,
            fmt,
            "src 用的是不允许的协议（javascript / data / vbscript 会被拒绝）",
        );
        return None;
    }
    Some(source.to_string())
}

/// `mode=`：位置只认三个值；图片额外允许 `inline`。
/// 没写时默认居中；写错时直接报问题，而不是悄悄摆正。
fn mode_of(block: &Block, fmt: &mut dyn Renderer, allow_inline: bool) -> Option<&'static str> {
    let Some(value) = block.param("mode") else {
        if block.param("align").is_some() {
            render_problem(
                block,
                fmt,
                "align= 已改为 mode=，请写 mode=left / center / right",
            );
            return None;
        }
        return Some("center");
    };
    match value.trim() {
        "left" => Some("left"),
        "center" => Some("center"),
        "right" => Some("right"),
        "inline" if allow_inline => Some("inline"),
        "inline" => {
            render_problem(
                block,
                fmt,
                "视频和音频没有 mode=inline，只能 left / center / right",
            );
            None
        }
        other => {
            let allowed = if allow_inline {
                "left / center / right / inline"
            } else {
                "left / center / right"
            };
            render_problem(
                block,
                fmt,
                &format!("mode= 只认 {allowed}，写成了 `{other}`"),
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// `::video` / `::audio`：摆一个带控件的播放器
    #[test]
    fn video_and_audio_render_players() {
        let video = render("::video src=片子.mp4 width=720px\n  一段注释\n");
        assert!(video.contains("<video"), "{video}");
        assert!(video.contains("controls"), "{video}");
        assert!(video.contains("src=\"片子.mp4\""), "{video}");
        assert!(video.contains("max-width: 720px"), "{video}");
        assert!(
            video.contains("<figcaption>一段注释</figcaption>"),
            "{video}"
        );

        let audio = render("::audio src=录音.mp3\n");
        assert!(audio.contains("<audio"), "{audio}");
        assert!(audio.contains("controls"), "{audio}");

        // 没有 src 就报用法问题，而不是摆一个空的播放器
        let bare = render("::video\n");
        assert!(bare.contains("template--problem"), "{bare}");
    }

    /// 位置、尺寸上限与图注三种参数一起写：尺寸落成上限声明，位置落成修饰类
    #[test]
    fn image_carries_mode_and_limits() {
        let html = render("::image src=/logo.svg mode=right width=320 caption=\"桥体\"\n");
        assert!(
            html.contains(r#"<figure class="mdpp-image mdpp-image--right">"#),
            "{html}"
        );
        assert!(html.contains(r#"src="/logo.svg""#), "{html}");
        assert!(html.contains("max-width: 320px"), "{html}");
        assert!(html.contains("<figcaption>桥体</figcaption>"), "{html}");
    }

    /// 行内模式：不生成 figure，图片高度交给 CSS 的 1em 随行高缩放
    #[test]
    fn image_inline_mode_is_an_inline_image() {
        let html = render("::image src=/star.svg mode=inline alt=星标\n");
        assert!(!html.contains("<figure"), "{html}");
        assert!(!html.contains("<figcaption"), "{html}");
        assert!(
            html.contains(r#"<img class="mdpp-image mdpp-image--inline""#),
            "{html}"
        );
        assert!(html.contains(r#"alt="星标""#), "{html}");
    }

    /// 行内图片不能带图注，也不能用 width/height 把尺寸定死
    #[test]
    fn image_inline_mode_refuses_a_caption() {
        let html = render("::image src=/star.svg mode=inline\n  星标\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }

    /// mode 写错要报问题，而不是悄悄当成居中
    #[test]
    fn image_refuses_an_unknown_mode() {
        let html = render("::image src=/logo.svg mode=wat\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("left / center / right / inline"), "{html}");
    }

    /// 旧参数不能静默当成居中；给出改名提示。
    #[test]
    fn image_rejects_the_old_align_parameter() {
        let html = render("::image src=/logo.svg align=right\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("align= 已改为 mode="), "{html}");
    }

    /// 图注两种写法：块里写一行最自然，`caption=` 优先；都没有时一个 `<figcaption>` 都不摆
    #[test]
    fn image_takes_its_caption_from_the_body() {
        // 图片下面写一行 —— 最自然的写法
        let html = render("::image src=/logo.svg\n  桥体（一〇七九年）\n");
        assert!(
            html.contains("<figcaption>桥体（一〇七九年）</figcaption>"),
            "{html}"
        );

        // 块内容在块里是被缩进的，注释不该带着那些空格
        let indented = render("::image src=/logo.svg\n    两边都有空格\n");
        assert!(
            indented.contains("<figcaption>两边都有空格</figcaption>"),
            "{indented}"
        );

        // 多行拼成一句
        let multiline = render("::image src=/logo.svg\n  第一行\n  第二行\n");
        assert!(
            multiline.contains("<figcaption>第一行 第二行</figcaption>"),
            "{multiline}"
        );

        // `caption=` 优先于块内容
        let explicit = render("::image src=/logo.svg caption=显式\n  块里那句\n");
        assert!(
            explicit.contains("<figcaption>显式</figcaption>"),
            "{explicit}"
        );
        assert!(!explicit.contains("块里那句"), "{explicit}");

        // 都没有：不出现空的图注
        let none = render("::image src=/logo.svg\n");
        assert!(!none.contains("<figcaption>"), "{none}");
    }

    /// 危险协议不渲染成图片；塞在尺寸里的别的声明一律不认，因此拼不出 style 属性
    #[test]
    fn image_refuses_dangerous_sources_and_sneaky_sizes() {
        // 危险协议：不渲染图片，只给提示
        // （提示框里会**回显**参数原文，那是文本、不是属性 —— 所以断言针对「有没有 img」）
        let bad = render("::image src=javascript:alert(1)\n");
        assert!(!bad.contains("<img"), "危险协议不该渲染成图片：{bad}");
        assert!(bad.contains("template--problem"), "{bad}");

        // 尺寸只认「数字 + 可选单位」：塞别的声明一律不认，因此拼不出 style 属性
        let sneaky = render("::image src=/x.svg width=\"1px; background: url(//evil)\"\n");
        assert!(
            !sneaky.contains("style="),
            "尺寸参数不该拼出 style 属性：{sneaky}"
        );

        // 没有 src：说清缺什么
        let missing = render("::image\n");
        assert!(missing.contains("template--problem"), "{missing}");
        assert!(missing.contains("src"), "{missing}");
    }

    /// 空的 `src` 与没写是同一件事：报用法问题，不摆一个空地址的图片
    #[test]
    fn an_empty_src_counts_as_missing() {
        let html = render("::image src=\"\"\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }
}
