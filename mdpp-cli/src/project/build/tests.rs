use super::*;

use crate::cli::options::{Build, Init};
use crate::project::config::NavEntry;
use crate::project::site::{INDEX_KEY, output_of};

use super::clean::staging_dir;
use super::nav::{nav_for, resolve_nav, toc_body};
use super::toc;
use std::collections::BTreeMap;

/// 临时目录名不跟产物目录名走：`build_dir` 可以是中文，但临时名字只用 ASCII
#[test]
fn the_staging_name_is_ascii_even_for_a_non_ascii_build_dir() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let staging = staging_dir(&dir.path().join("构建")).expect("算得出");
    let name = staging
        .file_name()
        .expect("有名字")
        .to_string_lossy()
        .into_owned();
    assert!(name.starts_with(".mdpp-tmp-"), "{name}");
    assert!(name.is_ascii(), "{name}");
}

/// 一个小站点：首页 + 两页，`指南/入门` 与 `手册`
fn site_of(files: &[&str]) -> (tempfile::TempDir, Site) {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    for file in files {
        let path = dir.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建得出目录");
        }
        std::fs::write(&path, "# 页").expect("写得下");
    }
    let site = Site::collect(dir.path()).expect("扫得动");
    (dir, site)
}

/// 生成的目录页：`insert` 进去之后导航与站内链接都指得到它
#[test]
fn an_inserted_page_lands_in_the_site() {
    let (_dir, mut site) = site_of(&["index.mdpp", "指南/入门.mdpp"]);
    site.insert(Page {
        source: PathBuf::new(),
        key: "目录".to_string(),
        output: output_of("目录"),
    })
    .expect("插得进");
    assert!(site.by_key("目录").is_some(), "{:?}", site.pages());
    assert_eq!(site.pages().len(), 3);

    // 撞了已有的键就报错 —— 生成页不该默默盖住作者自己写的页
    let mut again = site.clone();
    let error = again
        .insert(Page {
            source: PathBuf::new(),
            key: "目录".to_string(),
            output: output_of("目录"),
        })
        .expect_err("重复键该报错");
    assert!(error.contains("目录"), "{error}");
}

/// 导航解析：字符串按 `[[名字]]` 那套规则，单项表按键；目录按钮自动补在末尾
#[test]
fn nav_resolves_against_the_site_and_appends_the_toc_button() {
    let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "手册.md"]);
    let resolved = resolve_nav(
        &[
            NavEntry::SameName("手册".to_string()),
            NavEntry::Named(BTreeMap::from([(
                "指南".to_string(),
                "指南/入门".to_string(),
            )])),
        ],
        "目录",
        &site,
    )
    .expect("都指得到");
    assert_eq!(
        resolved,
        vec![
            ("手册".to_string(), "手册".to_string()),
            ("指南".to_string(), "指南/入门".to_string()),
            // 自动补上的目录按钮
            ("目录".to_string(), "目录".to_string()),
        ]
    );

    // 作者已经指过目录页，就不再自动补 —— 补两个按钮就重了
    let mut site = site;
    site.insert(Page {
        source: PathBuf::new(),
        key: "目录".to_string(),
        output: output_of("目录"),
    })
    .expect("插得进");
    let resolved = resolve_nav(
        &[NavEntry::Named(BTreeMap::from([(
            "全部页面".to_string(),
            "目录".to_string(),
        )]))],
        "目录",
        &site,
    )
    .expect("指得到");
    assert_eq!(resolved.len(), 1, "{resolved:?}");

    // 指不到的就报错，报错里带着那一项
    let error =
        resolve_nav(&[NavEntry::SameName("没有这页".to_string())], "", &site).expect_err("该报错");
    assert!(error.contains("没有这页"), "{error}");
    let error = resolve_nav(
        &[NavEntry::Named(BTreeMap::from([(
            "别名".to_string(),
            "指南/不存在".to_string(),
        )]))],
        "",
        &site,
    )
    .expect_err("该报错");
    assert!(error.contains("指南/不存在"), "{error}");
}

