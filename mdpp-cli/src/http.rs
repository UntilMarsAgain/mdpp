//! 一个**只够本地预览**的静态服务器。
//!
//! 不引 HTTP 库：这一份要的全部是「按路径取一个文件」加「一条 SSE 长连接」，而这两件
//! 事在 `std::net` 上是一百来行。引一整个库进来，为的是开发时看一眼页面。
//!
//! ## 实时刷新怎么走
//!
//! [`EVENTS_PATH`] 那条 **SSE**（Server-Sent Events）长连接一直开着，页面上的
//! [`LIVE_RELOAD`] 脚本连着它；`serve` 每次重出就把 [`generation`] 加一，连接那边
//! 发现数变了就写一行 `data: reload`，浏览器随即 `location.reload()`。
//!
//! 用 SSE 而不是 WebSocket：单向就够（SSE 是普通 HTTP，将来还有代理与日志），而且
//! 浏览器自己会重连 —— 页面开着的时候服务器重启了，不用刷新。

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tracing::debug;

/// 那条长连接。页面脚本连的是它，服务器靠它通知「重出好了」
pub const EVENTS_PATH: &str = "/__mdpp/events";

/// 塞进每个页面里的那一小段。**只在 `serve` 出的页面里有**
pub const LIVE_RELOAD: &str = concat!(
    "new EventSource(\"", "/__mdpp/events", "\")",
    ".addEventListener(\"reload\", () => location.reload());",
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

/// 收连接，一直收到关掉为止。每个连接一个线程（SSE 那条会一直占着）
pub fn serve(listener: TcpListener, dir: PathBuf, generation: Generation) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            debug!("有个连接没能接上");
            continue;
        };
        let dir = dir.clone();
        let generation = Arc::clone(&generation);
        std::thread::spawn(move || {
            if let Err(error) = handle(stream, &dir, &generation) {
                debug!(%error, "这个连接断了");
            }
        });
    }
}

/// 报一句「实际在哪个地址上」—— 端口写 0 时是内核给的，得问它要
pub fn address_of(listener: &TcpListener) -> String {
    listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "?".to_string())
}

fn handle(mut stream: TcpStream, dir: &Path, generation: &Generation) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.read_line(&mut request)? == 0 {
        return Ok(());
    }
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");

    if path == EVENTS_PATH {
        return events(&mut stream, generation);
    }
    match resolve(dir, path) {
        Some(Target::File(file)) => send_file(&mut stream, &file),
        Some(Target::Directory(to)) => redirect(&mut stream, &to),
        None => not_found(&mut stream),
    }
}

/// 一个地址要做什么
#[derive(Debug, PartialEq, Eq)]
enum Target {
    /// 给这个文件
    File(PathBuf),
    /// 那个目录该带斜杠，去这个地址
    Directory(String),
}

/// 那个地址对应什么
///
/// - `/` 与 `/x/` → 那个目录下的 `index.html`（目录式 URL 的约定）；
/// - `/x` 而 `x` 是个文件 → 给它；
/// - `/x` 而 `x` 是个目录 → **重定向**到 `/x/`。
///
/// 最后那条不是多余的：从 `/x` 出发，`安装/` 会解析到 `/安装/`，而不是 `/x/安装/`，
/// 于是页面上每个相对地址都指错一层。带斜杠的地址才是那个意思。
fn resolve(dir: &Path, path: &str) -> Option<Target> {
    let decoded = decode(path);
    // 地址里不该有 `..`，而有的话它已经被解码还原了 —— 拼之前一律丢掉
    let relative: PathBuf = decoded
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect();
    let at = dir.join(&relative);
    if path.ends_with('/') {
        let index = at.join("index.html");
        return index.is_file().then_some(Target::File(index));
    }
    if at.is_file() {
        return Some(Target::File(at));
    }
    // 带斜杠的那个地址先到 —— 它才是页面该有的样子
    let index = at.join("index.html");
    if index.is_file() {
        return Some(Target::Directory(format!("{}/", encode(&decoded))));
    }
    None
}

/// 换个地址去。301 而不是 303：地址确实**永久**改写成带斜杠那个了
fn redirect(stream: &mut TcpStream, to: &str) -> std::io::Result<()> {
    respond(
        stream,
        301,
        "text/plain; charset=utf-8",
        b"",
        Some(to),
    )
}

/// 那条长连接：连着，直到浏览器关掉页面
fn events(stream: &mut TcpStream, generation: &Generation) -> std::io::Result<()> {
    stream.write_all(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream; charset=utf-8\r\n\
         Cache-Control: no-store\r\n\
         Connection: keep-alive\r\n\r\n\
         : 连上了\r\n\r\n"
            .as_bytes(),
    )?;
    let mut seen = generation.load(Ordering::Relaxed);
    loop {
        std::thread::sleep(Duration::from_millis(300));
        if stream.write_all(": 心跳\r\n\r\n".as_bytes()).is_err() {
            return Ok(());
        }
        let now = generation.load(Ordering::Relaxed);
        if now != seen {
            seen = now;
            if stream.write_all(b"data: reload\n\n").is_err() {
                return Ok(());
            }
        }
    }
}

