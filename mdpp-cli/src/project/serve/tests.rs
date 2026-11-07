use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::project::Project;

use super::rebuild::pages_of;
use super::temp::{preview_name, temp_dir};

/// 一份项目（`init` 建出来的骨架）
fn scaffold() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let root = dir.path().join("站点");
    crate::project::init::run(&crate::cli::options::Init {
        dir: Some(root.clone()),
        name: Some("测试站点".to_string()),
    })
    .expect("建得出");
    (dir, Project::load(&root).expect("这是个项目"))
}

fn options() -> Build {
    Build {
        clean: true,
        strict: false,
        external_script: String::from("cdn"),
        jobs: 0,
        allow_outside: true,
    }
}

/// 一遍出站点，计数加一 —— 浏览器那边就是靠这个数知道要刷新
#[test]
fn a_rebuild_bumps_the_counter_so_browsers_reload() {
    let (_dir, project) = scaffold();
    let generation = http::generation();
    let cache = project.site_dir();
    let root = http::Root::new(cache.join(turn_dir(0)));

    rebuild(&project, &options(), &cache, &root, &generation, 0);
    let first = generation.load(Ordering::Relaxed);
    assert!(first > 0, "出完一趟该加一次");

    rebuild(&project, &options(), &cache, &root, &generation, 1);
    assert!(
        generation.load(Ordering::Relaxed) > first,
        "第二趟又加了一次"
    );
}

/// **换根而不是清空**：每一遍出在自己的子目录里，指针在写完之后才挪。
///
/// 这条钉的是一个真 bug：以前每一遍都 `remove_dir_all` 正在被服务的那个目录，
/// 而 HTTP 线程随时可能正在 `File::open` 它 —— 那个窗口里请求要么拿到
/// `ENOENT`（CSS 突然没了），要么拿到半截的 HTML
#[test]
fn the_directory_being_served_is_never_wiped_out_from_under_readers() {
    let (_dir, project) = scaffold();
    let generation = http::generation();
    let cache = project.site_dir();
    let root = http::Root::new(cache.join(turn_dir(0)));

    rebuild(&project, &options(), &cache, &root, &generation, 0);
    let serving = root.serving();
    assert!(
        serving.join("index.html").is_file(),
        "头一遍该出得来：{}",
        serving.display()
    );

    // 第二遍：**正在被服务的那个目录一个字节都不许动**
    let before: Vec<_> = walk(&serving)
        .into_iter()
        .map(|one| (one.0, one.1))
        .collect();
    rebuild(&project, &options(), &cache, &root, &generation, 1);
    let after: Vec<_> = walk(&serving)
        .into_iter()
        .map(|one| (one.0, one.1))
        .collect();
    assert_eq!(before, after, "旧的那一遍被动了 —— 它可能正被人读着");

    // 指针挪到新的一遍上，而新的一遍是完整的
    let moved = root.serving();
    assert_ne!(moved, serving, "指针该挪到新的一遍上");
    assert!(
        moved.join("index.html").is_file(),
        "新的一遍该是完整的：{}",
        moved.display()
    );
}

/// 走一遍的资源清单（相对路径 + 内容），用来比较「有没有被动过」
fn walk(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut queue = vec![dir.to_path_buf()];
    while let Some(at) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path.strip_prefix(dir).unwrap_or(&path).to_path_buf(), bytes));
            }
        }
    }
    out.sort();
    out
}

/// 防抖真的在等：没人再写了，也要**等满整个窗口**才说稳。
///
/// 以前这里走的是 `wait` —— 它看见改动立刻返回，于是 `SETTLE` 那一趟只睡了一跳
/// 就回来了，承诺的「等它稳下来」根本没发生。这条钉的就是那一下
#[test]
fn settle_waits_out_its_whole_window_before_reporting_stable() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let path = dir.path().join("页.md");
    std::fs::write(&path, "# 一").expect("写得下");
    let watch = Watch::new(vec![path]);

    let started = std::time::Instant::now();
    let stable = watch.settle(SETTLE);
    assert!(stable, "没人再写，该是稳的");
    assert!(
        started.elapsed() >= SETTLE,
        "只等了 {:?}，整个窗口都没走完",
        started.elapsed()
    );
}

