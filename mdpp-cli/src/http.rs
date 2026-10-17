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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::url;

use tracing::debug;

/// 正在被服务的那一份产物
///
/// 换的是**指针**，不是目录里的内容：重建时写进另一个目录，全部写完了才把指针挪过去。
/// 于是每个请求读到的要么是完整的一遍旧产物、要么是完整的一遍新产物，**中间没有
/// 「正被 `remove_dir_all`、正写一半」的窗口** —— 那个窗口里 `File::open` 要么拿到
/// `ENOENT`（CSS 突然没了、页面半白），要么拿到半截的 HTML。
///
/// `mdpp build` 犯不上这个：那个目录没人读，只有 `serve` 把它挂在 HTTP 上。
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
/// 我没发的事件」**不报任何错**（控制台干净、控制台干净得像对的），于是它看起来像
/// 「Firefox 不行」而其实是所有浏览器都不行（这个坑踩过一次）
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

/// 收连接，一直收到关掉为止。每个连接一个线程（SSE 那条会一直占着）
pub fn serve(listener: TcpListener, root: Root, generation: Generation) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            debug!("有个连接没能接上");
            continue;
        };
        let root = root.clone();
        let generation = Arc::clone(&generation);
        std::thread::spawn(move || {
            if let Err(error) = handle(stream, &root, &generation) {
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

fn handle(mut stream: TcpStream, root: &Root, generation: &Generation) -> std::io::Result<()> {
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
    // 目录**这一次请求才取**：换根是重建完之后的事，于是这一请求要么看到完整的一遍
    // 旧的、要么看到完整的一遍新的
    let dir = root.serving();
    match resolve(&dir, path) {
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
    let decoded = url::decode_lossy(path);
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
        return Some(Target::Directory(format!(
            "{}/",
            url::encode_path(&decoded)
        )));
    }
    None
}

/// 换个地址去。301 而不是 303：地址确实**永久**改写成带斜杠那个了
fn redirect(stream: &mut TcpStream, to: &str) -> std::io::Result<()> {
    respond(stream, 301, "text/plain; charset=utf-8", b"", Some(to))
}

/// 那条长连接：连着，直到浏览器关掉页面
///
/// **响应体不能靠连接关闭来界定**：HTTP/1.1 里没有 `Content-Length` 也没有
/// `Transfer-Encoding` 的响应，长度只能等连接关掉才算数 —— 而那与 `keep-alive`
/// 自相矛盾，各家浏览器的处理并不一致（有的会先攒够再交给上层，于是「流」没了）。
/// `Transfer-Encoding: chunked` 把这件事说死：每一段自己带长度，连接不必关。
///
/// 心跳用**注释行**（`: 心跳`）而不是 `data:`：注释行不算事件，页面脚本不会因为它动。
/// 而它非发不可 —— 中间什么都不写的连接会被当成死的（代理、浏览器各有各的超时），
/// 心跳是唯一能让「还活着」这件事被看出来又不打扰页面的办法。
fn events(stream: &mut TcpStream, generation: &Generation) -> std::io::Result<()> {
    stream.write_all(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream; charset=utf-8\r\n\
         Cache-Control: no-store\r\n\
         Connection: keep-alive\r\n\
         Transfer-Encoding: chunked\r\n\r\n"
            .as_bytes(),
    )?;
    chunk(stream, ": 连上了\n\n")?;
    let mut seen = generation.load(Ordering::Relaxed);
    loop {
        std::thread::sleep(Duration::from_millis(300));
        if chunk(stream, ": 心跳\n\n").is_err() {
            return Ok(());
        }
        let now = generation.load(Ordering::Relaxed);
        if now != seen {
            seen = now;
            // `data: reload` —— 一条 `message` 事件，载荷是 `reload`，与
            // `LIVE_RELOAD` 里挂的那个 `message` 配对（那上面写了为什么不是 `reload`）
            if chunk(stream, "data: reload\n\n").is_err() {
                return Ok(());
            }
        }
    }
}

/// 按 chunked 编码发一段：`长度\r\n` + 内容 + `\r\n`
fn chunk(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    write!(stream, "{:x}\r\n", body.len())?;
    stream.write_all(body.as_bytes())?;
    stream.write_all(b"\r\n")
}

/// 把 [`events`] 说的话取回来 —— 真的走一次 TCP，不是对着源码字符串看
///
/// 返回 `(响应头, 各段的内容)`：头到第一个空行为止；各段是从 chunked 里剥出来的。
/// 心跳是注释行（以 `:` 开头），一并留着 —— 断言「每 300ms 有一条」用得上
///
/// **喊「重出好了」的时机不能猜**。[`events`] 进循环前就把当前计数收进 `seen`，
/// 早了那一次它就 `now == seen`、那条事件永远不发，而下面死等它。
///
/// 时机靠一个**看得见的信号**对齐，不用 `sleep` 猜：第一条心跳是在 `seen` 收好
/// **之后**才写出去的（见 [`events`] 里那两行的先后），所以「客户端收到了心跳」
/// 就等于「服务器已经准备好了」。机器忙的时候 `sleep(20ms)` 会不够——
/// 这个用例单跑时过、并行跑时不过，就是这么来的（猜时间的那种写法迟早这样）
///
/// 那个 5 秒上限是**保底**：服务器要是不发东西，该「读到空」然后断言失败，
/// 而不是把整个测试挂住（挂过一次，排查花的时间比写它还长）
#[cfg(test)]
fn events_over_loopback(generation: &Generation) -> (String, Vec<String>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("绑得上");
    let address = listener.local_addr().expect("有地址");
    let (beating, beat_seen) = std::sync::mpsc::channel();

    // 客户端：连上、问一句、读。见到心跳就报一声，见到事件就收工
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(address).expect("连得上");
        stream
            .write_all(b"GET /__mdpp/events HTTP/1.1\r\nHost: x\r\n\r\n")
            .expect("说得出去");

        let mut raw = Vec::new();
        let mut buf = [0u8; 1024];
        let mut alive = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                // **把「为什么停了」写进结果里** —— 吞掉它的话，断言失败时看到的
                // 只有「没有事件」，得再加一轮打印才猜得出是对端挂了还是读出错
                Ok(0) => {
                    raw.extend_from_slice("\n[客户端：连接被关]\n".as_bytes());
                    break;
                }
                Err(error) => {
                    raw.extend_from_slice(format!("\n[客户端：读出错 {error}]\n").as_bytes());
                    break;
                }
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw);
                    if !alive && text.contains(": 心跳") {
                        alive = true;
                        let _ = beating.send(());
                    }
                    let events = text
                        .lines()
                        .filter(|line| line.starts_with("data: ") || line.starts_with("event: "))
                        .count();
                    if events >= 1 {
                        break;
                    }
                }
            }
        }
        raw
    });

    let (server, _) = listener.accept().expect("等得到连接");
    let writer = server.try_clone().expect("克隆得出");
    let counter = generation.clone();
    // [`events`] 那个线程**不等它自己回得来**：读够之后我们会断开，它的下一次
    // `write_all` 就会拿到错误。不 join —— 万一它没回得来，那正是要避免的挂死，
    // 不该在测试里再等一遍
    std::thread::spawn(move || {
        let mut writer = writer;
        let _ = events(&mut writer, &counter);
    });

    // 等心跳到达（= 服务器收好 `seen` 了），再喊「重出好了」
    beat_seen
        .recv_timeout(Duration::from_secs(5))
        .expect("该收到心跳 —— 服务器连上了吗");
    bump(generation);

    let raw = client.join().expect("读得出来");
    // 断开对端：`events` 的下一次 `write_all` 会拿到错误，它就从那个死循环回来了
    let _ = server.shutdown(std::net::Shutdown::Both);

    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    // chunked：一段一段剥（长度行 + 内容 + 空行）
    let mut rest = body;
    let mut lines = Vec::new();
    while let Some(at) = rest.find("\r\n") {
        let size = usize::from_str_radix(&rest[..at], 16).unwrap_or(0);
        let start = at + 2;
        let Some(after) = rest.get(start + size + 2..) else {
            break;
        };
        lines.extend(rest[start..start + size].lines().map(str::to_string));
        rest = after;
    }
    (head.to_string(), lines)
}

