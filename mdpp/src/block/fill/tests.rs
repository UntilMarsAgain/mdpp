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

/// 正路一条：`{{param}}` 换成值、`{{body}}` 换成块内容 —— 这一步错了，模板整页都是没填的空
#[test]
fn substitutes_params_and_body() {
    let item = block("x", &[("color", "红")], "块内容");
    assert_eq!(
        substitute("a {{color}} b {{body}} c", &item, "块内容"),
        "a 红 b 块内容 c"
    );
}

/// 多行值要跟着占位符所在行的缩进走 —— 只缩第一行的话，后面的行落回行首，
/// 靠缩进划范围的块当场被截断（语法页的分栏模板就是这么坏的）
#[test]
fn a_multiline_value_inherits_the_placeholders_indent() {
    let item = block("x", &[], "甲\n乙");
    assert_eq!(
        substitute("::code\n  {{body}}\n", &item, "甲\n乙"),
        "::code\n  甲\n  乙\n"
    );
}

/// 占位符在列 0 或值只有一行：一个字都不动 —— 页外壳填的就是这一类
#[test]
fn a_column_zero_or_single_line_value_is_left_alone() {
    let item = block("x", &[], "甲\n乙");
    assert_eq!(substitute("{{body}}", &item, "甲\n乙"), "甲\n乙");
    assert_eq!(substitute("  {{body}}", &item, "甲"), "  甲");
}

/// 空行不补缩进：补了就是「只含空白的行」，该断开的地方反而断不开
#[test]
fn blank_lines_in_a_multiline_value_are_not_padded() {
    let item = block("x", &[], "甲\n\n乙");
    assert_eq!(substitute("  {{body}}", &item, "甲\n\n乙"), "  甲\n\n  乙");
}

/// 占位符在行中间时，后续行仍跟这一行的行首缩进对齐
#[test]
fn a_midline_placeholder_uses_the_lines_indent() {
    let item = block("x", &[], "甲\n乙");
    assert_eq!(
        substitute("  前缀 {{body}}", &item, "甲\n乙"),
        "  前缀 甲\n  乙"
    );
}

/// 认不出的名字原样留着：让作者看见自己写错了，比悄悄换成空串好
#[test]
fn unknown_or_unclosed_placeholders_stay_visible() {
    let item = block("x", &[], "");
    assert_eq!(substitute("{{unknown}}", &item, ""), "{{unknown}}");
    assert_eq!(substitute("{{unclosed", &item, ""), "{{unclosed");
}

/// 裸 flag 解析成 `on`；显式 `flag=` 是空值，不打开开关
#[test]
fn bare_flags_are_on_and_explicit_empty_is_not() {
    let bare = block("x", &[("js", "on")], "");
    assert!(flag(&bare, "js"));

    let empty = block("x", &[("js", "")], "");
    assert!(!flag(&empty, "js"));

    assert!(!flag(&block("x", &[], ""), "js"));
}

/// 严格填空：必填缺失报错，`?` 开头缺失填空串，正文里的 `@` 引用也报错
#[test]
fn checked_fill_separates_required_optional_and_late_refs() {
    let item = block("x", &[("color", "红")], "块内容");
    assert_eq!(
        substitute_checked("{{color}} / {{body}}", &item, "块内容"),
        Ok("红 / 块内容".to_string())
    );

    let missing = substitute_checked("前 {{missing}} 后", &item, "").unwrap_err();
    assert!(missing.contains("missing"), "{missing}");

    assert_eq!(
        substitute_checked("前 {{?missing}} 后", &item, ""),
        Ok("前  后".to_string())
    );
    assert_eq!(
        substitute_checked("{{?body}}", &item, "块内容"),
        Ok("块内容".to_string())
    );

    // 晚绑定引用留给块头阶段；正文里写它会原样显示
    assert_eq!(
        substitute_checked("{{@body}}", &item, "块内容"),
        Ok("{{@body}}".to_string())
    );

    let unclosed = substitute_checked("{{missing", &item, "").unwrap_err();
    assert!(unclosed.contains("没有闭合"), "{unclosed}");
}

