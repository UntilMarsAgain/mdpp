use crate::engine::Mdpp;

fn render(src: &str) -> String {
    Mdpp::new().render(src)
}

/// 一张两列的表：左边分组名、右边内容 —— 与 `::fields` 同一个骨架
#[test]
fn it_is_a_two_column_table_like_fields() {
    let html = render("::navbox title=桥梁\n  [分类]\n    * [[拱桥]]\n");
    assert!(
        html.contains(r#"<table class="mdpp-navbox__table">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<th class="mdpp-navbox__group" scope="row">分类</th>"#),
        "{html}"
    );
    assert!(html.contains(r#"<td class="mdpp-navbox__cell">"#), "{html}");
    assert!(html.contains("<li>"), "{html}");
}

/// **`[标签]` 就是分组名** —— 不另配参数
#[test]
fn the_label_is_the_group_title() {
    let html = render("::navbox title=桥梁\n  [收录标准]\n    甲\n");
    assert!(html.contains("收录标准"), "{html}");
    // 组名**不**从参数里来
    assert!(
        html.contains(r#"<th class="mdpp-navbox__group" scope="row">"#),
        "{html}"
    );
    assert!(!html.contains("template__param"), "{html}");
}

/// 分组名可以**乱写**：不像编号也不像保留字，就是显示用的那几个字
#[test]
fn a_group_name_is_whatever_the_author_wrote() {
    let html = render("::navbox title=x\n  [2024 年的条目]\n    甲\n  [B 组 · 补充]\n    乙\n");
    assert!(!html.contains("template--problem"), "{html}");
    assert!(html.contains("2024 年的条目"), "{html}");
    assert!(html.contains("B 组 · 补充"), "{html}");
}

/// 分组名**可以重复** —— 它是显示用的字，不是标识。维基那些「方针类」导航框
/// 就是同一个标签在每个 `[header]` 下面各写一次。
#[test]
fn a_group_name_may_repeat_under_each_header() {
    let html = render(
        "::navbox title=x\n  [header]\n    基本原则\n  [基本原则]\n    甲\n  [header]\n    内容方针\n  [基本原则]\n    乙\n",
    );
    assert!(!html.contains("template--problem"), "{html}");
    assert_eq!(html.matches("navbox__head").count(), 2, "{html}");
    // 数的是**分组行**上的那个标签（通栏里也写着「基本原则」，不能混进来）
    assert_eq!(
        html.matches(r#"<th class="mdpp-navbox__group" scope="row">基本原则</th>"#)
            .count(),
        2,
        "该出现两次：{html}"
    );
    assert!(html.contains("甲") && html.contains("乙"), "{html}");
}

/// `[header]` 是一条横跨两列的通栏，落在作者写它的那两行之间
#[test]
fn a_header_is_a_full_width_row_between_them() {
    let html = render(
        "::navbox title=求问百科\n  [header]\n    基本原则\n  [甲组]\n    甲\n  [乙组]\n    乙\n",
    );
    assert!(html.contains(r#"colspan="2""#), "{html}");
    assert!(html.contains("基本原则"), "{html}");
    // 通栏横跨两列，里面**没有**分组名也没有列表 —— 按 `</tr>` 切出那一行来断，
    // 固定长度的窗口会溢出到下一行去（那一行有分组名，于是误判）
    let row = |class: &str| -> String {
        let from = html.find(class).unwrap_or_else(|| panic!("{class} 该在"));
        let rest = &html[from..];
        let to = rest.find("</tr>").expect("行要有个头");
        format!("{}{}", class, &rest[..to])
    };
    let head = row("navbox__head");
    assert!(!head.contains("navbox__group"), "通栏不该带分组名：{head}");
    assert!(!head.contains("<li>"), "通栏不该带列表：{head}");
    assert!(head.contains("基本原则"), "{head}");

    // 次序：通栏真的在第一行之前
    assert!(
        html.find("基本原则").unwrap() < html.find("甲组").unwrap(),
        "{html}"
    );
}

/// `[header]` 不带编号；带编号的是老写法，要指出该写什么
#[test]
fn a_header_takes_no_number() {
    let ok = render("::navbox title=x\n  [header]\n    甲\n  [分组]\n    乙\n");
    assert!(!ok.contains("template--problem"), "{ok}");

    // 带数字的（从维基照搬过来的）不认，且提示要说清改成什么
    for bad in ["header1", "header0", "header2"] {
        let html = render(&format!("::navbox title=x\n  [{bad}]\n    甲\n"));
        assert!(html.contains("template--problem"), "{bad} -> {html}");
        assert!(html.contains("不用编号"), "{bad} -> {html}");
        assert!(html.contains("[header]"), "{bad} -> {html}");
    }
}

/// **`[header]` 可以写很多条** —— 它没有编号，也就不存在「重号」这回事。
///
/// 这是「不带编号」换来的：编号一旦没有，去重就没有了依据，而通栏本来就是
/// 「写在哪儿就落在哪儿」，重复并不构成冲突。
#[test]
fn a_header_may_appear_many_times() {
    let html = render(
        "::navbox title=x\n  [header]\n    甲\n  [一组]\n    一\n  [header]\n    乙\n  [二组]\n    二\n",
    );
    assert!(!html.contains("template--problem"), "{html}");
    assert_eq!(html.matches("navbox__head").count(), 2, "{html}");
    assert!(html.contains("甲") && html.contains("乙"), "{html}");
    // 次序：甲 → 一组 → 乙 → 二组
    let at = |needle: &str| html.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(
        at("甲") < at("一") && at("一") < at("乙") && at("乙") < at("二组"),
        "{html}"
    );
}

/// 空通栏报错（它就是它的标题，没有标题等于没有）
#[test]
fn an_empty_header_is_refused() {
    let empty = render("::navbox title=x\n  [header]\n  [分组]\n    甲\n");
    assert!(empty.contains("template--problem"), "{empty}");
    assert!(empty.contains("是空的"), "{empty}");
    assert!(!empty.contains("navbox__table"), "不渲染半截：{empty}");
}

/// `[above]` 紧贴标题栏之下，`[below]` 在最底下 —— 位置是**定的**，不由作者写在哪决定
#[test]
fn above_sits_at_the_top_and_below_at_the_bottom() {
    let html = render(
        "::navbox title=桥梁\n  [above]\n    上面那句\n  [分类]\n    甲\n  [below]\n    下面那句\n",
    );
    let top = html.find("上面那句").expect("above");
    let group = html.find(">分类<").expect("分组");
    let bottom = html.find("下面那句").expect("below");
    assert!(top < group && group < bottom, "{html}");
    assert!(html.contains(r#"class="mdpp-navbox__above""#), "{html}");
    assert!(html.contains(r#"class="mdpp-navbox__below""#), "{html}");
    // 两处是内容不是表头
    assert_eq!(html.matches("navbox__above").count(), 1, "{html}");
}

/// `[above]` / `[below]` 各只能有一处
#[test]
fn above_and_below_may_appear_once() {
    for (label, other) in [("above", "below"), ("below", "above")] {
        let html = render(&format!(
            "::navbox title=x\n  [{label}]\n    甲\n  [分组]\n    乙\n  [{label}]\n    丙\n"
        ));
        assert!(html.contains("template--problem"), "{label} -> {html}");
        assert!(html.contains("写了两处"), "{html}");
        let _ = other;
    }
}

/// **没有 `title=` 就是嵌进去的样子**：没有标题栏、没有边框、没有开关
#[test]
fn no_title_means_nested() {
    let html = render("::navbox\n  [桥墩]\n    * 甲\n");
    assert!(html.contains("navbox--nested"), "{html}");
    assert!(!html.contains("navbox__bar"), "没有标题栏：{html}");
    assert!(!html.contains("navbox__toggle"), "{html}");
    assert!(
        html.contains(r#"<th class="mdpp-navbox__group" scope="row">桥墩</th>"#),
        "{html}"
    );
}

/// 嵌进去的没有开关，写 `state=` 是白写 —— 说出来
#[test]
fn state_on_a_nested_navbox_is_pointless() {
    let html = render("::navbox state=collapsed\n  [分组]\n    甲\n");
    assert!(html.contains("template--problem"), "{html}");
    assert!(html.contains("state="), "{html}");
}

/// 每个分组里可以嵌一个 `::navbox` 当子 group
#[test]
fn a_group_can_hold_a_nested_navbox() {
    let html = render(
        "::navbox title=桥的构件\n  [上部结构]\n    * 桥面\n\n    ::navbox\n      [桥墩]\n        * 石墩\n        * 木墩\n",
    );
    assert!(!html.contains("template--problem"), "{html}");
    assert_eq!(html.matches("navbox--nested").count(), 1, "{html}");
    // 子导航框落在**父导航框的内容格子里**
    assert!(html.contains("桥面"), "{html}");
    assert!(html.contains("石墩") && html.contains("木墩"), "{html}");
    assert!(html.contains("桥墩"), "{html}");
    // 父导航框只有自己的标题栏
    assert_eq!(html.matches("navbox__bar").count(), 1, "{html}");
}

/// 装着子导航框的格子带 `navbox__cell--branch` 记号 —— CSS 靠它把
/// 「父格文字的大间距」与「子导航框贴左的小间距」分开
#[test]
fn a_cell_holding_a_nested_navbox_is_marked() {
    let html = render(
        "::navbox title=x\n  [分组]\n    * 甲\n\n    ::navbox\n      [子组]\n        * 乙\n",
    );
    assert!(
        html.contains(r#"<td class="mdpp-navbox__cell mdpp-navbox__cell--branch">"#),
        "{html}"
    );
    // 只有装着子框的那一格带记号
    assert_eq!(html.matches("navbox__cell--branch").count(), 1, "{html}");
}

/// 三层嵌套时**每层**各带各的记号 —— 第三层格子里没子框，不带
#[test]
fn nested_cells_are_marked_at_every_level() {
    let html = render(
        "::navbox title=x\n  [甲]\n    一句\n\n    ::navbox\n      [乙]\n        一句\n\n        ::navbox\n          [丙]\n            一句\n",
    );
    assert_eq!(html.matches("navbox__cell--branch").count(), 2, "{html}");
}

/// 写在列表项里的不算（那是懒延续，子导航框整个被吞成文字）
#[test]
fn a_navbox_swallowed_by_a_list_is_not_marked() {
    let html =
        render("::navbox title=x\n  [分组]\n    * 甲\n    ::navbox\n      [子]\n        乙\n");
    assert!(!html.contains("navbox__cell--branch"), "{html}");
}

/// 包在一层块里的子导航框也算嵌套 —— 分支记号看的是整棵子树，不只是直接子节点。
#[test]
fn a_nested_navbox_inside_a_wrapper_is_still_marked() {
    let html = render(
        "::navbox title=x\n  [分组]\n    ::element tag=div\n      ::navbox\n        [子组]\n          * 乙\n",
    );
    assert!(html.contains("navbox__cell--branch"), "{html}");
}

/// 嵌套**必须空一行**，否则会被 markdown 当成列表项的续行。
///
/// 不空行时那一行连同后面几行一起变成 `<li>` 里的普通文字 —— 子导航框整个消失，
/// 页面上只看得见一句 `::navbox`。
#[test]
fn nesting_without_a_blank_line_swallows_the_block() {
    let swallowed =
        render("::navbox title=外层\n  [组]\n    * 甲\n    ::navbox\n      [子组]\n        * 乙\n");
    assert!(
        !swallowed.contains("navbox--nested"),
        "不空行时子导航框整个被列表吞成文字了：{swallowed}"
    );
    assert!(
        swallowed.contains("::navbox"),
        "原文成了列表项里的字：{swallowed}"
    );

    let separate = render(
        "::navbox title=外层\n  [组]\n    * 甲\n\n    ::navbox\n      [子组]\n        * 乙\n",
    );
    assert_eq!(separate.matches("navbox--nested").count(), 1, "{separate}");
}

/// 折叠：`state=collapsed` 落成 `hidden` + `aria-expanded="false"`，**不给 onclick**
#[test]
fn collapse_is_structure_not_behaviour() {
    let collapsed = render("::navbox title=桥梁 state=collapsed\n  [分类]\n    * 甲\n");
    assert!(
        collapsed.contains(r#"data-state="collapsed""#),
        "{collapsed}"
    );
    assert!(
        collapsed.contains(r#"aria-expanded="false""#),
        "{collapsed}"
    );
    assert!(
        collapsed.contains(r#"<div class="mdpp-navbox__body" hidden="hidden">"#),
        "{collapsed}"
    );
    // 按钮上写的是「点了会发生什么」：折着时写「显示」
    assert!(collapsed.contains(">显示</button>"), "{collapsed}");

    let expanded = render("::navbox title=桥梁\n  [分类]\n    * 甲\n");
    assert!(expanded.contains(r#"aria-expanded="true""#), "{expanded}");
    assert!(expanded.contains(">隐藏</button>"), "{expanded}");
    assert!(!expanded.contains("hidden"), "{expanded}");
    assert!(!expanded.contains("onclick"), "点击由使用方接：{expanded}");
}

/// 按钮上带着两个词，脚本换的时候不必把中文硬编码进 JS
#[test]
fn the_button_carries_both_labels() {
    let html = render("::navbox title=桥梁\n  [分类]\n    * 甲\n");
    assert!(html.contains(r#"data-label-show="显示""#), "{html}");
    assert!(html.contains(r#"data-label-hide="隐藏""#), "{html}");
}

/// `state=plain`：展开且没有开关
#[test]
fn plain_has_no_toggle() {
    let html = render("::navbox title=桥梁 state=plain\n  [分类]\n    * 甲\n");
    assert!(html.contains(r#"data-state="plain""#), "{html}");
    assert!(!html.contains("navbox__toggle"), "{html}");
    assert!(html.contains("桥梁"), "标题仍在：{html}");
}

/// `state=` 写错就整块拒掉
#[test]
fn a_bad_state_is_refused() {
    let html = render("::navbox title=x state=autocollapse\n  [分类]\n    * 甲\n");
    assert!(html.contains("template--problem"), "{html}");
    for known in ["expanded", "collapsed", "plain"] {
        assert!(html.contains(known), "提示里该说得出 {known}：{html}");
    }
    assert!(
        !html.contains(r#"class="mdpp-navbox""#),
        "不渲染半截：{html}"
    );
}

/// **`[link 页面]` 让分组名变成一条内部链接。**
///
/// 走的是 markdown 的 `[[…]]`，所以**链接解析器是同一个**：递了解析器就出 `href`，
/// 没递就落成红链 —— 与正文里的 `[[…]]` 一模一样。
#[test]
fn a_group_label_can_be_an_internal_link() {
    use crate::RenderContext;
    use crate::link::{LinkResolver, LinkTarget, ResolvedLink};
    use std::sync::Arc;

    struct Vault;
    impl LinkResolver for Vault {
        fn resolve(&self, target: &str) -> Option<ResolvedLink> {
            Some(ResolvedLink::Link(LinkTarget::new(format!(
                "/notes/{target}"
            ))))
        }
    }
    let html = Mdpp::new().render_with(
        "::navbox title=x\n  [link 桥梁]\n    内容\n",
        &RenderContext::new(),
    );
    // 标签格里是一条 `<a>`，不是文字
    let row = html.find("navbox__group").expect("要有标签");
    let row = &html[row..row + 200];
    assert!(row.contains("<a "), "分组名该是链接：{row}");
    assert!(row.contains(">桥梁</a>"), "{row}");

    // 递了解析器就出地址
    let resolver: Arc<dyn LinkResolver> = Arc::new(Vault);
    let with = Mdpp::new().render_with(
        "::navbox title=x\n  [link 桥梁]\n    内容\n",
        &RenderContext::new().with_links(resolver),
    );
    assert!(with.contains(r#"href="/notes/桥梁""#), "{with}");

    // `|` 那一半换显示文字
    let alias = Mdpp::new().render_with(
        "::navbox title=x\n  [link 桥梁|拱桥一览]\n    内容\n",
        &RenderContext::new(),
    );
    assert!(alias.contains(">拱桥一览</a>"), "{alias}");
    assert!(alias.contains(r#"data-doc="桥梁""#), "{alias}");
}

/// `[linkage]` 之类是**普通分组名** —— 不能因为开头那几个字母就当成链接写法
#[test]
fn only_the_bare_word_link_starts_a_link() {
    for label in ["linkage", "linked", "links"] {
        let html = render(&format!("::navbox title=x\n  [{label}]\n    甲\n"));
        assert!(!html.contains("template--problem"), "{label}: {html}");
        assert!(html.contains(label), "{label}");
        assert!(!html.contains("<a "), "{label} 不该是链接：{html}");
    }
}

/// `[link]` 没写页面名：说清要什么，而不是画一个空格子
///
/// **整块报，不渲染半截** —— 标签是在认标签那一轮认的，那时一行开标签都还没写
#[test]
fn a_link_label_needs_a_page() {
    for label in ["link", "link "] {
        let html = render(&format!("::navbox title=x\n  [{label}]\n    甲\n"));
        assert!(html.contains("template--problem"), "{label:?} -> {html}");
        assert!(html.contains("没写页面名"), "{html}");
        assert!(
            !html.contains(r#"class="mdpp-navbox""#),
            "不渲染半截：{html}"
        );
    }
}

/// **`[分组名 [[链接]]]` 那种写法不做**，而且是有理由的不做。
///
/// 分节只认「整行一对方括号」，而它为了不把 `[[甲]]` 认成标签，把**任何含方括号**
/// 的标签都退回去。放开就得去数嵌套方括号 —— 一个写错半边的 `[a [b]` 会让整个
/// 分节错位。分开写没这个风险。
#[test]
fn brackets_inside_a_label_are_not_a_link() {
    let html = render("::navbox title=x\n  [分组 [[甲]]]\n  内容\n");
    // 不是链接，而是「标签写歪了」的那一类
    assert!(!html.contains(r#"<a href"#), "不该认成链接：{html}");
    assert!(
        html.contains("template--problem") || html.contains("分组"),
        "{html}"
    );
}

/// 三个**底色**参数落成自定义属性，交给样式表去用；**字色一并给出**
#[test]
fn the_three_backgrounds_become_custom_properties() {
    let html = render(
        "::navbox title=x group-bg=#0055a4 title-bg=#123456 header-bg=#0af\n  [分类]\n    甲\n",
    );
    assert!(!html.contains("template--problem"), "{html}");
    for (name, value) in [
        ("--navbox-group-bg", "#0055a4"),
        ("--navbox-title-bg", "#123456"),
        ("--navbox-head-bg", "#00aaff"),
    ] {
        assert!(
            html.contains(&format!("{name}: {value};")),
            "少了 {name}：{html}"
        );
    }
    // `#rgb` 那种简写要规范成六位，与 `::banner` 同一套
    let short = render("::navbox title=x group-bg=#0af\n  [分类]\n    甲\n");
    assert!(short.contains("--navbox-group-bg: #00aaff;"), "{short}");
}

/// **深底自动配浅字、浅底自动配深字** —— 与 `::banner`、提示框同一套。
///
/// 作者只管给底色；深底配深字是最常见的「自己给自己挖坑」，而底色定了之后
/// 字色就该由这里算，而不是让作者自己配一遍。
#[test]
fn the_text_colour_follows_the_background_brightness() {
    let dark = render("::navbox title=x group-bg=#0055a4\n  [分类]\n    甲\n");
    assert!(
        dark.contains("--navbox-group-bg: #0055a4; --navbox-group-fg: #f5f5f5;"),
        "深底该配浅字：{dark}"
    );

    let light = render("::navbox title=x title-bg=#ffe680\n  [分类]\n    甲\n");
    assert!(
        light.contains("--navbox-title-bg: #ffe680; --navbox-title-fg: #101010;"),
        "浅底该配深字：{light}"
    );
}

/// 一个底色都没给时**不写** `style` —— 多余的 `style=""` 是噪音
#[test]
fn no_background_means_no_style_attribute() {
    let html = render("::navbox title=x\n  [分类]\n    甲\n");
    assert!(!html.contains("style="), "{html}");
    // 给了别的属性也不该冒出 style
    let nested = render("::navbox\n  [分类]\n    甲\n");
    assert!(!nested.contains("style="), "{nested}");
}

/// 底色值只认十六进制 —— 它要写进 `style` 属性，宽松了就等于允许塞任意声明
#[test]
fn a_bad_background_is_refused() {
    for (param, value) in [
        ("group-bg", "red"),
        ("title-bg", "#12345"),
        ("header-bg", "#fff; position: fixed"),
    ] {
        let html = render(&format!(
            "::navbox title=x {param}={value}\n  [分类]\n    甲\n"
        ));
        assert!(
            html.contains("template--problem"),
            "{param}={value}: {html}"
        );
        assert!(html.contains("#rgb"), "{html}");
        // 整块拒掉，不渲染半截
        assert!(!html.contains(r#"class="mdpp-navbox""#), "{html}");
    }
}

/// 一个分组都没有
#[test]
fn an_empty_navbox_says_so() {
    let html = render("::navbox title=x\n");
    assert!(html.contains("template--problem"), "{html}");
    assert!(html.contains("分节是空的"), "{html}");

    // 只有通栏也不算数 —— 它自己撑不起一个框
    let headers = render("::navbox title=x\n  [header]\n    只有通栏\n");
    assert!(headers.contains("template--problem"), "{headers}");
}

/// 第一个标签之前有内容
#[test]
fn content_before_the_first_slot_is_refused() {
    let html = render("::navbox title=x\n  开场白\n  [分类]\n    甲\n");
    assert!(html.contains("template--problem"), "{html}");
}

/// 标题按 markdown 渲染，但外层那个 `<p>` 剥掉
#[test]
fn the_title_is_markdown_without_the_wrapping_paragraph() {
    let html = render("::navbox title=**桥梁**\n  [分类]\n    甲\n");
    assert!(html.contains("<strong>桥梁</strong>"), "{html}");
    assert!(!html.contains("<p><strong>桥梁</strong></p>"), "{html}");
}

/// **列表横排是纯 CSS 的事**：渲染器吐的就是普通的 `<ul><li>`。
///
/// 这条钉的是那条分工 —— 一旦渲染器开始为布局吐标记，结构与样式就绑死了，
/// 而换一份样式表的人会立刻看到列表塌回竖排，输出里却没有任何东西能解释为什么。
#[test]
fn horizontality_is_pure_css() {
    let html = render("::navbox title=x\n  [分类]\n    * 甲\n    * 乙\n");
    assert!(html.contains("<ul>"), "{html}");
    assert_eq!(html.matches("<li>").count(), 2, "{html}");
    for marker in ["hlist", "horizontal", "inline"] {
        assert!(!html.contains(marker), "渲染器不该吐 {marker}：{html}");
    }
    // 分隔点只该出现在样式表里
    assert!(!html.contains('\u{b7}'), "分隔点是 CSS 的 ::after：{html}");
}

/// 列表里的内部链接交给链接解析器 —— 导航框整块都是链接
#[test]
fn links_inside_a_navbox_go_through_the_resolver() {
    use crate::RenderContext;
    use crate::link::{LinkResolver, LinkTarget, ResolvedLink};
    use std::sync::Arc;

    struct Vault;
    impl LinkResolver for Vault {
        fn resolve(&self, target: &str) -> Option<ResolvedLink> {
            Some(ResolvedLink::Link(LinkTarget::new(format!(
                "/notes/{target}"
            ))))
        }
    }
    let resolver: Arc<dyn LinkResolver> = Arc::new(Vault);
    let context = RenderContext::new().with_links(resolver);
    let html = Mdpp::new().render_with("::navbox title=x\n  [分类]\n    * [[拱桥]]\n", &context);
    assert!(html.contains(r#"href="/notes/拱桥""#), "{html}");
}
