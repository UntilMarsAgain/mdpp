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
/// 看到改动后再扫几眼，**连着这么久没再变**才出 —— 否则页面会闪一次半成品
const SETTLE: Duration = Duration::from_millis(120);

pub fn run(options: &Serve) -> Result<(), String> {
    let project = Project::open()?;

    // 先绑端口：端口被占要在**起服务之前**就说，而不是站起来了才说
    let listener = TcpListener::bind((options.host.as_str(), options.port))
        .map_err(|error| format!("{}:{} 上起不来：{error}", options.host, options.port))?;
    let address = http::address_of(&listener);

    // 缓存目录：每一遍出在**自己的子目录**里（`第 N 遍`），出齐了才把指针挪过去。
    // 于是「清空产物目录」删的永远是没人读的那个 —— 改到一半刷新也不会撞见半截的
    // 页面或凭空消失的 CSS
    let cache = cache_dir()?;
    let root = http::Root::new(cache.join("第 0 遍"));
    let generation = http::generation();
    let for_server = std::sync::Arc::clone(&generation);
    let for_root = root.clone();
    std::thread::spawn(move || http::serve(listener, for_root, for_server));

    eprintln!("站点在 http://{address}/ （Ctrl-C 停）");
    eprintln!("出到 {}", shown(&cache));

    // 盯着的：正文、模板、配置，以及配置里指的那几个文件
    let mut watch = Watch::new(watched(&project));
    let options = Build {
        clean: true,
        strict: false,
    };

    // 先出一遍，让人马上能打开看
    let mut turn = 0usize;
    rebuild(&project, &options, &cache, &root, &generation, turn);

    loop {
        // 等一个改动；等到了先让它稳下来，再收走
        watch.wait(POLL);
        if !watch.settle(SETTLE) {
            // 一直有人在写：不赖在这儿等，先出一趟，下一轮眼睛还盯着
            debug!("等满 {} 毫秒还没稳下来，先出一趟", SETTLE.as_millis());
        }
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
        turn += 1;
        rebuild(&project, &options, &cache, &root, &generation, turn);
    }
}

/// 出一遍，并通知浏览器
///
/// **出在 `cache/第 N 遍` 里，全部写完才把服务的指针挪过去** —— 所以清理与写入都
/// 发生在没人读的地方。挪过去之后旧的那一遍还留着（可能有连接正读着它），
/// 再下一遍才顺手清掉。
fn rebuild(
    project: &Project,
    options: &Build,
    cache: &Path,
    root: &http::Root,
    generation: &http::Generation,
    turn: usize,
) {
    let into = cache.join(turn_dir(turn));
    // 塞那段刷新脚本是**这一遍成功之后**的事：出不来的那一遍连产物都不是，
    // 更不该往里面塞东西（那样下一遍 `clean` 会连它一起清掉，白费）
    let result = match build::build_into(project, options, &into) {
        Ok(built) => {
            inject(&into);
            Ok(built)
        }
        Err(message) => Err(message),
    };
    match result {
        Ok(built) => {
            build::report(&built);
            // **挪指针在一切写完之后**：这一条之前，请求读到的还是完整的一遍旧的
            root.point_at(&into);
            http::bump(generation);
            info!(pages = built.pages, "重出好了");
            // 再往前那一遍（两遍之前的）没人要了 —— 它可能还被人打开着，
            // 但那是个已经关掉的响应，删了不影响
            if turn >= 2 {
                let _ = std::fs::remove_dir_all(cache.join(turn_dir(turn - 2)));
            }
        }
        Err(message) => {
            // 出不来就**留在旧页面上**：浏览器那头还连着，下一次好了会自己刷新。
            // 这一遍半截的产物**不挪指针**（那才是「留在旧页面上」的实现方式），
            // 下一遍会出在另一个目录里，它自己就没人管了
            eprintln!("这一遍出不来：{message}");
        }
    }
}

