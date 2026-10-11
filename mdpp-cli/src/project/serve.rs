//! `mdpp serve` —— 本地预览，改一个字浏览器就自己刷新。
//!
//! 三件事：
//!
//! 1. **出一遍**到临时目录（不是你的 `build/` —— 预览到一半的东西不该被当成产物），
//!    每一页里塞一段连着长连接的脚本（[`crate::http::LIVE_RELOAD`]）；
//! 2. **盯着** `pages/`、`templates/`、`mdpp.toml`，外加配置里指的那几个文件
//!    （`extra_css` / `extra_js` / `favicon` —— 它们每次出站点都要读）；
//! 3. **一有改动就重出**，重出好了把那把计数加一，浏览器那边自己刷新。
//!
//! ## 为什么盯着是「轮询」而不是 inotify
//!
//! inotify 要多一棵依赖树（`notify` → inotify/mio/crossbeam…），而一个文档站点的
//! 文件树小到可以每半秒扫一遍。少一棵依赖树，换来的是**到处都能跑**（macOS、
//! Windows、容器里的 bind mount、网盘同步目录 —— 那些地方 inotify 经常不灵）。
//!
//! 代价是改动到刷新之间最多差半秒。对「改一个字看一眼」来说，那看不出来。

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::http;
use crate::options::{Build, Serve};
use crate::project::build;
use crate::project::{Project, config, shown, under};

use tracing::{debug, info};

/// 多久看一次有没有变
const POLL: Duration = Duration::from_millis(500);

/// 编辑器存一次文件常常是「先清空再写」，两次写之间只隔几毫秒。
/// 看到改动后再等这么一会儿，等它稳下来再出 —— 否则页面会闪一次半成品
const SETTLE: Duration = Duration::from_millis(120);

pub fn run(options: &Serve) -> Result<(), String> {
    let project = Project::open()?;

    // 先绑端口：端口被占要在**起服务之前**就说，而不是站起来了才说
    let listener = TcpListener::bind((options.host.as_str(), options.port))
        .map_err(|error| format!("{}:{} 上起不来：{error}", options.host, options.port))?;
    let address = http::address_of(&listener);

    let site_dir = temp_dir()?;
    let generation = http::generation();
    let root = site_dir.clone();
    let for_server = std::sync::Arc::clone(&generation);
    std::thread::spawn(move || http::serve(listener, root, for_server));

    eprintln!("站点在 http://{address}/ （Ctrl-C 停）");
    eprintln!("出到 {}", shown(&site_dir));

    // 盯着的：正文、模板、配置，以及配置里指的那几个文件
    let mut watch = Watch::new(watched(&project));
    let options = Build {
        clean: true,
        strict: false,
    };

    // 先出一遍，让人马上能打开看
    rebuild(&project, &options, &site_dir, &generation);

    loop {
        // 等一个改动；等到了先让它稳下来，再收走
        watch.wait(POLL);
        watch.wait(SETTLE);
        let Some(changed) = watch.diff() else {
            continue;
        };
        eprintln!(
            "改动了 {} —— 重出",
            changed
                .iter()
                .map(|one| one.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        );
        rebuild(&project, &options, &site_dir, &generation);
    }
}

/// 出一遍，并通知浏览器
fn rebuild(project: &Project, options: &Build, site_dir: &Path, generation: &http::Generation) {
    match build::build_into(project, options, site_dir) {
        Ok(built) => {
            build::report(&built);
            inject(site_dir);
            http::bump(generation);
            info!(pages = built.pages, "重出好了");
        }
        Err(message) => {
            // 出不来就**留在旧页面上**：浏览器那头还连着，下一次好了会自己刷新
            eprintln!("这一遍出不来：{message}");
        }
    }
}

/// 往每个页面里塞那段连长连接的脚本
///
/// 找不到 `</body>` 就补在末尾 —— 那说明模板被换过了，而**照样要能用**才是要紧的
fn inject(site_dir: &Path) {
    let tag = format!("<script>{}</script>\n", http::LIVE_RELOAD);
    for file in pages_of(site_dir) {
        let Ok(html) = std::fs::read_to_string(&file) else {
            continue;
        };
        if html.contains(http::EVENTS_PATH) {
            continue;
        }
        let out = match html.rfind("</body>") {
            Some(at) => {
                let mut out = String::with_capacity(html.len() + tag.len());
                out.push_str(&html[..at]);
                out.push_str(&tag);
                out.push_str(&html[at..]);
                out
            }
            None => format!("{html}\n{tag}"),
        };
        if let Err(error) = std::fs::write(&file, out) {
            debug!(%error, page = %file.display(), "塞不进那段脚本");
        }
    }
}

/// 产物里的那些页面（`<键>/index.html` 与根上那一个）
fn pages_of(site_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(site_dir) else {
        return out;
    };
    let mut queue: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    queue.sort();
    while let Some(path) = queue.pop() {
        if path.is_file() {
            if path.extension().is_some_and(|one| one == "html") {
                out.push(path);
            }
            continue;
        }
        // `assets/` 里不会有页面，不必往下走
        if path
            .file_name()
            .is_some_and(|one| one == crate::page::ASSET_DIR)
        {
            continue;
        }
        if let Ok(children) = std::fs::read_dir(&path) {
            queue.extend(children.filter_map(Result::ok).map(|entry| entry.path()));
        }
    }
    out
}

/// 这一次预览临时用的目录
///
/// **不放项目的 `build/`**：预览到一半的东西不该被当成产物，而 `--clean` 一清
/// 就把上一次好好的那一遍弄没了。进程被杀后它会留在 `/tmp`，下次起的时候清掉。
fn temp_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("mdpp-预览-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|error| format!("清不了 {}：{error}", dir.display()))?;
    }
    crate::project::ensure_dir(&dir)?;
    Ok(dir)
}

