use super::*;

/// 直接调净化器：这样测的是净化本身，不掺块边界那一层
///
/// （块边界按**缩进**划，一段跨行且第二行没缩进的 HTML 在块里会被截断 ——
/// 那是块语法的性质，不是净化器的，混在一起测就两处一起说不清了。）
fn clean(source: &str) -> String {
    let mdpp = crate::engine::Mdpp::new();
    sanitize(source, &|body| mdpp.render(body))
}

/// 走一遍完整的那条路：`::html` 块 → 净化。**单行**内容。
fn via_block(source: &str) -> String {
    crate::engine::Mdpp::new().render(&format!("::html\n  {source}\n"))
}

/// 最要紧的一条：白名单是**方向**，没想过的东西默认出不去
#[test]
fn unknown_elements_lose_their_tags_but_keep_their_words() {
    let html = clean("<my-widget>里面的正文</my-widget>");
    assert!(!html.contains("my-widget"), "{html}");
    assert!(html.contains("里面的正文"), "{html}");
}

/// `<script>` 要连里面的字一起没 —— 只摘标签，`alert(1)` 就留在正文里了
#[test]
fn scripts_are_gone_with_their_bodies() {
    let html = clean("<p>前</p><script>alert(1)</script><p>后</p>");
    assert!(!html.contains("script"), "{html}");
    assert!(!html.contains("alert"), "{html}");
    assert!(html.contains("前") && html.contains("后"), "{html}");
}

/// 这几家一律整段没 —— 只摘外壳的话，`<style>` 里的规则、`<svg>` 里藏的脚本、表单全都留着
#[test]
fn styles_and_foreign_content_are_gone_with_their_bodies() {
    for source in [
        "<style>body{display:none}</style>",
        "<svg><script>alert(1)</script></svg>",
        "<math><mtext><script>alert(1)</script></mtext></math>",
        "<object data=\"x\"></object>",
        "<form action=\"/x\"><input name=\"a\"></form>",
    ] {
        let html = clean(source);
        assert!(!html.contains("alert"), "{source}: {html}");
        assert!(!html.contains("<svg"), "{source}: {html}");
        assert!(!html.contains("display:none"), "{source}: {html}");
    }
}

/// `style=` 整条不放过 —— 一条 `url()` 就能把外部地址悄悄带进来
#[test]
fn inline_style_attributes_never_survive() {
    let html = clean("<p style=\"background:url(//tracker)\">字</p>");
    assert!(!html.contains("style"), "{html}");
    assert!(!html.contains("tracker"), "{html}");
    assert!(html.contains("字"), "{html}");
}

/// 大小写、断行、前导空格、`vbscript:`、`data:` 都改不了结论 —— 只认一种字面写法，这道堵等于没堵
#[test]
fn javascript_urls_are_refused_however_they_are_disguised() {
    for source in [
        r#"<a href="javascript:alert(1)">点</a>"#,
        r#"<a href="JaVaScRiPt:alert(1)">点</a>"#,
        "<a href=\"java\nscript:alert(1)\">点</a>",
        r#"<a href=" javascript:alert(1)">点</a>"#,
        r#"<a href="vbscript:msgbox(1)">点</a>"#,
        r#"<a href="data:text/html,<script>alert(1)</script>">点</a>"#,
    ] {
        let html = clean(source);
        assert!(!html.contains("javascript"), "{source}: {html}");
        assert!(!html.contains("vbscript"), "{source}: {html}");
        assert!(!html.contains("data:"), "{source}: {html}");
        assert!(!html.contains("script"), "{source}: {html}");
    }
}