fn send_file(stream: &mut TcpStream, file: &Path) -> std::io::Result<()> {
    let mut body = Vec::new();
    std::fs::File::open(file)?.read_to_end(&mut body)?;
    // 预览时一律不过缓存 —— 而 `mdpp serve` 的产物本来就在临时目录里
    respond(stream, 200, content_type(file), &body, None)
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
    // Content-Length 照正文的实际长度写 —— 重定向那一路（301）递的是空正文，
    // 所以它出去的是 0；文件与 404 有正文，长度就是正文的
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

// 百分号编解码不在这里另写一份 —— 两种策略（给机器跟的地址 / 给人看的 `href`）
// 连同两份解码都在 `crate::url`，那边说了每种给谁用

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
        for (url, expected) in [("/", "根"), ("/指南/", "指南"), ("/指南/深入/", "深入")]
        {
            let Target::File(file) = resolve(dir.path(), url).expect("找得到") else {
                panic!("{url} 该给文件，给的是别的");
            };
            assert_eq!(std::fs::read_to_string(&file).expect("读得动"), expected);
        }
    }

    /// 普通文件按地址原样给，**没有的就回 404** —— 拿别的文件（比如哪个 index）顶上，
    /// 预览里显示的就不是你点开的那一页
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
            assert_eq!(
                resolve(dir.path(), from),
                Some(Target::Directory(to.to_string())),
                "{from}"
            );
        }

        // 带斜杠的本来就是它要去的地方，不该再跳一次
        assert!(matches!(
            resolve(dir.path(), "/指南/"),
            Some(Target::File(_))
        ));
    }

    /// 后缀那张表要认得出大小写（`a.SVG` 也得认），文本类要带 `charset=utf-8` ——
    /// 编码不写进 Content-Type，浏览器只能靠猜
    #[test]
    fn content_types_are_guessed() {
        assert_eq!(
            content_type(Path::new("a.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(content_type(Path::new("a.css")), "text/css; charset=utf-8");
        assert_eq!(content_type(Path::new("a.SVG")), "image/svg+xml");
        assert_eq!(content_type(Path::new("a.woff2")), "font/woff2");
        // 认不出来的**不给** text/html —— 那会被浏览器当页面解析
        assert_eq!(
            content_type(Path::new("a.奇怪")),
            "application/octet-stream"
        );
        assert_eq!(
            content_type(Path::new("没有后缀")),
            "application/octet-stream"
        );
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

    /// **服务器真的发出来的那些字，要与页面挂的那个事件名对得上**
    ///
    /// 对不上时刷新**静悄悄地不响**：`EventSource` 对「你监听了我没发的事件」不报
    /// 任何错，控制台干净得像对的。错法是 `data: reload` 配 `addEventListener("reload")`
    /// —— SSE 里 `data:` 造的是一条 **`message`** 事件（载荷才是 `reload`），而
    /// `addEventListener` 的名字是**事件名**，两个名字空间根本不通。
    ///
    /// 现象是「Firefox 不刷新」，而其实所有浏览器都不刷新（这个坑踩过一次）
    #[test]
    fn the_server_and_the_script_agree_on_the_event_name() {
        let (head, lines) = events_over_loopback(&generation());
        assert!(
            head.contains("text/event-stream"),
            "那不是一条事件流：\n{head}"
        );
        let sent: Vec<&str> = lines
            .iter()
            .filter(|line| line.starts_with("data: ") || line.starts_with("event: "))
            .map(String::as_str)
            .collect();
        assert!(!sent.is_empty(), "一条事件也没发出去：\n{lines:?}");

        // 页面挂的是哪个名字
        let listens = LIVE_RELOAD
            .split("addEventListener(\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("脚本该挂一个监听");

        for line in sent {
            if let Some(name) = line.strip_prefix("event: ") {
                assert_eq!(
                    name.trim(),
                    listens,
                    "服务器发了事件 {name}，页面挂的却是 {listens} —— 它永远不响"
                );
            } else if let Some(data) = line.strip_prefix("data: ") {
                // `data:` 造的那条事件**永远叫 message**，与载荷无关
                assert_eq!(
                    listens, "message",
                    "服务器发的是 `data: {data}`，页面挂的却是 {listens} —— \
                     它永远不响（`data:` 造的是 message 事件，载荷才是 \"{data}\"）"
                );
            }
        }
    }

    /// 那条长连接得说清自己有多长 —— `keep-alive` 与「靠连接关闭来界定长度」是
    /// 自相矛盾的，各家浏览器的处理并不一致（有的先攒够再交给上层，于是「流」没了）。
    /// `chunked` 把这件事说死
    #[test]
    fn the_event_stream_declares_its_length_with_chunked_encoding() {
        let (head, _) = events_over_loopback(&generation());
        assert!(
            head.contains("Transfer-Encoding: chunked"),
            "长连接得带 chunked：\n{head}"
        );
        assert!(
            !head.contains("Content-Length"),
            "chunked 与 Content-Length 只能有一个：\n{head}"
        );
    }

    /// 计数从 0 起、每重出加一 —— 长连接全靠这个数的变化喊「刷新」，
    /// 加不上或者起点不对，浏览器那边就一直不响
    #[test]
    fn generation_counts_up() {
        let g = generation();
        assert_eq!(g.load(Ordering::Relaxed), 0);
        bump(&g);
        bump(&g);
        assert_eq!(g.load(Ordering::Relaxed), 2);
    }
}
