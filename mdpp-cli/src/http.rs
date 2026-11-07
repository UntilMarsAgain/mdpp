//! 本地预览服务器。
//!
//! HTTP 解析、静态文件（MIME、range、HEAD、目录索引、`..` 防护、chunked）交给
//! [`axum`] + [`tower_http`]；这里只保留三件本库自己的事：
//!
//! 1. **产物根可以换**：[`Root`] 里是一个指针，预览重建时先出到新目录，全部写完
//!    才挪指针。请求要么看到完整的一遍旧产物、要么看到完整的一遍新产物。
//! 2. **实时刷新**：一条 SSE 长连接，重建完成把 [`Generation`] 加一，页面脚本收到
//!    `message` 就 `location.reload()`。
//! 3. **盯文件**：[`collect`] 给 `serve` 的轮询循环提供「文件 → 大小/改动时间」。
//!
//! 手写那一版在请求行长度、chunked、超时与路径上都出过问题，所以 HTTP 这层不再
//! 自己实现。

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::Stream;
use tower::ServiceExt;
use tower_http::services::ServeDir;
use tracing::debug;

/// 正在被服务的那一份产物
///
/// 换的是**指针**，不是目录里的内容：重建时写进另一个目录，全部写完了才把指针挪过去。
/// 于是每个请求读到的要么是完整的一遍旧产物、要么是完整的一遍新产物，**中间没有
/// 「正被 `remove_dir_all`、正写一半」的窗口**。
#[derive(Debug, Clone)]
pub struct Root {
    dir: Arc<RwLock<PathBuf>>,
}

impl Root {
    /// 指着一个还没有产物的目录 —— 那时的请求该是「没有这一页」，不是「空文件」
    pub fn new(dir: PathBuf) -> Self {
        Root {
            dir: Arc::new(RwLock::new(dir)),
        }
    }

    /// 这一遍服务的是哪个目录。**每个请求取一次**，不是建一次就定死
    pub fn serving(&self) -> PathBuf {
        match self.dir.read() {
            Ok(dir) => dir.clone(),
            // 锁中毒只发生在上一个持锁的线程 panic 的时候 —— 那时里面的值仍然是
            // 好值，预览该继续能用（崩的那一次早就报过错了）
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// 换到新的一遍。**在全部内容写完之后才调**
    pub fn point_at(&self, dir: &Path) {
        match self.dir.write() {
            Ok(mut current) => *current = dir.to_path_buf(),
            Err(poisoned) => *poisoned.into_inner() = dir.to_path_buf(),
        }
    }
}

/// 那条长连接。页面脚本连的是它，服务器靠它通知「重出好了」
pub const EVENTS_PATH: &str = "/__mdpp/events";

/// 塞进每个页面里的那一小段。**只在 `serve` 出的页面里有**
///
/// 挂的是 `message` 而不是 `reload` —— 服务器发的是 `data: reload`，
/// 在 SSE 里那是「一条 `message` 事件，载荷是 `reload`」，**不是**一个名叫 `reload`
/// 的事件。挂错名字的那一版**哪个浏览器都不触发**，而 `EventSource` 对「你监听了
/// 我没发的事件」**不报任何错**。
pub const LIVE_RELOAD: &str = concat!(
    "new EventSource(\"",
    "/__mdpp/events",
    "\")",
    ".addEventListener(\"message\", () => location.reload());",
);

/// 重出的次数。浏览器那边盯着它变没变
pub type Generation = Arc<AtomicU64>;

/// 新的一份计数
pub fn generation() -> Generation {
    Arc::new(AtomicU64::new(0))
}

/// 加一次 —— 「重出好了，刷新吧」
pub fn bump(generation: &Generation) {
    generation.fetch_add(1, Ordering::Relaxed);
}

/// 路由之间共享的那两样
#[derive(Clone)]
struct AppState {
    root: Root,
    generation: Generation,
}

/// 收连接，一直收到关掉为止。调用方通常在独立线程里跑它。
pub fn serve(listener: std::net::TcpListener, root: Root, generation: Generation) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            debug!(%error, "建不了预览服务器的运行时");
            return;
        }
    };

    runtime.block_on(async move {
        if let Err(error) = listener.set_nonblocking(true) {
            debug!(%error, "预览服务器切不了非阻塞");
            return;
        }
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                debug!(%error, "预览服务器接不上端口");
                return;
            }
        };
        let app = router(root, generation);
        if let Err(error) = axum::serve(listener, app).await {
            debug!(%error, "预览服务器停了");
        }
    });
}

