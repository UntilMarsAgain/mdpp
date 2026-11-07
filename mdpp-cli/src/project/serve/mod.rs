//! `mdpp serve` —— 本地预览，改一个字浏览器就自己刷新。
//!
//! 三件事：
//!
//! 1. **出一遍**到临时目录（不是你的 `build/` —— 预览到一半的东西不该被当成产物），
//!    每一页里塞一段连着长连接的脚本（[`crate::server::http::LIVE_RELOAD`]）；
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

mod rebuild;
mod temp;
mod watch;

#[cfg(test)]
mod tests;

use std::net::TcpListener;
use std::time::Duration;

use crate::cli::options::{Build, Serve};
use crate::project::{Project, shown};
use crate::server::http;
use tracing::debug;

use rebuild::{rebuild, turn_dir};
use temp::cache_dir;
use watch::{Watch, watched};

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
    if !matches!(options.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        eprintln!(
            "警告：serve 是本地预览服务器，没有认证与访问控制；\
             绑在 {} 上会让同一网络里的人读到整个站点",
            options.host
        );
    }

    // 缓存目录：每一遍出在**自己的子目录**里（`turn-N`），出齐了才把指针挪过去。
    // 于是「清空产物目录」删的永远是没人读的那个 —— 改到一半刷新也不会撞见半截的
    // 页面或凭空消失的 CSS
    let cache = cache_dir()?;
    let root = http::Root::new(cache.join(turn_dir(0)));
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
        external_script: String::from("cdn"),
        jobs: 0,
        // 预览出到系统临时目录，不在项目里 —— 这个开关只在这里为真
        allow_outside: true,
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