/// 一直有人在写也得有个头：等满上限就先出，不能挂在这儿
#[test]
fn settle_gives_up_instead_of_waiting_forever() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let path = dir.path().join("页.md");
    std::fs::write(&path, "# 一").expect("写得下");
    let watch = Watch::new(vec![path.clone()]);

    // 每 20 毫秒写一次，而且**每次长度都不一样** —— 靠大小就认得出它变了，
    // 不必去赌文件系统的时间戳精度
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer = {
        let stop = std::sync::Arc::clone(&stop);
        let path = path.clone();
        std::thread::spawn(move || {
            let mut size = 1usize;
            while !stop.load(Ordering::Relaxed) {
                size += 1;
                let _ = std::fs::write(&path, "改".repeat(size));
                std::thread::sleep(Duration::from_millis(20));
            }
        })
    };

    let started = std::time::Instant::now();
    let stable = watch.settle(SETTLE);
    stop.store(true, Ordering::Relaxed);
    writer.join().expect("写手停得下");

    assert!(!stable, "一直在写，不该报稳");
    // 上限是 `limit × 4`：挂住的话这条会等到天荒地老
    assert!(started.elapsed() < Duration::from_secs(5), "等太久了");
}

/// 出的页面里要真有那段「变了就刷新」的脚本，而**普通的 build 不带**
#[test]
fn the_reload_script_shows_up_only_under_serve() {
    let (_dir, project) = scaffold();
    // 先正经 build 一次，才有「不带那段脚本的产物」可比
    crate::project::build::build_into(
        &project,
        &Build {
            clean: true,
            strict: false,
            external_script: String::from("cdn"),
            jobs: 0,
            allow_outside: false,
        },
        &project.site_dir(),
    )
    .expect("出得来");

    let built = std::fs::read_to_string(project.site_dir().join("index.html")).expect("读得动");
    assert!(
        !built.contains("__mdpp"),
        "build 的产物不该带那段脚本：{built}"
    );

    let preview = project.root.join("preview");
    let root = http::Root::new(preview.join(turn_dir(0)));
    rebuild(
        &project,
        &options(),
        &preview,
        &root,
        &http::generation(),
        0,
    );
    let served =
        std::fs::read_to_string(preview.join(turn_dir(0)).join("index.html")).expect("读得动");
    assert!(
        served.contains("__mdpp/events"),
        "预览该连着长连接：{served}"
    );
}

/// 每一页都要有 —— 深层页里的那段也得在
#[test]
fn every_page_gets_the_reload_script() {
    let (_dir, project) = scaffold();
    let deep = project.pages_dir().join("指南");
    std::fs::create_dir_all(&deep).expect("建得出");
    std::fs::write(deep.join("安装.mdpp"), "# 安装\n").expect("写得下");

    let preview = project.root.join("preview");
    let root = http::Root::new(preview.join(turn_dir(0)));
    rebuild(
        &project,
        &options(),
        &preview,
        &root,
        &http::generation(),
        0,
    );

    let pages = pages_of(&preview.join(turn_dir(0)));
    assert!(pages.len() >= 3, "{pages:?}");
    for page in pages {
        let html = std::fs::read_to_string(&page).expect("读得动");
        assert!(
            html.contains("__mdpp/events"),
            "{} 没带那段脚本",
            page.display()
        );
    }
}

