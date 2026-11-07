use super::*;

use crate::project::config::NavEntry;
use crate::project::site::{INDEX_KEY, output_of};

use super::clean::staging_dir;
use super::nav::{nav_for, resolve_nav, toc_body};
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
    assert!(body.contains("<ul class=\"toc\">"), "{body}");
    assert!(body.contains(">桥梁笔记<"), "首页该显示站点名：{body}");
    assert!(body.contains(">入门<"), "{body}");
    assert!(body.contains("指南/入门"), "键该陪着链接显示：{body}");
    // 链接是相对目录页的：目录页在 `build/目录/`，回首页要退一层
    assert!(body.contains(r#"href="../""#), "{body}");
    assert!(body.contains(r#"href="../指南/入门/""#), "{body}");
}
