use super::*;

fn page<'a>(body: &'a str, assets: Assets) -> Page<'a> {
    Page {
        title: "某篇笔记",
        lang: Some("zh-Hans"),
        theme: None,
        chrome: Chrome::default(),
        canonical: Canonical::default(),
        favicon: None,
        body,
        extra_css: &[],
        extra_js: &[],
        assets,
        external_script: ExternalScript::Cdn,
    }
}

/// 站点页面：带顶栏那一套
fn site_page<'a>(body: &'a str) -> Page<'a> {
    Page {
        chrome: Chrome {
            name: "某站点".to_string(),
            description: "一句话说明".to_string(),
            ..Chrome::default()
        },
        ..page(body, Assets::Linked(ASSET_DIR.to_string()))
    }
}

/// `--full` 出来的是**整页**：`<!DOCTYPE` 打头，样式、脚本、正文都跟着 ——
/// 缺哪一样页面也出得来，只是光着、点不动、或者空的
#[test]
fn a_full_document_carries_the_stylesheet_the_script_and_the_body() {
    let html = document(&page("<p>正文</p>", Assets::Inline));
    assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
    assert!(html.contains(".mdpp-content"), "样式没进页面");
    assert!(html.contains("tabs__tab--on"), "脚本没进页面");
    assert!(html.contains("<p>正文</p>"), "{html}");
}

/// 「能看」与「能用」是两件事 —— 少了脚本，选项卡点不动，页面仍然「看着对」。
/// 这条钉的是**脚本在**，钉不住它「有效」；后者靠 `::tabs` 的按钮带
/// `data-mdpp-block` 之类的约定 —— 而那也只是形状上的线索。
#[test]
fn the_script_is_there_so_the_page_is_usable() {
    let html = document(&page("<div class=\"tabs\"></div>", Assets::Inline));
    assert!(html.contains("DOMContentLoaded"), "{html}");
    assert!(html.contains("tabs__tab--on"), "{html}");
}

/// 导航框的开关也要接上 —— 与选项卡同一个分工
#[test]
fn the_script_binds_the_navbox_toggle_too() {
    let js = default_js();
    assert!(js.contains(".navbox__toggle"), "{js}");
    assert!(js.contains(".navbox__body"), "{js}");
    // 按钮上那两个字由渲染器落在 data-label-* 上，脚本里不硬编码中文
    assert!(js.contains("data-label-show"), "{js}");
    assert!(js.contains("data-label-hide"), "{js}");
}

/// 公开的变体运行时：页面脚本可以读环境、跑/标记资源、监听切换
#[test]
fn the_script_exposes_the_variant_runtime() {
    let js = default_js();
    for needle in [
        "window.mdppEnv",
        "window.mdppVariants",
        "hasRun",
        "runCount",
        "markRun",
        "mdpp:variantchange",
        "mdpp:variantmount",
        "data-mdpp-asset",
        "data-mdpp-once",
    ] {
        assert!(js.contains(needle), "脚本里少了 {needle}");
    }
}

/// **按钮上的字要跟着状态换。** 少了这一步，按钮会一直停在「显示」上，
/// 而内容已经展开了 —— 页面看着正常，而那个字在说谎。
#[test]
fn the_toggle_label_is_swapped_by_the_state_change() {
    let js = default_js();
    let fold = js
        .split("function fold")
        .nth(1)
        .expect("脚本里该有 fold 那一步");
    let fold = &fold[..fold.find("\n  }").expect("fold 有个头")];
    assert!(
        fold.contains("wasCollapsed ? \"data-label-hide\" : \"data-label-show\""),
        "按钮的字没跟着状态换：\n{fold}"
    );
    assert!(fold.contains("textContent = next"), "\n{fold}");
}

/// `<html …>` 那一整行。主题属性只可能落在这一行上 —— 正文里别的 `data-mdpp-theme`
/// 都是内联进来的样式表自己的选择器（都在 `mdpp.css` 里），与属性落没落无关
fn html_tag(html: &str) -> &str {
    html.lines()
        .find(|line| line.starts_with("<html"))
        .expect("<html> 这一行")
}

/// 主题要落在 `<html>` 上 —— 配色那两节靠 `:root[data-mdpp-theme]` 挑，
/// 落到别处（或干脆不落），主题就成了一个没人听的值
#[test]
fn a_theme_lands_on_the_html_element() {
    let html = document(&Page {
        theme: Some("dark"),
        ..page("x", Assets::Inline)
    });
    assert_eq!(
        html_tag(&html),
        r#"<html lang="zh-Hans" data-mdpp-theme="dark">"#
    );
}

/// 不提供语言就不写 `<html lang>` —— 与配置里「不填写就不提供」一致
#[test]
fn a_missing_lang_leaves_the_html_element_without_lang() {
    let html = document(&Page {
        lang: None,
        ..page("x", Assets::Inline)
    });
    assert_eq!(html_tag(&html), "<html>");

    // 空串也当没写
    let blank = document(&Page {
        lang: Some("   "),
        ..page("x", Assets::Inline)
    });
    assert_eq!(html_tag(&blank), "<html>");
}

