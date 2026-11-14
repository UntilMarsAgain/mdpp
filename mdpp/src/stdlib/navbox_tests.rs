//! `navbox` 模板的回归测试。
//!
//! 结构、校验和嵌套都在模板里，所以这里从 Markdown 源一路渲染到 HTML，盯的是
//! 作者写下来的东西最后变成什么，而不是某个 Rust 函数。

/// 把一段 mdpp 源渲染成 HTML；模板解析失败会直接 panic。
fn render(source: &str) -> String {
    crate::Mdpp::new().render(source)
}

/// 断言是一张完整的导航框，而不是问题框。
fn assert_navbox(html: &str) {
    assert!(html.contains("class=\"mdpp-navbox\""), "{html}");
    assert!(!html.contains("mdpp-template--problem"), "{html}");
}

#[test]
fn renders_a_two_column_grouped_table() {
    let html = render("::navbox title=示例\n  [分类]\n    * 甲\n    * 乙\n");
    assert_navbox(&html);
    assert!(html.contains("mdpp-navbox__title"), "{html}");
    assert!(html.contains("mdpp-navbox__bar"), "{html}");
    assert!(html.contains("mdpp-navbox__table"), "{html}");
    assert!(html.contains("mdpp-navbox__group"), "{html}");
    assert!(html.contains("mdpp-navbox__cell"), "{html}");
    assert!(
        html.contains("<th class=\"mdpp-navbox__group\" scope=\"row\">"),
        "{html}"
    );
    assert!(html.contains("<li>甲</li>"), "{html}");
    assert!(html.contains("<li>乙</li>"), "{html}");
}

#[test]
fn repeated_groups_and_headers_keep_source_order() {
    let html = render(
        "::navbox title=示例\n  [header]\n    第一批\n  [甲]\n    一\n  [乙]\n    二\n  [header]\n    第二批\n  [甲]\n    三\n",
    );
    assert_navbox(&html);
    assert_eq!(html.matches("第二批").count(), 1, "{html}");
    assert_eq!(
        html.matches("class=\"mdpp-navbox__head\"").count(),
        2,
        "{html}"
    );
    assert_eq!(
        html.matches("class=\"mdpp-navbox__group\"").count(),
        3,
        "{html}"
    );
    assert!(
        html.find("第一批").unwrap() < html.find("第二批").unwrap(),
        "{html}"
    );
}

#[test]
fn above_and_below_are_pinned_to_the_edges() {
    let html = render(
        "::navbox title=示例\n  [below]\n    底注\n  [甲]\n    一\n  [above]\n    顶注\n  [乙]\n    二\n",
    );
    assert_navbox(&html);
    assert!(html.contains("class=\"mdpp-navbox__above\""), "{html}");
    assert!(html.contains("class=\"mdpp-navbox__below\""), "{html}");
    assert!(
        html.find("顶注").unwrap() < html.find("甲").unwrap()
            && html.find("乙").unwrap() < html.find("底注").unwrap(),
        "{html}"
    );
}

#[test]
fn invalid_headers_are_reported() {
    let numbered = render("::navbox title=示例\n  [header1]\n    甲\n  [分类]\n    乙\n");
    assert!(numbered.contains("mdpp-template--problem"), "{numbered}");
    assert!(numbered.contains("header1"), "{numbered}");

    let empty = render("::navbox title=示例\n  [header]\n  [分类]\n    乙\n");
    assert!(empty.contains("mdpp-template--problem"), "{empty}");
    assert!(empty.contains("是空的"), "{empty}");
}