/// 第 `turn` 遍出在哪个子目录。名字里带中文是为了好认（那是给人看的），
/// 而它是**我们自己拼的**路径，不经过系统那一层编码，没有上面说的那个风险
fn turn_dir(turn: usize) -> String {
    format!("第 {turn} 遍")
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
/// 就把上一次好好的那一遍弄没了。
///
/// 目录名带 pid，于是几次预览（包括两个终端同时开）互不相干。也因为如此，**进程被
/// 杀掉时它会留在临时目录里**：下一回起的是另一个 pid，撞不上；下面那段 `exists` 清的
/// 只是 pid 号恰好被系统复用回来的旧目录。想清干净自己 `rm -rf` 也行。
///
/// **名字里只有 ASCII** —— `std::env::temp_dir()` 给的那条路径在某些环境（老 Windows、
/// 非 UTF-8 的 locale、带转义的挂载点）会被系统按自己的编码重新解释，夹在中间的
/// 非 ASCII 字符是最容易在那一步变成乱码、于是 `exists` 与 `remove_dir_all` 指不到
/// 同一个目录的东西。这个目录名没有任何给人看的价值，`mdpp-preview-` 已经说清了它是谁的
fn temp_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(preview_name());
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|error| format!("清不了 {}：{error}", dir.display()))?;
    }
    crate::project::ensure_dir(&dir)?;
    Ok(dir)
}

/// 这一次预览的临时目录叫什么
///
/// **名字里只有 ASCII** —— `std::env::temp_dir()` 给的那条路径在某些环境（老 Windows、
/// 非 UTF-8 的 locale、带转义的挂载点）会被系统按自己的编码重新解释，夹在中间的
/// 非 ASCII 字符是最容易在那一步变成乱码、于是 `exists` 与 `remove_dir_all` 指不到
/// 同一个目录的东西。这个目录名没有任何给人看的价值，`mdpp-preview-` 已经说清了它是谁的
fn preview_name() -> String {
    format!("mdpp-preview-{}", std::process::id())
}

/// 预览的缓存目录：临时目录下面再垫一层，每一遍出在它自己的子目录里
///
/// **垫这一层是为了换根** —— `http` 那边拿的是一个可以改指向的 [`http::Root`]，
/// 而它指的是这一层里的某个子目录。一遍出完，指针挪到那个子目录上。
fn cache_dir() -> Result<PathBuf, String> {
    let cache = temp_dir()?.join("缓存");
    crate::project::ensure_dir(&cache)?;
    Ok(cache)
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

    /// 等它**稳下来**：从现在起连着 `limit` 毫秒没有新变化，才返回。
    ///
    /// 与 [`Watch::wait`] 比的不是同一张照：`wait` 问的是「与上次收下的那份还一样吗」，
    /// 而改动已经发生了，回不去 —— 拿它当判据会变成「看见改动立刻返回」，防抖就没了。
    /// 这里比的是**上一眼**：连着几眼都一样，才算写完了。
    ///
    /// 也不能一直等下去：真有人（或某个进程）一直在写，得有个头 —— 等满
    /// `limit × 4` 还不稳就先出，下一轮眼睛还盯着。返回值说这一次是不是真等稳了。
    ///
    /// **只看不动 `seen`**：照还得留给 [`Watch::diff`] 去收，它要拿旧照比出「改了哪些」
    fn settle(&self, limit: Duration) -> bool {
        let step = Duration::from_millis(50);
        let cap = limit.saturating_mul(4);
        let mut latest = http::collect(&self.paths);
        let mut quiet = Duration::ZERO;
        let mut waited = Duration::ZERO;

        while waited < cap {
            let nap = step.min(limit - quiet);
            std::thread::sleep(nap);
            waited += nap;

            let now = http::collect(&self.paths);
            if now == latest {
                quiet += nap;
                if quiet >= limit {
                    return true;
                }
            } else {
                latest = now;
                quiet = Duration::ZERO;
            }
        }
        // 等到超时还没稳：不赖在这儿了，先出，下一轮再看
        false
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
        assert!(started.elapsed() < Duration::from_secs(2), "等太久了");
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

        let preview = project.root.join("预览");
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
}