/// 标题常常是文件名，里面带 `&` 与引号很常见 —— 不转义就进不了属性
#[test]
fn the_title_is_escaped_into_the_attribute() {
    let html = document(&Page {
        title: r#"a"b&c<d"#,
        ..page("x", Assets::Inline)
    });
    assert!(
        html.contains("<title>a&quot;b&amp;c&lt;d</title>"),
        "{html}"
    );
    assert!(!html.contains("<d"), "{html}");
}

/// 不在白名单里的字符串**进不了属性** —— 引号挂 `onload` 那种写法现在连机会都没有：
/// 要么是 `light` / `dark`，要么整条属性不出现
#[test]
fn a_value_outside_the_theme_whitelist_never_reaches_the_html_element() {
    let html = document(&Page {
        theme: Some("dark\" onload=\"alert(1)"),
        ..page("x", Assets::Inline)
    });
    assert!(!html_tag(&html).contains("data-mdpp-theme"), "{html}");
    assert!(!html.contains("onload"), "{html}");
}

/// `theme = "system"` 与**不写**一样：`<html>` 上不落属性，深浅色因此由
/// `prefers-color-scheme` 当场定（`mdpp.css` 里那条 `@media`）；
/// 而钉死的那两个照旧落属性
#[test]
fn system_theme_leaves_the_html_element_without_an_attribute() {
    for theme in [None, Some("system")] {
        let html = document(&Page {
            theme,
            ..site_page("x")
        });
        assert!(!html_tag(&html).contains("data-mdpp-theme"), "{theme:?}");
    }

    let pinned = document(&Page {
        theme: Some("light"),
        ..site_page("x")
    });
    assert_eq!(
        html_tag(&pinned),
        r#"<html lang="zh-Hans" data-mdpp-theme="light">"#,
        "钉死的那个该照旧"
    );
}

/// 页面外壳里要有无 JS 降级：tabs 与折叠的 navbox 由 `<noscript>` 展开
#[test]
fn the_shell_has_a_no_js_fallback() {
    let html = document(&site_page("x"));
    assert!(html.contains("<noscript>"), "{html}");
    assert!(html.contains(".tabs__panel[hidden]"), "{html}");
    assert!(html.contains(".navbox__body[hidden]"), "{html}");
}

/// 外链那一路：页面里只有一个 `<link>` 一个 `<script src>`，资源由 `copy_assets` 拷出去
#[test]
fn linked_assets_point_at_the_copied_files() {
    let html = document(&page("x", Assets::Linked(ASSET_DIR.to_string())));
    assert!(
        html.contains(r#"<link rel="stylesheet" href="assets/mdpp.css">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<script src="assets/mdpp.js" data-mdpp-version="0.1.0" data-mdpp-mode="linked" data-mdpp-runtime="mdpp"></script>"#),
        "{html}"
    );
    // 内联的那些一份都不该在（`<noscript>` 里还有一小块降级 CSS，不算正文样式）
    assert!(!html.contains("--mdpp-bg"), "{html}");
    assert!(html.contains("x"), "{html}");
}

/// 资源真的被拷出去了，而且模板**不**跟着走 —— 它是模板，不是资源
#[test]
fn the_assets_are_copied_out_but_the_template_stays() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let copied = copy_assets(dir.path()).expect("拷得动");

    // 正文样式、站点外壳、脚本 —— 就这三个
    assert_eq!(copied, 3, "一份正文 css、一份站点 css、一个 js");
    assert!(dir.path().join("assets/mdpp.css").is_file());
    assert!(dir.path().join("assets/site.css").is_file());
    assert!(dir.path().join("assets/mdpp.js").is_file());
    assert!(
        !dir.path().join("assets/site.html").exists(),
        "模板不该进产物"
    );
    // 拷出来的字节与嵌进来的一样
    assert_eq!(
        std::fs::read_to_string(dir.path().join("assets/mdpp.js")).expect("读得动"),
        default_js()
    );
}

