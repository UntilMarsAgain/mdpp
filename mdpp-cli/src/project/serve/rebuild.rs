use std::path::{Path, PathBuf};

use crate::cli::options::Build;
use crate::project::{Project, build};
use crate::server::http;
use tracing::{debug, info};

/// 出一遍，并通知浏览器
///
/// **出在 `cache/turn-N` 里，全部写完才把服务的指针挪过去** —— 所以清理与写入都
/// 发生在没人读的地方。挪过去之后旧的那一遍还留着（可能有连接正读着它），
/// 再下一遍才顺手清掉。
pub(super) fn rebuild(
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

/// 第 `turn` 遍出在哪个子目录。只用 ASCII：路径名不参与展示，
/// 而 `std::env::temp_dir()` 在非 UTF-8 locale 下对非 ASCII 名的解释并不可靠。
pub(super) fn turn_dir(turn: usize) -> String {
    format!("turn-{turn}")
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
pub(super) fn pages_of(site_dir: &Path) -> Vec<PathBuf> {
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
            .is_some_and(|one| one == crate::render::page::ASSET_DIR)
        {
            continue;
        }
        if let Ok(children) = std::fs::read_dir(&path) {
            queue.extend(children.filter_map(Result::ok).map(|entry| entry.path()));
        }
    }
    out
}
