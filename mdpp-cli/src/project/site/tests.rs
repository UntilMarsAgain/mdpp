use std::sync::{Arc, Mutex};

use mdpp::link::{LinkResolver, ResolvedLink};

use super::paths::is_template_target;
use super::*;

fn temp_pages(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    for file in files {
        let path = dir.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建得出目录");
        }
        std::fs::write(&path, "# 页").expect("写得下");
    }
    dir
}

fn site_of(files: &[&str]) -> (tempfile::TempDir, Site) {
    let dir = temp_pages(files);
    let site = Site::collect(dir.path()).expect("扫得动");
    (dir, site)
}

/// 页面键最终会变成产物里的路径：`.` / `..` / 反斜杠 / 控制字符一律拒绝
#[test]
fn page_keys_refuse_path_segments_that_can_escape() {
    for key in [
        "",
        "..",
        "../x",
        "a/../b",
        "a//b",
        "/x",
        "a\\b",
        "a\u{0}b",
        "CON",
        "nul.txt",
        "指南/..",
    ] {
        assert!(check_key(key).is_err(), "{key:?} 该被拒绝");
    }
    for key in ["index", "指南/入门", "2.0", "我的 页", "com10", "auxiliary"] {
        assert!(check_key(key).is_ok(), "{key:?} 该通过");
    }
}

/// 即使上游漏了校验，拼输出路径这一步也不能留下 `..`
#[test]
fn output_of_never_keeps_parent_segments() {
    assert_eq!(output_of("../pwn"), PathBuf::from("pwn/index.html"));
    assert_eq!(output_of("a/../../b"), PathBuf::from("a/b/index.html"));
    assert_eq!(output_of("index"), PathBuf::from("index.html"));
}

/// 生成页（目录页）的键也来自配置，同样不能逃出站点目录
#[test]
fn a_generated_page_key_cannot_escape() {
    let (_dir, mut site) = site_of(&["index.mdpp"]);
    let page = Page {
        source: PathBuf::new(),
        key: "../pwn".to_string(),
        output: output_of("../pwn"),
    };
    let error = site.insert(page).expect_err("该拒绝");
    assert!(error.contains("不是页名"), "{error}");
}

/// Unix 上反斜杠是合法文件名字符；它不能被当成目录分隔符来解释
#[cfg(unix)]
#[test]
fn a_backslash_in_a_page_name_cannot_escape_the_site_dir() {
    let dir = temp_pages(&["a\\..\\..\\pwn.mdpp"]);
    let error = Site::collect(dir.path()).expect_err("该拒绝");
    assert!(
        error.contains("不是页名") || error.contains("反斜杠"),
        "{error}"
    );
}

/// 符号链接不跟：链接指向哪里是链接自己的事，站点不该替它把外面纳进来
#[cfg(unix)]
#[test]
fn a_symlinked_page_is_not_followed() {
    let dir = temp_pages(&["real.mdpp"]);
    std::os::unix::fs::symlink(dir.path().join("real.mdpp"), dir.path().join("link.mdpp"))
        .expect("建得出符号链接");
    let site = Site::collect(dir.path()).expect("扫得动");
    let keys: Vec<&str> = site.pages().iter().map(|page| page.key.as_str()).collect();
    assert_eq!(keys, vec!["real"]);
    assert!(
        site.others().iter().any(|one| one == "link.mdpp"),
        "{:?}",
        site.others()
    );
}

/// 键去掉后缀、换成 `/`，输出永远是 `<键>/index.html`
#[test]
fn pages_become_keys_and_index_files() {
    let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "手册.md"]);
    let keys: Vec<&str> = site.pages().iter().map(|page| page.key.as_str()).collect();
    assert_eq!(keys, vec!["index", "手册", "指南/入门"]);

    assert_eq!(
        site.by_key("指南/入门").map(|page| page.output.clone()),
        Some(PathBuf::from("指南/入门/index.html"))
    );
    assert_eq!(
        site.by_key("index").map(|page| page.output.clone()),
        Some(PathBuf::from("index.html"))
    );
}

/// 一页的**地址**是目录式的，而首页落在站点根上（没有自己那一层）
#[test]
fn a_pages_url_is_the_directory_it_was_written_to() {
    let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "我的 页.mdpp"]);
    assert_eq!(site.by_key("index").expect("有首页").url_path(), "");
    assert_eq!(
        site.by_key("指南/入门").expect("有那一页").url_path(),
        "指南/入门/"
    );
    // 键里那点碍事的字符要编码，中文原样留着
    assert_eq!(
        site.by_key("我的 页").expect("有那一页").url_path(),
        "我的%20页/"
    );
}