/// 要盯着的那几条路径
fn watched(project: &Project) -> Vec<PathBuf> {
    let mut paths = vec![
        project.pages_dir(),
        project.templates_dir(),
        project.root.join(config::FILE_NAME),
    ];
    for file in project
        .config
        .extra_css
        .iter()
        .chain(&project.config.extra_js)
        .chain(&project.config.favicon)
    {
        paths.push(under(&project.root, file));
    }
    paths
}

/// 盯着几条路径：手上一张「文件 → 大小与改动时间」的照，和刚扫的那张比
struct Watch {
    paths: Vec<PathBuf>,
    seen: BTreeMap<PathBuf, (u64, SystemTime)>,
}

impl Watch {
    fn new(paths: Vec<PathBuf>) -> Self {
        let seen = http::collect(&paths);
        Watch { paths, seen }
    }

    /// 等一次改动（没等到就等满 `limit`）。
    ///
    /// 这一层**只看不动**那张照 —— 动了的话 `diff` 就说不出「改了什么」。
    /// 分片睡而不是一次睡到底：`Ctrl-C` 是默认的退出方式，而睡死了别的线程也喊不动它
    fn wait(&self, limit: Duration) {
        let step = Duration::from_millis(50);
        let mut waited = Duration::ZERO;
        while waited < limit {
            std::thread::sleep(step.min(limit - waited));
            waited += step;
            if http::collect(&self.paths) != self.seen {
                return;
            }
        }
    }

    /// 扫一遍，把这一轮照收下，返回改了 / 增了 / 删了的那几个
    ///
    /// **删掉的也要算一次改动**：少了一页是改动，而只比「现在有的」是发现不了它的
    fn diff(&mut self) -> Option<Vec<PathBuf>> {
        let now = http::collect(&self.paths);
        let mut changed: Vec<PathBuf> = now
            .iter()
            .filter(|(path, mark)| self.seen.get(*path) != Some(*mark))
            .map(|(path, _)| path.clone())
            .collect();
        changed.extend(
            self.seen
                .keys()
                .filter(|path| !now.contains_key(*path))
                .cloned(),
        );
        changed.sort();
        self.seen = now;
        (!changed.is_empty()).then_some(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use std::sync::atomic::Ordering;

    /// 一份项目（`init` 建出来的骨架）
    fn scaffold() -> (tempfile::TempDir, Project) {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let root = dir.path().join("站点");
        crate::project::init::run(&crate::options::Init {
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
        }
    }

    /// 一遍出站点，计数加一 —— 浏览器那边就是靠这个数知道要刷新
    #[test]
    fn a_rebuild_bumps_the_counter_so_browsers_reload() {
        let (_dir, project) = scaffold();
        let generation = http::generation();
        let site_dir = project.site_dir();

        rebuild(&project, &options(), &site_dir, &generation);
        let first = generation.load(Ordering::Relaxed);
        assert!(first > 0, "出完一趟该加一次");

        rebuild(&project, &options(), &site_dir, &generation);
        assert!(
            generation.load(Ordering::Relaxed) > first,
            "第二趟又加了一次"
        );
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
            },
            &project.site_dir(),
        )
        .expect("出得来");

        let built = std::fs::read_to_string(project.site_dir().join("index.html")).expect("读得动");
        assert!(
            !built.contains("__mdpp"),
            "build 的产物不该带那段脚本：{built}"
        );

        let preview = project.root.join("预览");
        rebuild(&project, &options(), &preview, &http::generation());
        let served = std::fs::read_to_string(preview.join("index.html")).expect("读得动");
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

        let preview = project.root.join("预览");
        rebuild(&project, &options(), &preview, &http::generation());

        let pages = pages_of(&preview);
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
            "site_name = \"甲\"\nextra_css = [\"我的.css\"]\nfavicon = \"assets/icon.svg\"\n",
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
}
