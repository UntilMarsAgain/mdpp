//! 正文里**引用的那些本地文件**：扫出来、拷进 `assets/`、把地址改到那儿去。
//!
//! ## 为什么非得改地址
//!
//! 产物是目录式的：`pages/指南/入门.mdpp` 落到 `build/指南/入门/index.html`。而正文
//! 里的 `图.png` 是照**源文件**那一层写的（`pages/指南/图.png`），到了产物里它指的
//! 却是 `build/指南/入门/图.png` —— 深了一层，因为页面自己多了一层目录。
//!
//! 所以只有两条路：要么把每个资源**拷进引用它的那一页的目录**（地址一个字不改，
//! 代价是同一张图被拷几十遍），要么把地址**改到共享的 `assets/`**（资源只有一份，
//! 代价是要动 HTML）。这里选后者：`assets/` 已经是站点资源待的地方（样式、脚本、
//! favicon 都在那儿），一处就够。
//!
//! ## 只认本地相对地址
//!
//! 带协议的（`https:`、`data:`）、以 `/` 开头的（站点根绝对地址）、`#` 开头的，
//! 一律不动 —— 那不是「一个文件」，或者不是这个工具该去替它找的东西。
//!
//! 解析出来**不在 `pages/` 里面**的一律不拷：一条 `../../../etc/passwd` 到了这一步
//! 会变成站点里一个可下载的文件，而引用它的是一篇正文。

use std::path::{Component, Path, PathBuf};

use crate::support::url;

/// 扫完一页的结果
#[derive(Debug, Default)]
pub struct Scan {
    /// 改好了地址的 HTML
    pub html: String,
    /// 要拷进产物的那些（按目标去重）
    pub wanted: Vec<Wanted>,
    /// 指了但 `pages/` 里找不到的
    pub missing: Vec<String>,
}

/// 一个待拷贝的资源
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// 源文件的完整路径
    pub from: PathBuf,
    /// 产物里的位置（`build/assets/…`）
    pub to: PathBuf,
}

/// 一页的上下文：地址是相对**源文件那一层**写的，产物在别处
#[derive(Debug, Clone)]
pub struct Context<'a> {
    /// 这一页的源文件**目录**
    pub page_dir: &'a Path,
    /// 正文目录（资源不许跑出它）
    pub pages_dir: &'a Path,
    /// 产物里 `assets/` 的完整路径
    pub assets_dir: &'a Path,
    /// 页面上指到 `assets/` 的**相对**前缀（`../assets` 那样，按页深算）
    pub assets_prefix: &'a str,
}

/// 扫一遍正文里的地址：本地相对的那些改写到 `assets/`，其余原样留着。
///
/// 只认**标签里的属性**。正文、注释、`<script>`/`<style>` 的内容里出现
/// `src="图.png"` 是字面量，不是属性，不能改写。
pub fn scan(html: &str, context: &Context) -> Scan {
    let mut scan = Scan {
        html: String::with_capacity(html.len()),
        ..Scan::default()
    };
    let mut at = 0usize;
    while at < html.len() {
        let Some(rel) = html[at..].find('<') else {
            scan.html.push_str(&html[at..]);
            break;
        };
        let start = at + rel;
        scan.html.push_str(&html[at..start]);
        let rest = &html[start..];

        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .map(|offset| start + offset + 3)
                .unwrap_or(html.len());
            scan.html.push_str(&html[start..end]);
            at = end;
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            let end = rest
                .find('>')
                .map(|offset| start + offset + 1)
                .unwrap_or(html.len());
            scan.html.push_str(&html[start..end]);
            at = end;
            continue;
        }
        if rest.starts_with("</") {
            let end = rest
                .find('>')
                .map(|offset| start + offset + 1)
                .unwrap_or(html.len());
            scan.html.push_str(&html[start..end]);
            at = end;
            continue;
        }

        let Some(name_end) = tag_name_end(html, start + 1) else {
            scan.html.push('<');
            at = start + 1;
            continue;
        };
        let tag = &html[start + 1..name_end];
        let (end, self_closing) = scan_start_tag(html, name_end, tag, context, &mut scan);
        at = end;

        if !self_closing && is_raw_text(tag) {
            let close = find_close_tag(html, at, tag).unwrap_or(html.len());
            scan.html.push_str(&html[at..close]);
            at = close;
        }
    }
    scan
}