/// 一页的导航：地址是相对那一页的，当前页只多一个 `aria-current`
#[test]
fn nav_for_marks_the_current_page_and_uses_relative_hrefs() {
    let list = vec![
        ("首页".to_string(), INDEX_KEY.to_string()),
        ("手册".to_string(), "手册".to_string()),
    ];
    let on_manual = nav_for(&list, "手册");
    assert_eq!(on_manual.len(), 2);
    // 手册 → 首页：退一层到根
    assert_eq!(on_manual[0].href, "../");
    assert!(!on_manual[0].active);
    // 手册 → 手册：退一层再进目录 —— 页与页之间永远是相对地址
    assert_eq!(on_manual[1].href, "../手册/");
    assert!(on_manual[1].active);

    let on_index = nav_for(&list, INDEX_KEY);
    // 首页 → 首页：地址是当前目录自己
    assert_eq!(on_index[0].href, "./");
    assert!(on_index[0].active);
    // 首页 → 手册：直接进目录
    assert_eq!(on_index[1].href, "手册/");
    assert!(!on_index[1].active);
}

/// 目录页的正文：每一页一个链接，首页显示站点名
#[test]
fn the_toc_body_lists_every_page() {
    let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp"]);
    let body = toc_body(&site, "桥梁笔记", "目录");
    assert!(body.contains("<ul class=\"mdpp-toc-page\">"), "{body}");
    assert!(body.contains(">桥梁笔记<"), "首页该显示站点名：{body}");
    assert!(body.contains(">入门<"), "{body}");
    assert!(body.contains("指南/入门"), "键该陪着链接显示：{body}");
    // 链接是相对目录页的：目录页在 `build/目录/`，回首页要退一层
    assert!(body.contains(r#"href="../""#), "{body}");
    assert!(body.contains(r#"href="../指南/入门/""#), "{body}");
}

/// 侧边目录从**渲染后的正文**里挑 h2/h3：链接直接用标题上那个锚点。
/// h1 是这一页的标题，不再进目录。
#[test]
fn the_sidebar_lists_h2_and_h3_with_their_anchors() {
    let body = concat!(
        r#"<h1 id="页面标题">页面标题</h1>"#,
        r#"<h2 id="安装">安装</h2>"#,
        r#"<p>正文</p>"#,
        r#"<h3 id="细节">细节 <code>a&amp;b</code></h3>"#,
    );
    let html = toc::sidebar(body);
    assert!(html.contains(r##"<a href="#安装">安装</a>"##), "{html}");
    assert!(
        html.contains(r##"<a href="#细节">细节 a&amp;b</a>"##),
        "{html}"
    );
    assert!(
        !html.contains("页面标题"),
        "h1 是页标题，不该再进目录：{html}"
    );
    // 宽屏一列 + 窄屏可展开入口，两份结构都在
    assert!(
        html.contains(r#"<aside class="mdpp-toc mdpp-toc--wide">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<details class="mdpp-toc mdpp-toc--narrow">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<summary class="mdpp-toc__summary">本页目录</summary>"#),
        "{html}"
    );
    // 两级各带自己的 class（缩进靠它）
    assert!(html.contains("mdpp-toc__item--h2"), "{html}");
    assert!(html.contains("mdpp-toc__item--h3"), "{html}");
}

/// 没有章节标题就不出目录：只写 h1、或者标题没有 `id` 都不该留下一个空盒子
#[test]
fn a_page_without_section_headings_gets_no_sidebar() {
    assert!(toc::sidebar("<p>只有正文</p>").is_empty());
    assert!(
        toc::sidebar(r#"<h1 id="页">页</h1>"#).is_empty(),
        "h1 是页标题，不是章节"
    );
    assert!(
        toc::sidebar("<h2>没有锚点</h2>").is_empty(),
        "没有 id 就没有能跳过去的目标"
    );
}

/// 属性不按 id 开头、引号里带 `>`、值里带实体，都要照样读出真正的锚点
#[test]
fn the_sidebar_reads_the_anchor_through_other_attributes_and_entities() {
    let html = toc::sidebar(r#"<h2 class="x" data-y="a>b" id='A&amp;B'>标题</h2>"#);
    assert!(html.contains(r##"href="#A&amp;B""##), "{html}");
    assert!(html.contains(">标题</a>"), "{html}");
}

/// 每个项目一个临时站点：`init` 建出来的骨架已经能 build
fn scaffold() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let root = dir.path().join("站点");
    crate::project::init::run(&Init {
        dir: Some(root.clone()),
        name: Some("测试站点".to_string()),
    })
    .expect("建得出");
    (dir, Project::load(&root).expect("这是个项目"))
}

fn build_options() -> Build {
    Build {
        clean: true,
        strict: false,
        external_script: String::from("cdn"),
        jobs: 0,
        allow_outside: false,
    }
}

/// 端到端：构建出来的页面带着侧边目录；只有 h1 的页面没有空目录。
/// 目录里的链接是 `#锚点`，不经过站点根，`site_url` / 资源前缀都碰不到它。
#[test]
fn a_built_page_carries_its_sidebar_and_a_plain_page_has_none() {
    let (_dir, project) = scaffold();
    std::fs::write(
        project.pages_dir().join("指南.mdpp"),
        "# 指南\n\n## 安装\n\n### 细节\n",
    )
    .expect("写得下");
    std::fs::write(
        project.pages_dir().join("空白.mdpp"),
        "# 空白\n\n只有一段正文。\n",
    )
    .expect("写得下");

    build_into(&project, &build_options(), &project.site_dir()).expect("出得来");

    let page = std::fs::read_to_string(project.site_dir().join("指南/index.html")).expect("读得动");
    assert!(
        page.contains(r#"<aside class="mdpp-toc mdpp-toc--wide">"#),
        "{page}"
    );
    assert!(page.contains(r##"href="#安装""##), "{page}");
    assert!(page.contains(r##"href="#细节""##), "{page}");
    assert!(
        page.contains(r#"<summary class="mdpp-toc__summary">本页目录</summary>"#),
        "{page}"
    );
    // 目录在正文之前（宽屏那一列在左边）
    assert!(
        page.find("mdpp-toc").expect("有目录")
            < page
                .find(r#"<article class="mdpp-content">"#)
                .expect("有正文"),
        "{page}"
    );

    let plain =
        std::fs::read_to_string(project.site_dir().join("空白/index.html")).expect("读得动");
    assert!(
        !plain.contains("mdpp-toc"),
        "只有 h1 的页面不该有空目录：{plain}"
    );
}

/// 窄屏收起靠 CSS 切两份结构，不靠 JS：断点、宽屏隐藏、details 出现都得在
#[test]
fn the_stylesheet_collapses_the_sidebar_at_the_narrow_breakpoint() {
    let css = crate::render::page::default_css();
    let narrow = css
        .find("@media (max-width: 99.99rem)")
        .expect("窄屏断点该在");
    let narrow: String = css[narrow..].chars().take(900).collect();
    assert!(narrow.contains(".mdpp-toc--wide {"), "{narrow}");
    assert!(narrow.contains("display: none;"), "{narrow}");
    assert!(narrow.contains(".mdpp-toc--narrow {"), "{narrow}");
    assert!(narrow.contains("display: block;"), "{narrow}");
    assert!(narrow.contains(".mdpp-toc__summary"), "{narrow}");

    // 纸上不印目录
    let print = css.find("@media print").expect("打印规则该在");
    let print: String = css[print..].chars().take(300).collect();
    assert!(print.contains(".mdpp-toc {"), "{print}");
    assert!(print.contains("display: none;"), "{print}");
}