/// 堵得狠了会把正常链接一起挡掉 —— 外链、相对路径、页内锚点都要过得去
#[test]
fn ordinary_urls_and_relative_targets_stay() {
    let html = clean(r#"<a href="https://example.com/x?y=1">外链</a>"#);
    assert!(
        html.contains(r#"href="https://example.com/x?y=1""#),
        "{html}"
    );

    let relative = clean(r#"<a href="另一页">内部</a>"#);
    assert!(relative.contains("另一页"), "{relative}");

    let fragment = clean(r##"<a href="#安装">跳节</a>"##);
    assert!(fragment.contains("#安装"), "{fragment}");
}

/// 属性按白名单给：`on…` 与没想过的一律不带 —— 不在表里也放出去，过滤就名存实亡
#[test]
fn event_attributes_and_unknown_ones_are_dropped() {
    let html = clean(r#"<p onclick="alert(1)" onmouseover="x()" data-x="1" id="锚">字</p>"#);
    assert!(!html.contains("onclick"), "{html}");
    assert!(!html.contains("onmouseover"), "{html}");
    assert!(!html.contains("data-x"), "{html}");
    assert!(!html.contains("id="), "{html}");
    assert!(html.contains("字"), "{html}");
}

/// 畸形标签：净化器与浏览器得看到同一棵树，否则净化等于没用
#[test]
fn malformed_markup_is_read_the_way_a_browser_reads_it() {
    let html = clean("<b>粗<p>段落</b>后面");
    assert!(html.contains("<b>"), "{html}");
    assert!(html.contains("</b>"), "{html}");
    assert!(html.contains("<p>"), "{html}");
}

/// 原样放回去，正文就能借这一截把标签撑开 —— 出去前得重新转义
#[test]
fn attribute_values_are_re_escaped_on_the_way_out() {
    let html = clean("<p title='a\"b<c&d'>字</p>");
    assert!(!html.contains(r#"title="a"b"#), "{html}");
    assert!(html.contains("&quot;"), "{html}");
    assert!(html.contains("&lt;c"), "{html}");
    assert!(html.contains("&amp;d"), "{html}");
}

/// 注释与 doctype 不是正文，整段不留 —— 注释里的字一样会送到读者眼前
#[test]
fn comments_and_doctypes_do_not_survive() {
    let html = clean("<!doctype html><!-- secret --><p>正文</p>");
    assert!(!html.contains("secret"), "{html}");
    assert!(!html.contains("doctype"), "{html}");
    assert!(html.contains("正文"), "{html}");
}

/// `<markdown>` 那一段：确实回到自己的解析器手里了
#[test]
fn a_markdown_element_is_rendered_by_the_markdown_renderer() {
    let html = clean("<div><markdown>**加粗**与 [[某页]]</markdown></div>");
    assert!(html.contains("<strong>加粗</strong>"), "{html}");
    // 内部链接是 wikilink，不是原样留下的方括号
    assert!(html.contains("wikilink"), "{html}");
    assert!(!html.contains("<markdown"), "{html}");
}

/// markdown 那段里的块也要能用 —— 它就是回到同一个解析器
#[test]
fn markdown_inside_markdown_still_gets_blocks() {
    let html = clean("<markdown>\n::note title=\"嵌的\"\n  正文\n</markdown>");
    assert!(html.contains("callout--note"), "{html}");
    assert!(html.contains("嵌的"), "{html}");
}

/// 关键是**原样**：取出来的那一段必须与原文**逐字节相同**。
///
/// 之前这个性质是靠「渲染出来的 HTML 长什么样」来验的，结果两次都验错 ——
/// markdown 与 HTML 对 `&lt;`、注释、`<b>` 这几样的最终处理**恰好一致**，
/// 两条路殊途同归，从输出上根本分不出来。所以直接验中间那一步。
#[test]
fn the_lifted_markdown_is_byte_identical_to_the_source() {
    let body = "写的是 &lt;b&gt; 不是标签\n\n<!-- 注释 -->\n<div 没闭合\n**加粗** & 别的";
    let source = format!("<p>前</p><markdown>{body}</markdown><p>后</p>");

    let (skeleton, slots) = lift_markdown(&source);
    assert_eq!(slots.len(), 1, "{skeleton}");
    assert_eq!(slots[0], body, "取出来的那一段必须与原文逐字节相同");
    // 骨架里不该再留下 `<markdown` 这个标签，只剩占位记号
    assert!(!skeleton.contains("<markdown"), "{skeleton}");
    assert!(skeleton.contains(SLOT_OPEN), "{skeleton}");
    // 标签之外的内容一点没动
    assert!(skeleton.contains("<p>前</p>"), "{skeleton}");
    assert!(skeleton.contains("<p>后</p>"), "{skeleton}");
}

/// 嵌着的 `<markdown>` 要配上：最外层取到的应当是**整段内层**，不是第一段
#[test]
fn lifting_respects_nesting() {
    let source = "<markdown>外<markdown>内</markdown>尾</markdown>";
    let (_, slots) = lift_markdown(source);
    assert_eq!(slots.len(), 1, "外层与内层配对，应当只有一段");
    assert_eq!(slots[0], "外<markdown>内</markdown>尾");
}

/// 生成 `levels` 层 `::html` → `<markdown>` → `::html` → … 的嵌套。
///
/// 注意缩进是**逐层加深的** —— 块的边界由缩进划出，写平了就不成块了。
fn nested_html(levels: usize) -> String {
    let mut out = String::new();
    for depth in 0..levels {
        let pad = "  ".repeat(depth + 1);
        out.push_str(&format!("{pad}::html\n{pad}  <markdown>\n"));
    }
    out.push_str(&format!("{}到底了\n", "  ".repeat(levels + 1)));
    for depth in (0..levels).rev() {
        let pad = "  ".repeat(depth + 1);
        out.push_str(&format!("{pad}  </markdown>\n"));
    }
    out
}

/// 一层是正常的：`<markdown>` 里那段的一切都真的生效
#[test]
fn a_nested_html_block_still_works() {
    let html = clean("<markdown>\n::html\n  <markdown>\n  内层\n  </markdown>\n</markdown>");
    assert!(html.contains("内层"), "{html}");
    assert!(!html.contains("已经停在这里"), "{html}");
}

/// 套得深了要停 —— 而且是**说清楚为什么停**，不是静悄悄截断
///
/// 层数取 12：够越过 [`MAX_NESTING`]（8）触发那道闸，又不至于让这个测试变成
/// **性能测试**。层数再往上的耗时问题不是这里要管的事 —— 那是块规则本身的性质。
#[test]
fn nesting_stops_instead_of_running_forever() {
    let html = crate::engine::Mdpp::new().render(&nested_html(12));
    assert!(html.contains("已经停在这里"), "{html}");
}

/// 没闭合的 `<markdown>` 当普通文字留着，不吞掉后面所有内容
#[test]
fn an_unclosed_markdown_element_does_not_swallow_the_page() {
    let html = clean("<p>前</p><markdown>后面");
    assert!(html.contains("前"), "{html}");
    assert!(html.contains("后面"), "{html}");
}

/// 作者自己写的一对占位记号：认不出是哪一段，就当普通文字留着 —— **而且不能崩**。
///
/// 记号是 3 个字节的私有区字符，切片切进它中间就是当场 panic；`::html` 是 Raw 档，
/// 正文里什么字都进得来，所以这条路必须有人钉着
#[test]
fn a_slot_marker_that_matches_nothing_stays_text_without_panicking() {
    // 一段 `<markdown>` 都没有：槽是空的，任何记号都落进「认不出」那一支
    let html = clean("\u{E000}abc\u{E001}");
    assert!(html.contains("abc"), "{html}");

    // 有槽，但下标越界、下标不是数字 —— 同一支
    let html = clean("<markdown>甲</markdown>\u{E000}9\u{E001}\u{E000}九\u{E001}");
    assert!(html.contains('甲'), "{html}");
}

/// 属性是**逐标签**给的：`img` 拿不到 href，`a` 拿不到 src
#[test]
fn attributes_are_granted_per_tag() {
    let img = clean(r#"<img src="a.png" alt="图" href="/evil">"#);
    assert!(img.contains(r#"src="a.png""#), "{img}");
    assert!(!img.contains("href"), "{img}");

    let link = clean(r#"<a href="/x" src="/evil">字</a>"#);
    assert!(link.contains("href"), "{link}");
    assert!(!link.contains("src"), "{link}");
}

/// 常用的一批要真的能用 —— 白名单不该把正文写作的路堵死
#[test]
fn the_common_shapes_come_through_usable() {
    let html = clean(
        r#"<table><thead><tr><th scope="col">甲</th></tr></thead>
               <tbody><tr><td colspan="2">乙</td></tr></tbody></table>
               <ul><li>一</li></ul><blockquote cite="/src"><p>引</p></blockquote>
               <details open><summary>细</summary><p>内</p></details>"#,
    );
    for needle in [
        "<table>",
        "<thead>",
        r#"<th scope="col">"#,
        r#"<td colspan="2">"#,
        "<ul>",
        "<li>一</li>",
        "<blockquote",
        "<details open>",
        "<summary>细</summary>",
    ] {
        assert!(html.contains(needle), "少了 {needle}: {html}");
    }
}

/// 空元素不许补闭合标签 —— `<br></br>` 是凭空多出来的、原文没有的东西
#[test]
fn void_elements_do_not_get_a_closing_tag() {
    let html = clean("<p>a<br>b<hr>c</p><img src=\"x.png\">");
    assert!(html.contains("<br>"), "{html}");
    assert!(!html.contains("</br>"), "{html}");
    assert!(!html.contains("</img>"), "{html}");
}

/// 那个「渲染一段 markdown」的出口是**参数**：传自己的也行，不必是本库那一个
#[test]
fn the_markdown_renderer_is_a_parameter_not_a_hard_wired_one() {
    let html = sanitize("<div><markdown>**加粗**</markdown></div>", &|body| {
        format!("<em>{body}</em>")
    });
    assert!(html.contains("<em>**加粗**</em>"), "{html}");
}

/// 「只想净化、不在渲染里」的场合：那一段被丢掉，但**不 panic**
#[test]
fn sanitizing_without_a_markdown_renderer_drops_the_slot_quietly() {
    let html = sanitize_without_markdown("<div><markdown>甲</markdown></div>");
    assert!(html.contains("<div>"), "{html}");
    assert!(!html.contains("甲"), "{html}");
}

/// 完整那条路：单行 HTML 走 `::html` 块，默认净化
#[test]
fn the_block_path_filters_and_keeps_the_rest() {
    let html = via_block(r#"<b>粗</b><script>alert(1)</script>"#);
    assert!(html.contains("<b>粗</b>"), "{html}");
    assert!(!html.contains("<script"), "{html}");
}

/// `srcset` 是一组地址：第一个合法不代表整条合法
#[test]
fn every_srcset_candidate_is_checked() {
    let html =
        clean(r#"<picture><source srcset="https://x/1.png 1x, javascript:alert(1) 2x"></picture>"#);
    assert!(!html.contains("javascript"), "{html}");

    let ok = clean(r#"<picture><source srcset="a.png 1x, b.png 2x"></picture>"#);
    assert!(ok.contains(r#"srcset="a.png 1x, b.png 2x""#), "{ok}");
}

/// 新窗口要带 `noopener`：被打开的页面不能通过 `window.opener` 反过来操作这一页
#[test]
fn target_blank_gets_noopener() {
    let html = clean(r#"<a href="https://example.com" target="_blank">外</a>"#);
    assert!(html.contains("noopener"), "{html}");
    assert!(html.contains("noreferrer"), "{html}");

    // 已有的 rel 不丢，只补缺的那两个
    let with_rel = clean(r#"<a href="https://example.com" target="_blank" rel="nofollow">外</a>"#);
    assert!(with_rel.contains("nofollow"), "{with_rel}");
    assert!(with_rel.contains("noopener"), "{with_rel}");
}