/// 普通 markdown 也是页面 —— `.mdpp` 是本方言的写法，不是唯一被认的
#[test]
fn plain_markdown_counts_as_a_page_too() {
    let (_dir, site) = site_of(&["甲.md"]);
    assert_eq!(site.pages().len(), 1);
}

/// 后缀不对的不是页面，隐藏的目录也不收
#[test]
fn other_files_and_hidden_directories_are_skipped() {
    let (_dir, site) = site_of(&[
        "甲.mdpp",
        "笔记.txt",
        "没有后缀",
        ".git/config",
        ".草稿/被藏起来.mdpp",
    ]);
    let keys: Vec<&str> = site.pages().iter().map(|page| page.key.as_str()).collect();
    assert_eq!(keys, vec!["甲"]);
}

/// 同一个键出现两次时 `.mdpp` 胜出，并记一笔 —— 默默挑一个会让人以为另一份还在用
#[test]
fn the_same_key_twice_keeps_the_dialect_file() {
    let (_dir, site) = site_of(&["甲.md", "甲.mdpp"]);
    assert_eq!(site.pages().len(), 1);
    assert_eq!(site.pages()[0].source.extension().unwrap(), "mdpp");
    assert_eq!(site.duplicates(), ["甲"]);
}

/// 手写出来的几种写法都要落到同一页
#[test]
fn targets_are_matched_loosely() {
    for target in ["指南/入门", "./指南/入门", "指南\\入门", "指南/入门.mdpp"] {
        assert_eq!(normalize(target).as_deref(), Some("指南/入门"), "{target}");
    }
    // 只写文件名的那一条归一后仍是文件名 —— 落到哪一页由「全站唯一」那条规则管
    assert_eq!(normalize(" 入门 ").as_deref(), Some("入门"));
    assert_eq!(normalize("入门.md").as_deref(), Some("入门"));
    assert_eq!(normalize("A/B.MDPP").as_deref(), Some("a/b"));
    assert_eq!(normalize(""), None);
    assert_eq!(normalize("   "), None);
}

/// 只认 `template:` 这一个前缀，大小写与空格都不挑。判宽了，`帮助:入门` 这样的
/// **真红链**会被当成模板页跳过、报告里少一条；判严了，`Template: 卡片` 又会被虚报成缺页
#[test]
fn template_targets_are_not_site_pages() {
    assert!(is_template_target("template:卡片"));
    assert!(is_template_target("Template: 卡片"));
    assert!(!is_template_target("卡片"));
    assert!(!is_template_target("帮助:入门"));
}

/// 资源前缀要数对层数 —— 少一个 `../`，浏览器会把 404 的样式当成空样式，
/// 页面于是光着身子出来，而且**不报错**
#[test]
fn the_asset_prefix_counts_the_depth_of_the_page() {
    assert_eq!(assets_href("index"), "assets");
    assert_eq!(assets_href("示例"), "../assets");
    assert_eq!(assets_href("指南/安装"), "../../assets");
    assert_eq!(assets_href("指南/深入/安装"), "../../../assets");
}

/// 页与页之间是相对的，每退一层加一个 `../`，目标总是以 `/` 结尾
#[test]
fn hrefs_are_relative_and_end_with_a_slash() {
    assert_eq!(href("index", "指南/入门"), "指南/入门/");
    assert_eq!(href("指南/入门", "手册"), "../../手册/");
    assert_eq!(href("指南/深入/安装", "手册/入门"), "../../../手册/入门/");
}

/// 回首页要退**几层**：页在 `/指南/深入/安装/` 就退三层，与键的段数一样
#[test]
fn the_home_link_walks_up_the_right_number_of_levels() {
    assert_eq!(href("index", "index"), "./");
    assert_eq!(href("示例", "index"), "../");
    assert_eq!(href("指南/入门", "index"), "../../");
    assert_eq!(href("指南/深入/安装", "index"), "../../../");
}