/// 站点那份外壳**只在 build 里引**。`render --full` 是自足的单文件页面，
/// 里面没有顶栏、也没有那一列
#[test]
fn the_site_shell_only_shows_up_in_a_built_site() {
    let built = document(&site_page("x"));
    assert!(built.contains(r#"href="assets/site.css""#), "{built}");
    let css = asset(SITE_CSS);
    for needle in ["1080px", ".mdpp-bar", ".mdpp-foot", ".mdpp-main"] {
        assert!(css.contains(needle), "site.css 里少了 {needle}");
    }

    let standalone = document(&page("x", Assets::Inline));
    // 单文件页面里没有 `site.css` 那条外链 —— 正文样式是内联进 `<style>` 的
    assert!(
        !standalone.contains(r#"href="assets/site.css""#),
        "{standalone}"
    );
    assert!(!standalone.contains("1080px"), "单文件页面不该带限宽");
    // 判据是**类选择器**（那个点得带着）：调色板里有两条 `--mdpp-bar-*` 变量
    // 跟着 `mdpp.css` 进了每一页，可单文件页面里没有顶栏这回事
    assert!(!standalone.contains(".mdpp-bar"), "单文件页面不该带顶栏");
}

/// 顶栏上是站点名、那句话说明与（给了的）仓库链接；站点名空就没有顶栏
#[test]
fn the_top_bar_carries_the_configured_phrases() {
    let html = document(&site_page("x"));
    assert!(
        html.contains("<span class=\"mdpp-bar__title\">某站点</span>"),
        "{html}"
    );
    assert!(
        html.contains("<span class=\"mdpp-bar__desc\">一句话说明</span>"),
        "{html}"
    );
    // 顶栏在正文之上
    assert!(
        html.find("mdpp-bar").expect("有顶栏")
            < html
                .find(r#"<article class="mdpp-content">"#)
                .expect("有正文")
    );

    // 没有站点名就没有顶栏 —— 那句话与仓库链接都跟着一起没了
    let nameless = document(&Page {
        chrome: Chrome {
            name: "   ".to_string(),
            description: "有说明也没用".to_string(),
            repo_url: "https://example.com".to_string(),
            copyright: "有版权也没用".to_string(),
            ..Chrome::default()
        },
        ..site_page("x")
    });
    assert!(!nameless.contains("mdpp-bar"), "{nameless}");
}

/// 顶栏里站点名**旁边**那个图标：配了就有一个 `<img>`，与站点名合成一组
/// （`.mdpp-bar__brand`）；不配就没有那个 `<img>`，其余一字不变
#[test]
fn a_logo_sits_next_to_the_site_name() {
    let with_logo = document(&Page {
        chrome: Chrome {
            logo: "assets/logo.svg".to_string(),
            ..site_page("x").chrome
        },
        ..site_page("x")
    });
    // 图在前、名在后，且合成一组 —— 分成两个 flex 项的话图得单独调 align-self
    assert!(
        with_logo.contains(concat!(
            r#"<span class="mdpp-bar__brand">"#,
            r#"<img class="mdpp-bar__logo" src="assets/logo.svg" alt="">"#,
            r#"<span class="mdpp-bar__title">某站点</span></span>"#,
        )),
        "{with_logo}"
    );

    // 不配就没有那个 `<img>`，而站点名照旧在
    let without = document(&site_page("x"));
    assert!(!without.contains("mdpp-bar__logo"), "{without}");
    assert!(
        without.contains("<span class=\"mdpp-bar__title\">某站点</span>"),
        "不配 logo 不该动站点名：{without}"
    );
}

/// 图标的地址要转义 —— 与 favicon 同一条规矩：配置是人写的，一个引号就能把
/// 属性截断（`onload=` 那种）
#[test]
fn the_logo_href_is_escaped() {
    let html = document(&Page {
        chrome: Chrome {
            logo: "assets/x\" onload=\"alert(1)".to_string(),
            ..site_page("x").chrome
        },
        ..site_page("x")
    });
    assert!(!html.contains("onload=\"alert"), "{html}");
    assert!(html.contains("&quot;"), "{html}");
}

/// 导航栏：每一项都是**同一个链接按钮**，当前页那一个只多 `aria-current`
/// （语义，不给颜色）；没有导航就不渲染那一行
#[test]
fn the_nav_bar_gives_every_item_the_same_class_and_only_marks_the_current_one() {
    let html = document(&Page {
        chrome: Chrome {
            name: "某站点".to_string(),
            nav: vec![
                NavLink {
                    label: "首页".to_string(),
                    href: "./".to_string(),
                    active: true,
                },
                NavLink {
                    label: "指南".to_string(),
                    href: "指南/".to_string(),
                    active: false,
                },
            ],
            ..Chrome::default()
        },
        ..site_page("x")
    });
    assert!(
        html.contains("<a class=\"mdpp-nav__link\" href=\"./\" aria-current=\"page\">首页</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a class=\"mdpp-nav__link\" href=\"指南/\">指南</a>"),
        "{html}"
    );
    // 当前页**不能**比别人多出一个 class —— 「另给一套样式」正是这条要钉住的事
    assert!(!html.contains("--active"), "{html}");
    // 样式侧同理：HTML 里没有那个 class，CSS 里再留着就是一条死规则
    assert!(
        !asset(SITE_CSS).contains("mdpp-nav__link--active"),
        "site.css 里还留着 --active 那条选择器"
    );
    // 导航在顶栏里，正文之上
    assert!(
        html.find("mdpp-nav").expect("有导航")
            < html
                .find(r#"<article class="mdpp-content">"#)
                .expect("有正文"),
        "{html}"
    );
    // 没有导航就不渲染那一行
    let plain = document(&site_page("x"));
    assert!(!plain.contains("mdpp-nav"), "{plain}");
}

/// 导航链接里的文字与地址都要转义 —— 配置是人写的，带引号/尖括号不该跑出来
#[test]
fn the_nav_bar_escapes_labels_and_hrefs() {
    let html = document(&Page {
        chrome: Chrome {
            name: "某站点".to_string(),
            nav: vec![NavLink {
                label: "a\"b<c".to_string(),
                href: "x\"y>z/".to_string(),
                active: false,
            }],
            ..Chrome::default()
        },
        ..site_page("x")
    });
    assert!(!html.contains("<c"), "{html}");
    assert!(html.contains("a&quot;b&lt;c"), "{html}");
    assert!(html.contains("x&quot;y&gt;z/"), "{html}");
}

/// 底栏上是 `copyright` 那一句话，右端是写死的「由 mdpp 生成」；
/// **没写 copyright 就没有底栏**（与 mkdocs 一个规矩）
#[test]
fn the_bottom_bar_carries_the_copyright() {
    let html = document(&site_page("x"));
    assert!(
        !html.contains("mdpp-foot"),
        "没写 copyright 就不该有底栏：{html}"
    );

    let with = document(&Page {
        chrome: Chrome {
            copyright: "© 某人".to_string(),
            ..site_page("x").chrome
        },
        ..site_page("x")
    });
    assert!(
        with.contains("<span class=\"mdpp-foot__text\">© 某人</span>"),
        "{with}"
    );
    assert!(with.contains("由 mdpp 生成"), "{with}");
    // 底栏在正文之下
    assert!(
        with.find("mdpp-content").expect("有正文") < with.find("mdpp-foot").expect("有底栏"),
        "{with}"
    );
}

/// 仓库链接只认 http/https —— 配置里写个 `javascript:` 就点一下执行一段
#[test]
fn the_repo_link_takes_only_web_urls() {
    for (url, wanted) in [
        ("https://example.com/x", true),
        ("http://example.com", true),
        ("javascript:alert(1)", false),
        ("data:text/html,x", false),
        ("  ", false),
    ] {
        let html = document(&Page {
            chrome: Chrome {
                repo_url: url.to_string(),
                ..site_page("x").chrome
            },
            ..site_page("x")
        });
        assert_eq!(
            html.contains("mdpp-bar__repo"),
            wanted,
            "{url:?} 认错了：{html}"
        );
    }
}

/// favicon 没有就是没有那个 `<link>`；给了就转义后写进 `href`
#[test]
fn the_favicon_tag_appears_only_when_there_is_one() {
    assert!(
        !document(&site_page("x")).contains("rel=\"icon\""),
        "不该有那个 link"
    );

    let html = document(&Page {
        favicon: Some("assets/图标.svg"),
        ..site_page("x")
    });
    assert!(
        html.contains(r#"<link rel="icon" href="assets/图标.svg">"#),
        "{html}"
    );
    // 里面的引号不能跑出来
    let hostile = document(&Page {
        favicon: Some("a\"onload=\"alert(1)"),
        ..site_page("x")
    });
    assert!(!hostile.contains("onload=\"alert"), "{hostile}");
}

/// `<meta name="description">` 与 `<link rel="canonical">` 各按各的来
#[test]
fn the_description_and_the_canonical_are_written_out() {
    let html = document(&Page {
        canonical: Canonical {
            root: Some("https://x.github.io/repo/".into()),
            path: "指南/安装/".into(),
        },
        ..site_page("x")
    });
    assert!(
        html.contains(r#"<meta name="description" content="一句话说明">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<link rel="canonical" href="https://x.github.io/repo/指南/安装/">"#),
        "{html}"
    );
}

/// **哪样没配就没有哪样** —— 空字符串的 `content=""` 与指向 `/` 的 canonical
/// 都是有害的：前者让搜索引擎拿一句空话当摘要，后者告诉它「全站只有一页」
#[test]
fn neither_tag_appears_without_the_thing_it_describes() {
    // 两样都没配
    let html = document(&page("x", Assets::Inline));
    assert!(!html.contains("name=\"description"), "没配就不该有：{html}");
    assert!(!html.contains("rel=\"canonical"), "没配就不该有：{html}");

    // 配了站点名但没配说明 —— 顶栏在，`<meta>` 不该在
    let html = document(&Page {
        chrome: Chrome {
            name: "某站点".into(),
            ..Chrome::default()
        },
        ..site_page("x")
    });
    assert!(html.contains("某站点"), "{html}");
    assert!(!html.contains("name=\"description"), "{html}");

    // 空串也当没配 —— 与顶栏那几个字段一个规矩
    let html = document(&Page {
        canonical: Canonical {
            root: Some(String::new()),
            path: String::new(),
        },
        ..site_page("x")
    });
    assert!(!html.contains("rel=\"canonical"), "{html}");
}

/// 首页的 canonical 就是站点根本身（末尾那个 `/` 不该有两个）
#[test]
fn a_canonical_is_the_site_root_plus_the_page() {
    let cases = [
        (
            Some("https://x.github.io/repo/"),
            "",
            "https://x.github.io/repo/",
        ),
        // 没写末尾那个 `/` 也认 —— 而拼接时不会漏也不会多
        (
            Some("https://x.github.io/repo"),
            "指南/",
            "https://x.github.io/repo/指南/",
        ),
        (None, "指南/", ""),
    ];
    for (root, path, expected) in cases {
        let url = Canonical {
            root: root.map(String::from),
            path: path.to_string(),
        }
        .url();
        if expected.is_empty() {
            assert_eq!(url, None, "没配 site_url 就没有 canonical");
        } else {
            assert_eq!(
                url.as_deref(),
                Some(expected),
                "root={root:?} path={path:?}"
            );
        }
    }
}

/// 那几句话是**纯文本**：转义之后原样进页面，不会变成标签或链接
#[test]
fn the_bar_phrases_are_escaped_not_rendered() {
    let html = document(&Page {
        chrome: Chrome {
            name: "<script>alert(1)</script>".to_string(),
            ..site_page("x").chrome
        },
        ..site_page("x")
    });
    assert!(!html.contains("<script>alert(1)"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
}

/// 默认样式要**真的是那一份**：空了也编译得过、页面也出得来，
/// 只是光着身子，没有人会吭声
#[test]
fn the_stylesheet_is_embedded_and_not_empty() {
    let css = default_css();
    assert!(css.len() > 500, "{}", css.len());
    assert!(css.contains(".mdpp-content"), "正文那一层得有样式");
}

/// 一份文件内部的次序错了会静悄悄地看起来对：
/// 变量那层必须在前（后面都在 `var(--mdpp-…)`），打印那层必须在最后（它是覆盖）。
#[test]
fn the_stylesheet_keeps_a_meaningful_internal_order() {
    let css = default_css();
    let base = css.find(":root").expect("变量那层");
    let callout = css.find(".callout {").expect("提示框");
    let tabs = css.find(".tabs__bar").expect("选项卡");
    let print = css.find("@media print").expect("打印");
    assert!(base < callout, "变量要在用它的那些之前");
    assert!(callout < tabs, "按拼进来的次序");
    assert!(tabs < print, "打印规则是覆盖，要压过前面的");
}

/// 正文那唯一一份样式表得**真的覆盖每个块** —— 少一节是静默的：
/// 页面少几样样式，而编译不报错。
#[test]
fn the_one_stylesheet_covers_every_block() {
    let css = default_css();
    for (thing, needle) in [
        ("配色变量", "--mdpp-accent"),
        ("提示框", "callout--warning"),
        ("平行列表", ".flatlist.hlist"),
        ("折叠", ".fold {"),
        ("标题带", "title-block"),
        ("基础盒子", ".surface"),
        ("媒体", "figcaption"),
        ("选项卡", "theme__part--dark"),
        ("导航框", "navbox__group"),
        ("图与公式", "math--display"),
        ("打印", "attr(data-label)"),
    ] {
        assert!(css.contains(needle), "{thing} 没进样式表（少 {needle}）");
    }
}

/// 裸表格（markdown 出来的那个，没有 class）得**真的有人给线** ——
/// `border-collapse` 只回答「线怎么合」，不给线的话表格就是一片裸字，深浅色都一样
#[test]
fn a_plain_markdown_table_gets_real_borders_from_the_stylesheet() {
    let css = default_css();
    // 只认没有 class 的那张表：`::fields` 与 `::navbox` 各有各的样子，不套圈框
    assert!(css.contains("table:not([class])"), "{css}");
    // 表头另有一条：底色走变量，于是深浅色是同一份规则而不是各写一份
    assert!(
        css.contains(".mdpp-content table:not([class]) thead th {"),
        "表头该有自己的那一条"
    );
    // 那条线要紧跟着选择器 —— 「整份 CSS 里哪儿都能找到」等于什么都没钉住。
    // 窗口按**字符**取：中文注释里一个字就是三个字节，按字节切会切在字中间
    let at = css
        .find(".mdpp-content table:not([class]) th,")
        .expect("格子那一条");
    let around: String = css[at..].chars().take(250).collect();
    assert!(
        around.contains("border: 1px solid var(--mdpp-border);"),
        "格子得有边框线：\n{around}"
    );
    // 宽表在窄屏上得**自己滚**，别把整张卡片撑破。两条得一起落：
    // `overflow` 单写在表格式盒上不生效，`display: block` 才是那把钥匙
    assert!(
        css.contains("display: block;\n  overflow-x: auto;"),
        "宽表得有自己那条横向滚动的路"
    );
}

/// `body` 的默认 8px 外边距得抹掉：顶栏要**贴着窗口边**，缩一圈就露出底色 ——
/// 而 `render --full` 那条路故意留着它（那儿没有顶栏，8px 正好当页边距）
#[test]
fn the_page_has_no_body_margin_in_a_built_site() {
    let css = asset(SITE_CSS);
    assert!(css.contains("body {\n  margin: 0;\n}"), "{css}");
    // 同一块底上不该再切一刀：站点名那一行与导航之间没有那条线
    assert!(!css.contains(".mdpp-nav {"), "导航那条不该再单独上色或画线");
    // 纸上要有留白：正文不能顶到纸边
    let print_at = css.find("@media print").expect("打印那一段");
    let print_css: String = css[print_at..].chars().take(900).collect();
    assert!(print_css.contains("padding: 1.5rem;"), "{print_css}");
}

/// 顶上那一条（站点名那一行 + 下面的导航）是**一整块有色的底**：底色取
/// `--mdpp-bar-bg`、字取 `--mdpp-bar-fg`。两个变量的默认值在 `mdpp.css` 的调色板
/// 里，`theme_color` 再盖掉它们 —— 所以 `site.css` 里一个颜色都没写死
#[test]
fn the_top_bar_is_one_colored_strip_read_off_css_variables() {
    let css = asset(SITE_CSS);
    assert!(css.contains("background: var(--mdpp-bar-bg)"), "{css}");
    assert!(css.contains("color: var(--mdpp-bar-fg)"), "{css}");
    // 导航那条自己不再上色：它与站点名那一行是同一块底。
    // `--mdpp-accent` 是**前景色**（深色下它是浅蓝），拿它铺底就是一条刺眼的亮带
    assert!(!css.contains("background: var(--mdpp-accent)"), "{css}");
}

/// 配了 `theme_color` 就落**四层**覆盖 —— 少一层就有一页掉回默认蓝：
/// `:root[data-mdpp-theme='light']` 的权重比 `:root` 高，钉死浅色的页面
/// 会把默认值捡回来
#[test]
fn a_configured_theme_color_lands_in_all_four_palette_layers() {
    let color = Color::parse("#123456").expect("合法的十六进制");
    let mut page = site_page("正文");
    page.chrome.theme_color = Some(color);
    let html = document(&page);

    let light = format!("--mdpp-bar-bg: {}; --mdpp-bar-fg: #ffffff;", color.css());
    let dark = format!(
        "--mdpp-bar-bg: {}; --mdpp-bar-fg: #ffffff;",
        color.darkened().css()
    );
    for needle in [
        format!(":root {{ {light} }}"),
        format!(":root[data-mdpp-theme='light'] {{ {light} }}"),
        format!(":root[data-mdpp-theme='dark'] {{ {dark} }}"),
    ] {
        assert!(html.contains(&needle), "少了「{needle}」：\n{html}");
    }
    // 跟随系统那一层靠 `prefers-color-scheme`，它得在深色媒体查询里
    assert!(
        html.contains(&format!(
            "@media (prefers-color-scheme: dark) {{\n  :root {{ {dark} }}"
        )),
        "少了跟随系统那一层：\n{html}"
    );
    // 浏览器地址栏也染上 —— 同一个算法，深浅两份
    assert!(
        html.contains(&format!(
            "<meta name=\"theme-color\" content=\"{}\">",
            color.css()
        )),
        "{html}"
    );
}

/// 没配 `theme_color` 就**没有那几层覆盖**：默认值在 `mdpp.css` 的调色板里，
/// 不必每页都带着一份一模一样的
#[test]
fn no_theme_color_means_no_override_block() {
    let html = document(&site_page("正文"));
    assert!(!html.contains("--mdpp-bar-bg"), "{html}");
    assert!(!html.contains("theme-color"), "{html}");
}

/// 字色按**亮度**挑：亮底上再压白字就看不清了（`#ffd166` 那种黄）
#[test]
fn a_light_theme_color_gets_dark_text_on_it() {
    let mut page = site_page("正文");
    page.chrome.theme_color = Color::parse("#ffd166");
    let html = document(&page);
    assert!(html.contains("--mdpp-bar-fg: #1a1c1f"), "{html}");
}

/// `::css` 注入的 CSS 靠 `var(--mdpp-…)` 取配色，变量名要与 `mdpp.css` 对得上
#[test]
fn the_palette_variables_are_all_there() {
    let css = default_css();
    for name in [
        "--mdpp-accent",
        "--mdpp-text",
        "--mdpp-text-dim",
        "--mdpp-link",
        "--mdpp-border",
        "--mdpp-surface",
        "--mdpp-danger",
        // 顶栏那块底与压在它上面的字 —— `site.css` 里一个颜色都没写死，
        // 全靠这两条；深色那两层里也得各有各的一份
        "--mdpp-bar-bg",
        "--mdpp-bar-fg",
    ] {
        assert!(css.contains(name), "少了 {name}");
    }
}

/// 内联脚本要整份在，也**只带最基本的那几样** —— 少了它页面点不动，
/// 多带一样就是替用它的人先做了一个决定
#[test]
fn the_script_is_embedded_and_does_the_minimum() {
    let js = default_js();
    assert!(js.contains("tabs__tab--on"), "{}", js);
    assert!(js.contains("DOMContentLoaded"), "{}", js);
    assert!(js.contains("addEventListener(\"click\""), "点击切换不能少");
    // 默认脚本多带一样（方向键、动画、记忆…）就有人得自己去拆，
    // 而拆一段别人的行为比不写要费劲
    assert!(!js.contains("ArrowRight"), "方向键不该由官方工具来定");
    assert!(
        !js.contains("localStorage"),
        "记住上次那栏不该由官方工具来定"
    );
}

/// 页面里的每一个 `<script>` 都带生成器环境：版本与挂载方式。
/// 单文件、站点外链、CDN、用户附带脚本都算。
#[test]
fn every_generated_script_carries_the_generator_environment() {
    let html = document(&Page {
        body: "<pre class=\"mermaid\">graph TD; A-->B;</pre><div class=\"math\" data-tex=\"x\"></div><pre class=\"language-rust\">fn main() {}</pre>",
        extra_js: &["// 我自己的".to_string()],
        ..page("x", Assets::Inline)
    });
    for segment in html.split("<script ").skip(1) {
        let head = &segment[..segment.find('>').expect("script 有开标签")];
        assert!(
            head.contains("data-mdpp-version=\"0.1.0\""),
            "脚本少了版本：{head}"
        );
        assert!(head.contains("data-mdpp-mode="), "脚本少了模式：{head}");
        assert!(
            head.contains("data-mdpp-runtime="),
            "脚本少了运行时：{head}"
        );
    }
    assert!(html.contains("data-mdpp-runtime=\"mermaid\""), "{html}");
    assert!(html.contains("data-mdpp-runtime=\"katex\""), "{html}");
    assert!(html.contains("data-mdpp-runtime=\"highlight\""), "{html}");
}

/// 附带的那些接在默认的**后面** —— 同优先级时后者胜出，
/// 所以「改一下」不必把默认那份重写一遍
#[test]
fn the_extra_files_come_after_the_defaults() {
    let html = document(&Page {
        extra_css: &["/* 我自己的 */".to_string()],
        extra_js: &["// 我自己的".to_string()],
        ..page("x", Assets::Inline)
    });
    let default_css = html.find(".mdpp-content").expect("默认样式");
    let extra_css = html.find("/* 我自己的 */").expect("附带的样式");
    let default_js = html.find("tabs__tab--on").expect("默认脚本");
    let extra_js = html.find("// 我自己的").expect("附带的脚本");
    assert!(default_css < extra_css, "附带的样式该在默认之后：{html}");
    assert!(default_js < extra_js, "附带的脚本该在默认之后：{html}");
}

/// 附带文件里出现闭合标签也不能提前收尾 —— 那是**用户写的**，比默认那份危险。
/// 默认样式与附带的那几份各是各自的一块 `<style>`，所以数出来是两块
#[test]
fn the_extra_files_cannot_close_their_own_tags() {
    let html = document(&Page {
        extra_css: &["a{}</style><p>顶出来了</p>".to_string()],
        extra_js: &[r#"var s = "</script>";"#.to_string()],
        ..page("x", Assets::Inline)
    });
    // 默认样式、附带样式，外加 `<noscript>` 里那小块降级样式
    assert_eq!(html.matches("</style>").count(), 3, "{html}");
    // 用户那半个闭合标签被去掉了，剩下的字留在样式块里当无害的 CSS 文本
    assert!(
        !html.contains("a{}</style>"),
        "附带的样式提前收尾了：{html}"
    );
    assert!(!html.contains(r#"</script>";"#), "{html}");
}

/// 占位符填**一趟**：正文里写着 `{{title}}` 也不能被改掉
#[test]
fn the_body_is_filled_last_so_placeholders_in_it_survive() {
    let html = document(&page("<p>{{title}} 与 {{unknown}}</p>", Assets::Inline));
    assert!(html.contains("<p>{{title}} 与 {{unknown}}</p>"), "{html}");
}

/// 认不出的占位符原样留着 —— 模板写错一个字要看得见，而不是悄悄吞掉
#[test]
fn an_unknown_placeholder_stays_visible() {
    let out = fill("a {{what}} b {{unclosed", &[("known", "1")]);
    assert_eq!(out, "a {{what}} b {{unclosed");
    assert_eq!(fill("{{known}}", &[("known", "1")]), "1");
}

/// 模板里要没有那个占位符 —— 少了的话页面照样出得来，只是标题或脚本悄悄没了
#[test]
fn the_template_still_has_the_placeholders_the_filler_fills() {
    let template = template();
    for name in [
        "lang_attr",
        "theme_attr",
        "title",
        "icons",
        "meta",
        "styles",
        "scripts",
        "bar",
        "foot",
        "body",
    ] {
        assert!(
            template.contains(&format!("{{{{{name}}}}}")),
            "模板里少了 {{{{{name}}}}}"
        );
    }
}

/// 页面里若用了 `::mermaid`（`class="mermaid"`），默认 `cdn` 模式会把
/// mermaid.js 的 `<script src="…">` 写进页面；`none` 模式则不写。
#[test]
fn external_script_cdn_injects_mermaid_when_needed() {
    let page = Page {
        body: "<pre class=\"mermaid\">graph TD; A-->B;</pre>",
        external_script: ExternalScript::Cdn,
        ..site_page("x")
    };
    let html = document(&page);
    assert!(
        html.contains("cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js"),
        "{html}"
    );
    assert!(!html.contains("katex"), "{html}");
}

/// 页面里若用了 `::math`（`class="math"`），默认 `cdn` 模式会把
/// KaTeX 的 `<script src="…">` 写进页面。
#[test]
fn external_script_cdn_injects_katex_when_needed() {
    let page = Page {
        body: "<div class=\"math math--display\">E=mc^2</div>",
        external_script: ExternalScript::Cdn,
        ..site_page("x")
    };
    let html = document(&page);
    assert!(
        html.contains("cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js"),
        "{html}"
    );
    // 只有 JS 没有 CSS 的话，分式/根号/间距排不对
    assert!(
        html.contains("cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css"),
        "{html}"
    );
    assert!(!html.contains("mermaid"), "{html}");
}

/// `external_script = none` 时，哪怕页面里有 mermaid / math，也不写脚本标签。
#[test]
fn external_script_none_injects_nothing() {
    let page = Page {
        body: "<pre class=\"mermaid\">graph TD; A-->B;</pre><div class=\"math\">x</div>",
        external_script: ExternalScript::None,
        ..site_page("x")
    };
    let html = document(&page);
    assert!(!html.contains("cdn.jsdelivr.net"), "{html}");
}

/// 页面里既没有 mermaid 也没有 math，`cdn` 模式也不写脚本标签。
#[test]
fn external_script_cdn_no_diagram_no_script() {
    let page = Page {
        body: "<p>只是普通文本</p>",
        external_script: ExternalScript::Cdn,
        ..site_page("x")
    };
    let html = document(&page);
    assert!(!html.contains("cdn.jsdelivr.net"), "{html}");
}

/// 页面里有代码块（`class="language-…"`）时，`cdn` 模式把 highlight.js
/// 连同 github 浅深两份主题一起写进页面 —— 主题靠 `media` 让系统挑。
#[test]
fn external_script_cdn_injects_hljs_with_both_themes() {
    let page = Page {
        body: "<pre><code class=\"language-rust\">fn main() {}</code></pre>",
        external_script: ExternalScript::Cdn,
        ..site_page("x")
    };
    let html = document(&page);
    assert!(
        html.contains("cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.7.0/build/highlight.min.js"),
        "{html}"
    );
    assert!(
        html.contains("styles/github.min.css"),
        "浅色主题没进来：{html}"
    );
    assert!(
        html.contains("styles/github-dark.min.css"),
        "深色主题没进来：{html}"
    );
    assert!(
        html.contains("media=\"(prefers-color-scheme: light)\"")
            && html.contains("media=\"(prefers-color-scheme: dark)\""),
        "主题要靠 media 让系统挑：{html}"
    );
}

/// 没有代码块的页面，highlight.js 与主题样式都不带 —— 多背一份不值。
#[test]
fn no_code_block_no_hljs_nor_theme() {
    let page = Page {
        body: "<p>只是普通文本</p>",
        external_script: ExternalScript::Cdn,
        ..site_page("x")
    };
    let html = document(&page);
    assert!(!html.contains("highlightjs"), "{html}");
    assert!(!html.contains("github"), "{html}");
}

/// 主题摆在用户附的样式之前：`--css` 里换一套自己的 highlight.js 配色，
/// 仍然是最后说话的那个。
#[test]
fn hljs_theme_yields_to_the_users_own_css() {
    let extra = "/* 这是 --css 附的样式 */".to_string();
    let page = Page {
        body: "<pre><code class=\"language-rust\">fn main() {}</code></pre>",
        external_script: ExternalScript::Cdn,
        extra_css: &[extra],
        ..site_page("x")
    };
    let html = document(&page);
    let theme = html.find("github.min.css").expect("主题在");
    let user = html.find("--css 附的样式").expect("用户样式也在");
    assert!(theme < user, "主题要排在用户样式之前：\n{html}");
}