/// 解析一个开始标签，原样复制，只在真正的资源属性上改值。
fn scan_start_tag(
    html: &str,
    name_end: usize,
    tag: &str,
    context: &Context,
    scan: &mut Scan,
) -> (usize, bool) {
    scan.html.push('<');
    scan.html.push_str(tag);
    let bytes = html.as_bytes();
    let mut at = name_end;

    while at < html.len() {
        if bytes[at].is_ascii_whitespace() {
            let begin = at;
            while at < html.len() && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            scan.html.push_str(&html[begin..at]);
            continue;
        }
        if bytes[at] == b'>' {
            scan.html.push('>');
            return (at + 1, false);
        }
        if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'>') {
            scan.html.push_str("/>");
            return (at + 2, true);
        }

        // 属性名。遇到奇怪的字符（例如落单的引号）就原样跳过一个字符。
        let name_start = at;
        while at < html.len()
            && !matches!(
                bytes[at],
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'=' | b'>' | b'/'
            )
        {
            at += 1;
        }
        if name_start == at {
            let ch = html[at..].chars().next().expect("还没到结尾");
            scan.html.push(ch);
            at += ch.len_utf8();
            continue;
        }
        let attr = &html[name_start..at];
        scan.html.push_str(attr);

        let space_start = at;
        while at < html.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        scan.html.push_str(&html[space_start..at]);

        if at >= html.len() || bytes[at] != b'=' {
            continue;
        }
        scan.html.push('=');
        at += 1;
        let space_start = at;
        while at < html.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        scan.html.push_str(&html[space_start..at]);

        if at < html.len() && (bytes[at] == b'"' || bytes[at] == b'\'') {
            let quote = bytes[at];
            scan.html.push(quote as char);
            at += 1;
            let Some(offset) = html[at..].find(quote as char) else {
                scan.html.push_str(&html[at..]);
                return (html.len(), false);
            };
            let close = at + offset;
            let value = &html[at..close];
            let replacement = if is_raw_text(tag) || !is_asset_attr(attr) {
                None
            } else {
                resolve_asset(value, context, scan)
            };
            scan.html.push_str(replacement.as_deref().unwrap_or(value));
            scan.html.push(quote as char);
            at = close + 1;
        } else {
            // 没引号的属性值也原样复制；不在这里改写，避免替作者决定新的引号形式。
            let value_start = at;
            while at < html.len()
                && !matches!(bytes[at], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'>')
            {
                at += 1;
            }
            scan.html.push_str(&html[value_start..at]);
        }
    }
    (html.len(), false)
}

/// 属性值是不是资源地址；改了值之后把结果记进 `wanted`，失败记进 `missing`。
fn resolve_asset(value: &str, context: &Context, scan: &mut Scan) -> Option<String> {
    match resolve(value, context) {
        Ok((from, to)) => {
            let relative = to
                .strip_prefix(context.assets_dir)
                .expect("目标就在 assets 里")
                .to_string_lossy()
                .replace('\\', "/");
            scan.wanted.push(Wanted { from, to });
            Some(format!(
                "{}/{}",
                context.assets_prefix,
                url::encode_href(&relative)
            ))
        }
        Err(Skip::NotOurs(reason)) => {
            tracing::debug!(value, reason, "这个地址不归我管");
            None
        }
        Err(Skip::Missing(value)) => {
            scan.missing.push(value);
            None
        }
    }
}

fn tag_name_end(html: &str, from: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    if !bytes.get(from).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let mut at = from;
    while at < bytes.len()
        && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'-' | b':' | b'_'))
    {
        at += 1;
    }
    (at > from).then_some(at)
}