/// 深层页里的链接必须真的指到那一页 —— 少退一层落到 `/指南/手册/` 而不是 `/手册/`
#[test]
fn a_link_from_a_deep_page_lands_on_the_right_page() {
    assert_eq!(href("指南/入门", "手册"), "../../手册/");
    assert_eq!(href("指南/深入/安装", "指南/入门"), "../../../指南/入门/");
    assert_eq!(href("指南/深入/安装", "index"), "../../../");
}

/// 键里的空格与 `#` 要编码；中文原样留着
#[test]
fn hrefs_are_percent_encoded() {
    assert_eq!(href("index", "我的 页"), "我的%20页/");
    assert_eq!(href("index", "a#b"), "a%23b/");
}

/// 解析器：认得出的给地址，认不出的记一笔红链
#[test]
fn the_resolver_links_inside_the_site_and_reports_the_rest() {
    let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "手册/安装.mdpp"]);
    let site = Arc::new(site);
    let missing = Arc::new(Mutex::new(Vec::new()));

    let index = site.by_key("index").expect("有首页").clone();
    let home = Links::for_page(site.clone(), &index, missing.clone());
    let html = home.resolve("指南/入门").expect("认得出");
    let ResolvedLink::Link(link) = html else {
        panic!("该给一个地址");
    };
    assert_eq!(link.url, "指南/入门/");
    assert!(home.resolve("没有这一页").is_none());
    assert!(missing.lock().expect("锁得住").len() == 1);

    let deep = site.by_key("指南/入门").expect("有那一页").clone();
    let inside = Links::for_page(site, &deep, missing.clone());
    let ResolvedLink::Link(link) = inside.resolve("手册/安装").expect("认得出") else {
        panic!("该给一个地址");
    };
    assert_eq!(link.url, "../../手册/安装/");
}

/// `[[入门]]` 这种只写文件名的：全站只有一个时认，多个时**不猜**
#[test]
fn a_bare_file_name_works_only_when_it_is_unambiguous() {
    let (_dir, site) = site_of(&["指南/入门.mdpp", "手册/入门.mdpp"]);
    let site = Arc::new(site);
    let missing = Arc::new(Mutex::new(Vec::new()));

    let home = Page {
        source: PathBuf::from("index.mdpp"),
        key: "index".to_string(),
        output: output_of("index"),
    };
    let links = Links::for_page(site, &home, missing.clone());
    assert!(links.resolve("入门").is_none(), "重名时不该替作者挑一个");

    let report = missing.lock().expect("锁得住").clone();
    assert_eq!(report.len(), 1);
    assert!(report[0].why.contains("好几页"), "{report:?}");
    assert!(report[0].why.contains("手册/入门"), "{report:?}");
}

/// 只写文件名的那条要能落回**唯一**的那一页
#[test]
fn a_bare_file_name_finds_the_only_page_with_that_name() {
    let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp"]);
    let site = Arc::new(site);
    let missing = Arc::new(Mutex::new(Vec::new()));
    let page = site.by_key("index").expect("有首页").clone();

    let ResolvedLink::Link(link) = Links::for_page(site, &page, missing.clone())
        .resolve("入门")
        .expect("认得出")
    else {
        panic!("该给一个地址");
    };
    assert_eq!(link.url, "指南/入门/");
    assert_eq!(
        link.attrs,
        vec![("data-page".to_string(), "指南/入门".to_string())]
    );
}

/// 模板页不是站内页面 —— 落成红链，但**不进报告**
#[test]
fn template_targets_stay_out_of_the_report() {
    let (_dir, site) = site_of(&["index.mdpp"]);
    let site = Arc::new(site);
    let missing = Arc::new(Mutex::new(Vec::new()));
    let page = site.pages()[0].clone();
    let links = Links::for_page(site, &page, missing.clone());

    assert!(links.resolve("template:卡片").is_none());
    assert!(missing.lock().expect("锁得住").is_empty());
}

/// 同一页里同一个目标写两次只报一次 —— 报告里全是重复行的话，真正的漏网之鱼就看不见了
#[test]
fn the_same_target_twice_is_reported_once() {
    let (_dir, site) = site_of(&["index.mdpp"]);
    let site = Arc::new(site);
    let missing = Arc::new(Mutex::new(Vec::new()));
    let page = site.pages()[0].clone();
    let links = Links::for_page(site, &page, missing.clone());

    links.resolve("没有这一页");
    links.resolve("没有这一页");
    assert_eq!(missing.lock().expect("锁得住").len(), 1);
}