#[test]
fn nested_navboxes_have_no_bar_and_cannot_take_state() {
    let html = render("::navbox title=父\n  [分组]\n    ::navbox\n      [子组]\n        内容\n");
    assert_navbox(&html);
    assert_eq!(
        html.matches("class=\"mdpp-navbox__bar\"").count(),
        1,
        "{html}"
    );
    assert_eq!(html.matches("class=\"mdpp-navbox\"").count(), 2, "{html}");
    assert!(html.contains("class=\"mdpp-navbox__cell\""), "{html}");
    assert!(!html.contains("mdpp-navbox__cell--branch"), "{html}");

    let bad = render("::navbox state=collapsed\n  [分组]\n    内容\n");
    assert!(bad.contains("mdpp-template--problem"), "{bad}");
    assert!(!bad.contains("mdpp-navbox__table"), "{bad}");
}

#[test]
fn state_controls_the_toggle_and_body() {
    let collapsed = render("::navbox title=示例 state=collapsed\n  [分类]\n    内容\n");
    assert_navbox(&collapsed);
    assert!(
        collapsed.contains("data-mdpp-state=\"collapsed\""),
        "{collapsed}"
    );
    assert!(collapsed.contains("aria-expanded=\"false\""), "{collapsed}");
    assert!(collapsed.contains("显示"), "{collapsed}");

    let plain = render("::navbox title=示例 state=plain\n  [分类]\n    内容\n");
    assert_navbox(&plain);
    assert!(plain.contains("data-mdpp-state=\"plain\""), "{plain}");
    assert!(!plain.contains("mdpp-navbox__toggle"), "{plain}");

    let bad = render("::navbox title=示例 state=wat\n  [分类]\n    内容\n");
    assert!(bad.contains("mdpp-template--problem"), "{bad}");
    assert!(!bad.contains("mdpp-navbox__table"), "{bad}");
}

#[test]
fn title_and_palette_are_rendered_safely() {
    let html = render(
        "::navbox title=\"**粗**标题\" group-bg=#ddeeff header-bg=#ffeedd\n  [分类]\n    内容\n",
    );
    assert_navbox(&html);
    assert!(html.contains("<strong>粗</strong>标题"), "{html}");
    assert!(html.contains("--navbox-group-bg: #ddeeff"), "{html}");
    assert!(html.contains("--navbox-group-fg: #101010"), "{html}");
    assert!(html.contains("--navbox-head-bg: #ffeedd"), "{html}");
    assert!(html.contains("--navbox-head-fg:"), "{html}");

    let bad = render("::navbox title=示例 group-bg=red\n  [分类]\n    内容\n");
    assert!(bad.contains("mdpp-template--problem"), "{bad}");
    assert!(bad.contains("只认"), "{bad}");
}

#[test]
fn link_labels_use_the_wiki_link_resolver() {
    let html = render("::navbox title=示例\n  [link 材料|材料类]\n    内容\n  [link]\n    内容\n");
    assert!(html.contains("mdpp-template--problem"), "{html}");
    assert!(html.contains("[link] 后面没写页面名"), "{html}");

    let good = render("::navbox title=示例\n  [link 材料|材料类]\n    内容\n");
    assert_navbox(&good);
    assert!(good.contains("mdpp-wikilink"), "{good}");
    assert!(good.contains("材料类"), "{good}");
    assert!(!good.contains("[link 材料"), "{good}");
}

#[test]
fn invalid_or_empty_navboxes_leave_only_one_problem() {
    for source in [
        "::navbox title=示例\n",
        "::navbox title=示例\n  [above]\n    一\n  [above]\n    二\n  [分类]\n    三\n",
    ] {
        let html = render(source);
        assert!(html.contains("mdpp-template--problem"), "{html}");
        assert!(!html.contains("mdpp-navbox__table"), "{html}");
        assert_eq!(html.matches("mdpp-template--problem").count(), 1, "{html}");
    }
}

#[test]
fn guard_discards_a_broken_navbox_as_a_whole() {
    let html = render("::navbox title=示例\n  [header]\n    第一批\n  [link]\n    没有页面名\n");
    assert!(html.contains("mdpp-template--problem"), "{html}");
    assert!(!html.contains("mdpp-navbox__table"), "{html}");
    assert!(!html.contains("第一批"), "{html}");
}