fn is_asset_attr(name: &str) -> bool {
    name.eq_ignore_ascii_case("src")
        || name.eq_ignore_ascii_case("href")
        || name.eq_ignore_ascii_case("poster")
}

/// 这些标签的内容是原始文本，里面的 `src="…"` 不是属性。
fn is_raw_text(tag: &str) -> bool {
    [
        "script",
        "style",
        "textarea",
        "title",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "plaintext",
    ]
    .iter()
    .any(|raw| tag.eq_ignore_ascii_case(raw))
}

/// 找 `</tag` —— 原始文本元素的结束位置；找不到就吃到结尾。
fn find_close_tag(html: &str, from: usize, tag: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut at = from;
    while at + 2 + tag.len() <= bytes.len() {
        if bytes[at] == b'<' && bytes[at + 1] == b'/' {
            let end = at + 2 + tag.len();
            if html[at + 2..end].eq_ignore_ascii_case(tag)
                && (end == bytes.len()
                    || matches!(bytes[end], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'>'))
            {
                return Some(at);
            }
        }
        at += 1;
    }
    None
}

/// 一个本地地址为什么没被拷 —— 前者不进报告，后者要报
enum Skip {
    /// 不是这个工具该管的东西：带协议的、根绝对地址、锚点、另一个页面
    NotOurs(&'static str),
    /// 像是要指一个文件，而那儿没有 —— 这才是要报给作者的一笔
    Missing(String),
}

/// 一个本地相对地址解出来是：源文件在哪儿、产物里该放哪儿
fn resolve(value: &str, context: &Context) -> Result<(PathBuf, PathBuf), Skip> {
    // 已经是产物里的资源地址（`assets/mdpp-…` 或 `../assets/mdpp-…`）——
    // 那是 `::css` / `::js` 的站点模式自己写出去的链接，不必再去 `pages/` 找
    let prefix = context.assets_prefix.trim_start_matches("./");
    if value.starts_with(&format!("{prefix}/mdpp-")) {
        return Err(Skip::NotOurs("已经是产物里的资源"));
    }
    // 以 `/` 结尾的是**站内地址**（本页解析器给出去的每一条都以 `/` 收尾），
    // 不是文件 —— 别去 `pages/另一页/` 那儿找一个目录
    if value.trim().is_empty()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.starts_with('#')
        || value.starts_with('?')
        || has_scheme(value)
    {
        return Err(Skip::NotOurs("不是本地相对地址"));
    }
    // 进来的地址可能**已经被转义过**：markdown-it 对 `href` / `src` 会把非 ASCII 转成
    // `%E6…`，`targets::encode_target_spaces` 会把链接目标里的空格补成 %20，`::image src=`
    // 里的 `&` 也会被转义。判「文件在不在」用的是**解码之后**的那一份 —— 拿转义串去找
    // 文件永远找不到；而解不出 UTF-8 就退回作者写下的原串，报告要说的正是那一份
    let decoded = url::decode_exact(value).unwrap_or_else(|| value.to_string());
    let source = normalize(&context.page_dir.join(&decoded));
    let Ok(relative) = source.strip_prefix(context.pages_dir) else {
        return Err(Skip::NotOurs("跑出正文目录了"));
    };
    if relative.as_os_str().is_empty() || relative.starts_with("..") {
        return Err(Skip::NotOurs("跑出正文目录了"));
    }
    // 页面自己算一个：引用它自己的那一页不算资源
    let is_page = source
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| crate::project::site::PAGE_SUFFIXES.contains(&ext));
    if is_page {
        return Err(Skip::NotOurs("那是另一个页面"));
    }
    // 符号链接不拷：链接指向哪里是链接自己的事，而资源扫描只该收
    // `pages/` 里真实存在的文件。跟过去就可能把外面的东西带进产物。
    // 不能只看最后一级：`pages/link -> /outside` 时，`link/secret.png`
    // 的最后一级是普通文件，真正的越界发生在中间那一级。
    if has_symlink_component(context.pages_dir, relative) {
        return Err(Skip::Missing(format!(
            "{decoded}（路径里有符号链接，不拷）"
        )));
    }
    let Ok(source_real) = source.canonicalize() else {
        return Err(Skip::Missing(decoded.clone()));
    };
    let Ok(pages_real) = context.pages_dir.canonicalize() else {
        return Err(Skip::Missing(decoded.clone()));
    };
    if !source_real.starts_with(&pages_real) {
        return Err(Skip::Missing(format!(
            "{decoded}（路径跑出正文目录，不拷）"
        )));
    }
    match std::fs::symlink_metadata(&source) {
        Ok(meta) if meta.is_file() => {}
        _ => return Err(Skip::Missing(decoded.clone())),
    }
    let target = context.assets_dir.join(relative);
    Ok((source, target))
}