/// 盯着的路径要盖住正文、模板、配置，外加配置里指的那几个文件
/// （`extra_css` / `extra_js` / `favicon` —— 它们每次出站点都要读）
#[test]
fn the_watched_paths_cover_everything_a_build_reads() {
    let (_dir, project) = scaffold();
    std::fs::write(project.root.join("我的.css"), "/* 我的 */").expect("写得下");
    std::fs::create_dir_all(project.root.join("assets")).expect("建得出");
    std::fs::write(project.root.join("assets/icon.svg"), "<svg/>").expect("写得下");
    std::fs::write(
            project.root.join("mdpp.toml"),
            "site_name = \"甲\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\nextra_css = [\"我的.css\"]\nfavicon = \"assets/icon.svg\"\n",
        )
        .expect("改得下");

    let project = Project::load(&project.root).expect("读得动");
    let listed: Vec<String> = watched(&project)
        .iter()
        .map(|one| one.display().to_string())
        .collect();
    for expected in [
        "pages",
        "templates",
        "mdpp.toml",
        "我的.css",
        "assets/icon.svg",
    ] {
        assert!(
            listed.iter().any(|one| one.ends_with(expected)),
            "没盯着 {expected}：{listed:?}"
        );
    }
}

/// 改一个字、新加一个、删掉一个，都要被认出来
#[test]
fn a_changed_added_or_removed_file_is_all_noticed() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let pages = dir.path().join("pages");
    std::fs::create_dir_all(&pages).expect("建得出");

    let mut watch = Watch::new(vec![pages.clone()]);
    assert!(watch.diff().is_none(), "一开始没有改动");

    let page = pages.join("甲.mdpp");
    std::fs::write(&page, "一").expect("写得下");
    assert!(watch.diff().is_some(), "新增没被认出来");

    std::fs::write(&page, "改过了").expect("写得下");
    assert!(watch.diff().is_some(), "改动没被认出来");

    std::fs::remove_file(&page).expect("删得掉");
    assert!(watch.diff().is_some(), "删除没被认出来");
    assert!(watch.diff().is_none(), "认过一次就该记下了");
}

/// 认出来的是**哪些**文件 —— 报给作者看，他才知道工具看见了什么
#[test]
fn the_changed_files_are_named() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    let pages = dir.path().join("pages");
    std::fs::create_dir_all(&pages).expect("建得出");
    let mut watch = Watch::new(vec![pages.clone()]);

    std::fs::write(pages.join("甲.mdpp"), "一").expect("写得下");
    std::fs::write(pages.join("乙.mdpp"), "二").expect("写得下");

    let changed = watch.diff().expect("有改动");
    assert_eq!(changed.len(), 2, "{changed:?}");
    assert!(
        changed
            .iter()
            .all(|one| one.to_string_lossy().ends_with(".mdpp")),
        "{changed:?}"
    );
}

/// 预览的临时目录每次是新的 —— 上一回留下的会被清掉
#[test]
fn the_preview_directory_is_fresh_every_time() {
    let dir = tempfile::tempdir().expect("造得出临时目录");
    std::env::set_current_dir(dir.path()).expect("切得进去");
    // temp_dir 用的是进程号，所以这个用例只能验「不报错 + 是空目录」
    let preview = temp_dir().expect("建得出");
    assert!(preview.is_dir());
    assert!(
        std::fs::read_dir(&preview)
            .expect("读得动")
            .next()
            .is_none(),
        "上一回留下的东西该被清掉"
    );
    let again = temp_dir().expect("建得出");
    assert_eq!(preview, again);
}

/// 目录名里**只有 ASCII** —— `std::env::temp_dir()` 给的那条路径会被系统按自己的
/// 编码重新解释一遍，夹在中间的非 ASCII 字符最容易在那一步变成乱码，于是
/// `exists` 与 `remove_dir_all` 指不到同一个目录
#[test]
fn the_preview_directory_name_is_pure_ascii() {
    let name = preview_name();
    assert!(name.is_ascii(), "目录名不该有非 ASCII 字符：{name}");
    assert!(name.starts_with("mdpp-preview-"), "{name}");
    assert!(
        name.ends_with(&std::process::id().to_string()),
        "名字该带进程号（两次预览互不相干）：{name}"
    );
}

/// 每一遍的目录名也只用 ASCII：它同样会落到文件系统上
#[test]
fn the_turn_directory_name_is_pure_ascii() {
    for turn in [0, 1, 7, 42] {
        let name = turn_dir(turn);
        assert!(name.is_ascii(), "目录名不该有非 ASCII 字符：{name}");
        assert!(name.starts_with("turn-"), "{name}");
    }
}