/// 报一句「实际在哪个地址上」—— 端口写 0 时是内核给的，得问它要
pub fn address_of(listener: &std::net::TcpListener) -> String {
    listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "?".to_string())
}

fn router(root: Root, generation: Generation) -> Router {
    let state = AppState { root, generation };
    Router::new()
        .route(EVENTS_PATH, get(events))
        .fallback(static_files)
        .with_state(state)
}

/// SSE：每 300ms 看一眼计数，变了就发一条 `data: reload`。
///
/// `Event::default().data("reload")` 在浏览器里是 **`message`** 事件，载荷是
/// `reload` —— 与 [`LIVE_RELOAD`] 里挂的名字一一对应。
async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let generation = state.generation;
    let stream = futures_util::stream::unfold(generation, |generation| async move {
        let seen = generation.load(Ordering::Relaxed);
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if generation.load(Ordering::Relaxed) != seen {
                break;
            }
        }
        Some((Ok(Event::default().data("reload")), generation))
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// 静态文件：`ServeDir` 负责目录索引、`/x` → `/x/` 的跳转、range、HEAD、MIME
/// 与 `..` 防护。根每次请求取一次，于是换根与请求之间没有窗口。
async fn static_files(State(state): State<AppState>, request: Request) -> Response {
    let root = state.root.serving();
    let service = ServeDir::new(root);
    match service.oneshot(request).await {
        Ok(response) if response.status() == StatusCode::NOT_FOUND => not_found(),
        // `ServeDir` 的响应体是它自己的 `ResponseBody`，换成 axum 的 `Body`
        // 才能直接作为 handler 的返回值
        Ok(response) => response.map(Body::new),
        Err(infallible) => match infallible {},
    }
}

/// 没有这一页时的 HTML。比一行 `404 Not Found` 好读，也方便直接点回站点根。
fn not_found() -> Response {
    const BODY: &str = "<!DOCTYPE html>\n\
        <meta charset=\"utf-8\">\n\
        <title>没有这一页</title>\n\
        <style>body{font:16px/1.7 system-ui,sans-serif;margin:4rem auto;max-width:32rem;padding:0 1rem}\n\
        a{color:#2f6fd0}</style>\n\
        <h1>没有这一页</h1>\n\
        <p>地址不对，或者那一页还没建。<a href=\"/\">回站点根</a></p>\n";
    (
        StatusCode::NOT_FOUND,
        [(CONTENT_TYPE, "text/html; charset=utf-8")],
        BODY,
    )
        .into_response()
}

// ---------------------------------------------------------------- 盯文件

/// 文件的一张小照：大小与改动时间。**只比这两样** —— 内容另算，
/// 而对「改没改」这个问题它们够了
fn stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// 递归收下那些路径下的每一个文件（路径本身是文件就收它自己）
pub fn collect(paths: &[PathBuf]) -> BTreeMap<PathBuf, (u64, std::time::SystemTime)> {
    let mut out = BTreeMap::new();
    for path in paths {
        if path.is_file() {
            if let Some(mark) = stamp(path) {
                out.insert(path.clone(), mark);
            }
            continue;
        }
        walk(path, &mut out, 0);
    }
    out
}

/// 目录树有多深算封顶 —— 而正文目录不会比这更深
const MAX_DEPTH: usize = 32;

fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (u64, std::time::SystemTime)>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out, depth + 1);
        } else if let Some(mark) = stamp(&path) {
            out.insert(path, mark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 换根换的是指针：旧目录的读者不受影响，新请求从新目录取
    #[test]
    fn the_root_pointer_swaps_instead_of_wiping() {
        let first = tempfile::tempdir().expect("造得出临时目录");
        let second = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(first.path().join("index.html"), "一").expect("写得下");
        std::fs::write(second.path().join("index.html"), "二").expect("写得下");

        let root = Root::new(first.path().to_path_buf());
        assert_eq!(root.serving(), first.path());
        root.point_at(second.path());
        assert_eq!(root.serving(), second.path());
    }

    /// 页面里挂的事件名与服务器发的事件名对得上
    #[test]
    fn the_script_listens_for_message() {
        assert!(
            LIVE_RELOAD.contains(&format!("\"{EVENTS_PATH}\"")),
            "{LIVE_RELOAD}"
        );
        assert!(LIVE_RELOAD.contains("location.reload()"));
        assert!(LIVE_RELOAD.contains(r#"addEventListener("message""#));
        assert!(!LIVE_RELOAD.contains("</script"));
    }

    fn app(root: Root) -> Router {
        router(root, generation())
    }

    #[tokio::test]
    async fn static_files_are_served_with_their_mime() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("index.html"), "<h1>首页</h1>").expect("写得下");
        std::fs::write(dir.path().join("站点.css"), ".a{}").expect("写得下");

        let response = app(Root::new(dir.path().to_path_buf()))
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("请求造得出"),
            )
            .await
            .expect("服务回得来");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/html")
        );

        let css = app(Root::new(dir.path().to_path_buf()))
            .oneshot(
                Request::builder()
                    .uri("/站点.css")
                    .body(Body::empty())
                    .expect("请求造得出"),
            )
            .await
            .expect("服务回得来");
        assert_eq!(css.status(), StatusCode::OK);
        assert!(
            css.headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/css")),
            "{:?}",
            css.headers().get(CONTENT_TYPE)
        );
    }

    /// 目录地址带斜杠：`/guide` 跳到 `/guide/`，相对链接才不会指错一层
    #[tokio::test]
    async fn a_directory_without_a_slash_is_redirected() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::create_dir_all(dir.path().join("指南")).expect("建得出");
        std::fs::write(dir.path().join("指南/index.html"), "文").expect("写得下");

        let response = app(Root::new(dir.path().to_path_buf()))
            .oneshot(
                Request::builder()
                    .uri("/指南")
                    .body(Body::empty())
                    .expect("请求造得出"),
            )
            .await
            .expect("服务回得来");
        assert!(
            response.status().is_redirection(),
            "该跳转：{}",
            response.status()
        );
    }

    /// 没有的页给的是我们那张 404，而不是一行空响应
    #[tokio::test]
    async fn a_missing_page_gets_the_html_404() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let response = app(Root::new(dir.path().to_path_buf()))
            .oneshot(
                Request::builder()
                    .uri("/没有这一页")
                    .body(Body::empty())
                    .expect("请求造得出"),
            )
            .await
            .expect("服务回得来");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/html")),
            "{:?}",
            response.headers().get(CONTENT_TYPE)
        );
    }

    /// `..` 走不出产物根 —— 这是库的路径校验，不是我们的
    #[tokio::test]
    async fn parent_segments_do_not_escape_the_root() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("index.html"), "首页").expect("写得下");
        let response = app(Root::new(dir.path().to_path_buf()))
            .oneshot(
                Request::builder()
                    .uri("/../Cargo.toml")
                    .body(Body::empty())
                    .expect("请求造得出"),
            )
            .await
            .expect("服务回得来");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// 事件流本身：响应头得是 SSE，且不阻塞 handler 返回
    #[tokio::test]
    async fn the_event_stream_answers_with_sse_headers() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let response = app(Root::new(dir.path().to_path_buf()))
            .oneshot(
                Request::builder()
                    .uri(EVENTS_PATH)
                    .body(Body::empty())
                    .expect("请求造得出"),
            )
            .await
            .expect("服务回得来");
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/event-stream")),
            "{:?}",
            response.headers().get(CONTENT_TYPE)
        );
    }
}