/// `root` 到 `relative` 的每一级（含根）都不能是符号链接。
fn has_symlink_component(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    if matches!(
        std::fs::symlink_metadata(&current),
        Ok(meta) if meta.file_type().is_symlink()
    ) {
        return true;
    }
    for part in relative.components() {
        current.push(part.as_os_str());
        if matches!(
            std::fs::symlink_metadata(&current),
            Ok(meta) if meta.file_type().is_symlink()
        ) {
            return true;
        }
    }
    false
}

/// `foo.png` 里带 `:` 吗 —— 带了就当协议（`https:`、`data:`），不当文件名
fn has_scheme(value: &str) -> bool {
    match value.find(':') {
        Some(at) => !value[..at].contains('/'),
        None => false,
    }
}

/// `a/./b/../c` → `a/c`。纯字面，不去碰文件系统（那些路径多半还不存在）
fn normalize(path: &Path) -> PathBuf {
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str().to_os_string()),
        }
    }
    out.iter().collect()
}

/// 把待拷贝的那些真的拷过去（一个目标已经拷过就跳过）
pub fn copy(wanted: &[Wanted], already: &mut Vec<PathBuf>) -> std::io::Result<usize> {
    let mut copied = 0usize;
    for one in wanted {
        if already.contains(&one.to) {
            continue;
        }
        if let Some(parent) = one.to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&one.from, &one.to)?;
        already.push(one.to.clone());
        copied += 1;
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个「嵌套页」的角度：正文目录下的 `指南/`，前缀是 `../assets`
    fn context<'a>(page_dir: &'a Path, pages_dir: &'a Path, assets_dir: &'a Path) -> Context<'a> {
        Context {
            page_dir,
            pages_dir,
            assets_dir,
            assets_prefix: "../assets",
        }
    }

    /// 同一页引的两张图落到两个地方：同目录的跟着目录走，上一层的落在 `assets/` 根下
    #[test]
    fn a_local_image_moves_to_assets_and_its_address_follows() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("build/assets");
        std::fs::create_dir_all(pages.join("指南")).expect("建得出");
        std::fs::write(pages.join("指南/图.png"), "图片").expect("写得下");
        std::fs::write(pages.join("其他.png"), "另一张").expect("写得下");

        let html = r#"<p><img src="图.png"> 与 <img src="../其他.png"></p>"#;
        let scan = scan(html, &context(&pages.join("指南"), &pages, &assets));

        assert_eq!(
            scan.html,
            r#"<p><img src="../assets/指南/图.png"> 与 <img src="../assets/其他.png"></p>"#,
            "{}",
            scan.html
        );
        assert_eq!(scan.missing, Vec::<String>::new());
        assert_eq!(
            scan.wanted,
            vec![
                Wanted {
                    from: pages.join("指南/图.png"),
                    to: assets.join("指南/图.png"),
                },
                Wanted {
                    from: pages.join("其他.png"),
                    to: assets.join("其他.png"),
                },
            ]
        );
    }

    /// 同一个资源被引两次只记一次拷贝
    #[test]
    fn the_same_file_referenced_twice_is_copied_once() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::write(pages.join("图.png"), "图片").expect("写得下");

        let scan = scan(
            r#"<img src="图.png"><img src="./图.png">"#,
            &context(&pages, &pages, &assets),
        );
        assert_eq!(
            scan.html,
            r#"<img src="../assets/图.png"><img src="../assets/图.png">"#
        );
        assert_eq!(scan.wanted.len(), 2, "两处引用都要改地址");
        let mut already = Vec::new();
        let copied = copy(&scan.wanted, &mut already).expect("拷得动");
        assert_eq!(copied, 1, "同一个目标只拷一次");
    }

    /// 协议、根绝对、锚点、跨页 —— 一律原样留着
    #[test]
    fn what_is_not_a_local_file_is_left_alone() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");

        let html = concat!(
            r#"<a href="https://example.com/a.png">外站</a>"#,
            r#"<img src="data:image/png;base64,AAA">"#,
            r##"<img src="/绝对/地址.png">"##,
            r##"<a href="#小节">锚点</a>"##,
            r#"<a class="mdpp-wikilink" href="另一页/">站内</a>"#,
        );
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert_eq!(scan.html, html, "不该动的动了");
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
        assert!(scan.missing.is_empty(), "{:?}", scan.missing);
    }

    /// 符号链接不拷：链接指向哪里是链接自己的事，资源扫描只收真实文件
    #[cfg(unix)]
    #[test]
    fn a_symlinked_asset_is_not_copied() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::create_dir_all(&outside).expect("建得出");
        std::fs::write(outside.join("secret.png"), "不该被拷走").expect("写得下");
        std::os::unix::fs::symlink(outside.join("secret.png"), pages.join("图.png"))
            .expect("建得出符号链接");

        let scan = scan(r#"<img src="图.png">"#, &context(&pages, &pages, &assets));
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
        assert!(
            scan.missing.iter().any(|one| one.contains("符号链接")),
            "{:?}",
            scan.missing
        );
    }

    /// 指了但文件不在那儿 —— 报告里要有一笔，作者才看得出是笔误
    #[test]
    fn a_reference_to_nothing_is_reported() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");

        let scan = scan(
            r#"<img src="没有这张.png">"#,
            &context(&pages, &pages, &assets),
        );
        assert_eq!(scan.html, r#"<img src="没有这张.png">"#, "找不到就原样留着");
        assert_eq!(scan.missing, vec!["没有这张.png".to_string()]);
        assert!(scan.wanted.is_empty());
    }

    /// 引用跑出 `pages/` 的不拷 —— 那是一条 `../../../etc/passwd`
    #[test]
    fn a_reference_climbing_out_of_pages_is_refused() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("project/pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        let secret = dir.path().join("project/口令.txt");
        std::fs::write(&secret, "不该被拷走").expect("写得下");

        let scan = scan(
            r#"<img src="../../口令.txt">"#,
            &context(&pages, &pages, &assets),
        );
        assert!(scan.wanted.is_empty(), "跑出去了还拷：{:?}", scan.wanted);
        // 原样留着，但**不报缺** —— 那不是笔误，是作者写错了地方
        assert!(scan.missing.is_empty(), "{:?}", scan.missing);
        assert_eq!(scan.html, r#"<img src="../../口令.txt">"#);
    }

    /// 指向另一个页面文件的不算资源 —— 页面之间该用 `[[…]]`
    #[test]
    fn a_reference_to_another_page_is_not_an_asset() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::write(pages.join("乙.mdpp"), "# 乙").expect("写得下");

        let scan = scan(
            r#"<a href="乙.mdpp">乙</a>"#,
            &context(&pages, &pages, &assets),
        );
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
    }

    /// 不是资源属性的（`class=`、`style=`、`<a href=>` 那类）一律不碰
    #[test]
    fn only_those_three_attributes_are_looked_at() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::write(pages.join("图.png"), "图片").expect("写得下");

        let html = r#"<div class="图.png" data-x="图.png"><img alt="图.png" src="图.png"></div>"#;
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert_eq!(
            scan.html,
            r#"<div class="图.png" data-x="图.png"><img alt="图.png" src="../assets/图.png"></div>"#
        );
        assert_eq!(scan.wanted.len(), 1);
    }

    /// 正文、注释、`data-src` 里的 `src=` 都不是属性，不能改写
    #[test]
    fn text_and_comments_that_look_like_attributes_are_left_alone() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::write(pages.join("图.png"), "图片").expect("写得下");

        let html = concat!(
            r#"<div>正文里写 src="图.png" 是这样</div>"#,
            r#"<!-- <img src="图.png"> -->"#,
            r#"<p data-src="图.png">别动</p>"#,
        );
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert_eq!(scan.html, html, "不该动的动了");
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
        assert!(scan.missing.is_empty(), "{:?}", scan.missing);
    }

    /// 目录符号链接也要拦住：只检查最后一级会漏掉 `link/secret.png`
    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_on_the_way_is_not_followed() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::create_dir_all(&outside).expect("建得出");
        std::fs::write(outside.join("secret.png"), "不该被拷走").expect("写得下");
        std::os::unix::fs::symlink(&outside, pages.join("链接")).expect("建得出符号链接");

        let html = r#"<img src="链接/secret.png">"#;
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
        assert_eq!(scan.html, html, "不该改地址");
        assert!(
            scan.missing.iter().any(|one| one.contains("符号链接")),
            "{:?}",
            scan.missing
        );
    }

    /// 词法归一：`a/./b/../c` → `a/c`，而 `..` 不会跑出根
    #[test]
    fn paths_are_normalized_without_touching_the_disk() {
        assert_eq!(normalize(Path::new("a/./b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize(Path::new("a/b/../../..")), PathBuf::from(""));
        assert_eq!(
            normalize(Path::new("/甲/乙/../丙.mdpp")),
            PathBuf::from("/甲/丙.mdpp")
        );
    }

    /// 看着像路径但带协议的（`C:/x`、`.git` 这种）不被当成协议，也不被拷
    #[test]
    fn schemes_are_spotted() {
        for value in [
            "https://x/y.png",
            "data:image/png;base64,AA",
            "javascript:alert(1)",
        ] {
            assert!(has_scheme(value), "{value} 该当协议");
        }
        for value in ["a/b:c.png", "./图.png", "../图.png", "图.png"] {
            assert!(!has_scheme(value), "{value} 不该当协议");
        }
    }

    /// `<script>` / `<style>` **内容**里的 `src=` / `href=` 是脚本/样式的字面量，
    /// 不是要改写的 HTML 属性 —— `::js` 里写 `document.write('<img src="图.png">')`
    /// 是一段程序的字符串，改它等于改语义，拷出来的文件还指错地方
    #[test]
    fn script_and_style_content_is_left_alone() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::write(pages.join("图.png"), "图片").expect("写得下");

        // 脚本字符串里的 `src="图.png"` 一个都不动 —— 即使文件真的在 pages/ 里
        let html = concat!(
            r#"<script>document.write('<img src="图.png">')</script>"#,
            r#"<img src="图.png">"#,
        );
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert_eq!(
            scan.html,
            concat!(
                r#"<script>document.write('<img src="图.png">')</script>"#,
                r#"<img src="../assets/图.png">"#,
            ),
            "{}",
            scan.html
        );
        // 只拷了脚本外面那一个 `img` 的引用
        assert_eq!(scan.wanted.len(), 1, "{:?}", scan.wanted);
        // 脚本里那句不算缺资源
        assert!(scan.missing.is_empty(), "{:?}", scan.missing);
    }

    /// `<style>` 里的 `url("图.png")` 同样不动 —— 那是样式内容的字面量
    #[test]
    fn style_content_urls_are_left_alone() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let pages = dir.path().join("pages");
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&pages).expect("建得出");
        std::fs::write(pages.join("图.png"), "图片").expect("写得下");

        let html = r#"<style>.a { background: url("图.png") }</style>"#;
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert_eq!(scan.html, html, "样式里的地址不该动");
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
        assert!(scan.missing.is_empty(), "{:?}", scan.missing);
    }
}