fn send_file(stream: &mut TcpStream, file: &Path) -> std::io::Result<()> {
    let mut body = Vec::new();
    std::fs::File::open(file)?.read_to_end(&mut body)?;
    // 预览时一律不过缓存 —— 而 `mdpp serve` 的产物本来就在临时目录里
    respond(
        stream,
        200,
        content_type(file),
        &body,
        None,
    )
}

fn not_found(stream: &mut TcpStream) -> std::io::Result<()> {
    let body = "<!DOCTYPE html>\n\
        <meta charset=\"utf-8\">\n\
        <title>没有这一页</title>\n\
        <style>body{font:16px/1.7 system-ui,sans-serif;margin:4rem auto;max-width:32rem;padding:0 1rem}\n\
        a{color:#2f6fd0}</style>\n\
        <h1>没有这一页</h1>\n\
        <p>地址不对，或者那一页还没建。<a href=\"/\">回站点根</a></p>\n"
        .as_bytes();
    respond(stream, 404, "text/html; charset=utf-8", body, None)
}

/// 回一个响应。`location` 非空时是重定向
fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    location: Option<&str>,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        301 => "Moved Permanently",
        404 => "Not Found",
        _ => "Unknown",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n");
    if let Some(location) = location {
        head.push_str(&format!("Location: {location}\r\n"));
    }
    head.push_str("Cache-Control: no-store\r\nConnection: close\r\n");
    // 302 之后正文没有意义，长度给 0
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// 扩展名 → `Content-Type`。认不出来的给 `application/octet-stream`
/// —— 猜一个 `text/html` 会被浏览器当页面解析，那比报错糟
fn content_type(file: &Path) -> &'static str {
    let extension = file
        .extension()
        .map(|one| one.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "txt" | "md" | "mdpp" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// 百分号解码。解不出 UTF-8 就用 `from_utf8_lossy` 的结果 —— 预览而已，
/// 而一条坏地址不该让整个连接报错
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 百分号编码：`Location` 头里要给的是一个地址，中文得写成 `%E6…` 才认得
///
/// 只编码该编码的（路径里那些分隔符要留着），其余原样
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

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

fn walk(
    dir: &Path,
    out: &mut BTreeMap<PathBuf, (u64, std::time::SystemTime)>,
    depth: usize,
) {
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

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::create_dir_all(dir.path().join("指南/深入")).expect("建得出");
        std::fs::create_dir_all(dir.path().join("assets")).expect("建得出");
        std::fs::write(dir.path().join("index.html"), "根").expect("写得下");
        std::fs::write(dir.path().join("指南/index.html"), "指南").expect("写得下");
        std::fs::write(dir.path().join("指南/深入/index.html"), "深入").expect("写得下");
        std::fs::write(dir.path().join("assets/base.css"), "body{}").expect("写得下");
        dir
    }

    /// 目录式 URL：根与每一层都给那个目录的 `index.html`
    #[test]
    fn directories_serve_their_index() {
        let dir = tree();
        for (url, expected) in [
            ("/", "根"),
            ("/指南/", "指南"),
            ("/指南/深入/", "深入"),
        ] {
            let Target::File(file) = resolve(dir.path(), url).expect("找得到") else {
                panic!("{url} 该给文件，给的是别的");
            };
            assert_eq!(std::fs::read_to_string(&file).expect("读得动"), expected);
        }
    }

    #[test]
    fn files_are_served_as_they_are() {
        let dir = tree();
        let Target::File(file) = resolve(dir.path(), "/assets/base.css").expect("找得到") else {
            panic!("该给文件");
        };
        assert!(file.ends_with("assets/base.css"));
        assert!(resolve(dir.path(), "/没有这个.css").is_none());
        assert!(resolve(dir.path(), "/指南/没有这一页").is_none());
    }

    /// 地址里的百分号编码要解开，否则中文文件名永远找不到
    #[test]
    fn percent_encoded_paths_are_decoded() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::create_dir_all(dir.path().join("指南")).expect("建得出");
        std::fs::write(dir.path().join("指南/安装.html"), "装").expect("写得下");

        let Target::File(file) =
            resolve(dir.path(), "/%E6%8C%87%E5%8D%97/%E5%AE%89%E8%A3%85.html").expect("找得到")
        else {
            panic!("该给文件");
        };
        assert!(file.ends_with("指南/安装.html"));
    }

    /// 地址里混进 `..` 一律丢掉 —— 预览也不该让人翻到目录外面去
    #[test]
    fn climbing_out_is_dropped() {
        let dir = tree();
        // 那些 `..` 当没写过，于是落到站点根那个 `index.html`
        let Target::File(file) = resolve(dir.path(), "/../../../index.html").expect("找得到")
        else {
            panic!("该给文件");
        };
        assert_eq!(file, dir.path().join("index.html"));

        // 而指向系统目录的那些：它被**关在这个目录下面**，于是只是「没有」
        assert!(resolve(dir.path(), "/../../../../etc/passwd").is_none());
    }

    /// 少了那个斜杠要去带斜杠的地址 —— 不然页面里的相对地址会指到上一层去
    #[test]
    fn a_directory_without_its_slash_is_redirected() {
        let dir = tree();
        for (from, to) in [
            ("/指南", "/%E6%8C%87%E5%8D%97/"),
            ("/指南/深入", "/%E6%8C%87%E5%8D%97/%E6%B7%B1%E5%85%A5/"),
            ("/%E6%8C%87%E5%8D%97", "/%E6%8C%87%E5%8D%97/"),
        ] {
            assert_eq!(resolve(dir.path(), from), Some(Target::Directory(to.to_string())), "{from}");
        }

        // 带斜杠的本来就是它要去的地方，不该再跳一次
        assert!(matches!(
            resolve(dir.path(), "/指南/"),
            Some(Target::File(_))
        ));
    }

    #[test]
    fn encoding_leaves_the_address_alone_but_says_nothing_about_it() {
        assert_eq!(encode("/指南/安装/"), "/%E6%8C%87%E5%8D%97/%E5%AE%89%E8%A3%85/");
        assert_eq!(encode("/assets/base.css"), "/assets/base.css");
        // 空格与问号在路径里得编码 —— 问号是地址与查询的分界
        assert_eq!(encode("/a b?c"), "/a%20b%3Fc");
        assert_eq!(decode(&encode("/指南/安装/")), "/指南/安装/");
    }

    #[test]
    fn content_types_are_guessed() {
        assert_eq!(content_type(Path::new("a.html")), "text/html; charset=utf-8");
        assert_eq!(content_type(Path::new("a.css")), "text/css; charset=utf-8");
        assert_eq!(content_type(Path::new("a.SVG")), "image/svg+xml");
        assert_eq!(content_type(Path::new("a.woff2")), "font/woff2");
        // 认不出来的**不给** text/html —— 那会被浏览器当页面解析
        assert_eq!(content_type(Path::new("a.奇怪")), "application/octet-stream");
        assert_eq!(content_type(Path::new("没有后缀")), "application/octet-stream");
    }

    /// 文件一改，收集到的那张小照就该不同 —— 这是「变了没有」的判据
    #[test]
    fn a_changed_file_is_noticed() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let page = dir.path().join("甲.mdpp");
        std::fs::write(&page, "一").expect("写得下");

        let before = collect(&[dir.path().to_path_buf()]);
        assert_eq!(before.len(), 1);

        std::fs::write(&page, "二了").expect("写得下");
        let after = collect(&[dir.path().to_path_buf()]);
        assert_ne!(before, after, "改了却没看出来");

        // 没动就是没动
        assert_eq!(after, collect(&[dir.path().to_path_buf()]));
    }

    /// 新加与删掉一个文件也要被看出来 —— 只比「已存在的那几个」是发现不了新增的
    #[test]
    fn added_and_removed_files_are_noticed() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let before = collect(&[dir.path().to_path_buf()]);
        assert!(before.is_empty());

        let page = dir.path().join("甲.mdpp");
        std::fs::write(&page, "一").expect("写得下");
        let added = collect(&[dir.path().to_path_buf()]);
        assert_ne!(before, added);

        std::fs::remove_file(&page).expect("删得掉");
        assert_ne!(added, collect(&[dir.path().to_path_buf()]));
    }

    /// 目录树里的文件也要收全（模板页在子目录里）
    #[test]
    fn nested_files_are_collected() {
        let dir = tree();
        let all = collect(&[dir.path().to_path_buf()]);
        assert_eq!(all.len(), 4, "{:?}", all.keys().collect::<Vec<_>>());
    }

    /// 那段脚本连的是**根相对**的地址 —— 深层页里也一样连得到
    #[test]
    fn the_live_reload_script_uses_a_root_relative_url() {
        assert!(LIVE_RELOAD.contains(EVENTS_PATH), "{LIVE_RELOAD}");
        // 地址以 `/` 开头：从 `/指南/安装/` 出发不会解析到 `/指南/安装/__mdpp/events`
        assert!(
            LIVE_RELOAD.contains(&format!("\"{EVENTS_PATH}\"")),
            "{LIVE_RELOAD}"
        );
        assert!(LIVE_RELOAD.contains("location.reload()"));
        // 里面不能出现能提前闭合标签的字样
        assert!(!LIVE_RELOAD.contains("</script"));
    }

    #[test]
    fn generation_counts_up() {
        let g = generation();
        assert_eq!(g.load(Ordering::Relaxed), 0);
        bump(&g);
        bump(&g);
        assert_eq!(g.load(Ordering::Relaxed), 2);
    }
}