/// 前面有晚绑定引用时，多行值仍要按占位符在**原文**里的行首缩进对齐
#[test]
fn a_late_ref_before_a_multiline_value_does_not_steal_the_indent() {
    let item = block("x", &[], "甲\n乙");
    let filled = substitute_checked(
        "::element class=\"x {{?@class}}\"\n  {{body}}\n",
        &item,
        "甲\n乙",
    )
    .expect("都可选/存在");
    assert!(filled.contains("\n  甲\n  乙"), "{filled}");
}

/// 没写 `src=` 时，正文就是块自己带的那段 —— 枢纽的这条默认岔路先得走对
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

/// 默认档就要挡住脚本，同时正文一个字不丢 —— 两头都得盯住
#[test]
fn scripts_are_filtered_by_default() {
    let html = sanitize_html("<p>正文</p><script>alert(1)</script>");
    assert!(!html.contains("<script"), "{html}");
    assert!(html.contains("正文"), "{html}");
}

/// `on…=` 与 `javascript:` 是最常见的两条执行路，都得堵上 —— 无辜的属性不能跟着陪葬
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

/// 收窄的几条岔路一起钉：并排的、括号里带逗号的、写给根的、已经收过的 —— 错一条，样式要么漏出正文要么被收两遍
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

/// `@media` 里的选择器照样要收，`@keyframes` 里的一律别动 —— 反了一处，样式就漏出正文或动画被改坏
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

/// 未闭合的块不能原样漏出去：浏览器会把文件结尾当成 `}`，于是整页都被改掉
#[test]
fn an_unclosed_block_is_dropped_instead_of_escaping_the_scope() {
    let css = scope_css("body { background: red");
    assert!(!css.contains("body"), "{css}");
    assert!(!css.contains("background"), "{css}");
    assert!(css.contains("未闭合"), "{css}");
}

/// `@import` 必须先丢 —— 留着它，正文里一行 CSS 就能把页面支到外部去拉样式
#[test]
fn imports_are_dropped() {
    // 让页面去外部拉样式：既慢又不受控
    assert_eq!(
        scope_css("@import url(x.css); .a { }"),
        ".mdpp-content .a { }"
    );
}

/// 正文里混进来的 `</style>` 会提前收掉样式块，把后面的字顶到页面上 —— 得先摘掉
#[test]
fn style_cannot_be_closed_early() {
    let css = sanitize_css("a { } </style><b>x</b>");
    assert!(!css.contains("</style"), "{css}");
    assert!(css.contains("<b>x</b>"), "只去掉闭合标签，其余照旧：{css}");
}

/// 大小写怎么变都要认出来，且只插反斜杠、一个字母不改 —— 改字母就改了字符串或正则里的意思
#[test]
fn script_end_cannot_close_its_own_tag() {
    assert_eq!(escape_script_end(r#"a</script>b"#), r#"a<\/script>b"#);
    // 大小写照原样留着：那可能是字符串里的内容
    assert_eq!(escape_script_end("</SCRIPT>"), r#"<\/SCRIPT>"#);
    assert_eq!(escape_script_end("没有"), "没有");
}

/// 这几种常见写法都得算数 —— 少认一种，作者明明写了想打开，结果静悄悄关着；
/// 显式 `js=` 是空值，不再算打开（裸 flag 在解析时已经落成 `on`）
#[test]
fn switches_read_the_usual_spellings() {
    for spelling in ["true", "yes", "on", "ON"] {
        let item = block("x", &[("js", spelling)], "");
        assert!(allows_js(&item), "js={spelling:?} 应当算打开");
    }
    for spelling in ["", "false", "off", "no", "0"] {
        let item = block("x", &[("js", spelling)], "");
        assert!(!allows_js(&item), "js={spelling:?} 不该算打开");
    }
    assert!(!allows_js(&block("x", &[], "")));
}

/// `::html` 的 `<markdown>` 槽位不能借 `raw_html` 绕过净化 ——
/// `-u` 是给正文作者开的，不该把「安全块」也一起打开。
#[test]
fn markdown_inside_html_stays_sanitized_even_with_raw_html() {
    let mdpp = crate::engine::Mdpp::builder().raw_html(true).build();
    let html = mdpp.render("::html\n  <markdown>\n  <script>alert(1)</script>\n  </markdown>\n");
    assert!(!html.contains("<script"), "{html}");
}
