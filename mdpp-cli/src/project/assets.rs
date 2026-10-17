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

use crate::url;

/// 值得一看的属性：页面里指向一个文件的地方
const ATTRIBUTES: [&str; 3] = ["src", "href", "poster"];

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
pub fn scan(html: &str, context: &Context) -> Scan {
    let mut out = Scan {
        html: String::with_capacity(html.len()),
        ..Scan::default()
    };
    let mut rest = html;
    // 当前在不在 `<script>` / `<style>` 里：那里的 `src=` / `href=` 是脚本/样式的内容，
    // 不是要改写的属性 —— `::js` 里写 `document.write('<img src="图.png">')` 是字面量，
    // 改写它等于改了一段程序的语义（见 `update_raw` 那一段的说明）
    let mut raw = Raw::None;
    loop {
        let Some(at) = rest.find("=\"") else {
            out.html.push_str(rest);
            return out;
        };
        // 先把这 `="` 前面的那一段喂给状态机：`<script …>` 从这儿进来、
        // `</script>` 从这儿出去，状态翻对了，`="` 落在哪一边才判得准
        update_raw(&mut raw, &rest[..at]);

        let value = match value_of(rest, at + 2) {
            Some(value) => value,
            // 引号没关上：这一段不是属性。跳过 `="` 接着找（往下走了，不会卡住）
            None => {
                out.html.push_str(&rest[..at + 2]);
                rest = &rest[at + 2..];
                continue;
            }
        };

        // 三个属性之一、且看着像本地文件，才去解它。
        // **`<script>` / `<style>` 里的不算** —— 那是内容不是属性（`::js` 的字符串、
        // `::html js` 的原样 HTML 都从这里经过），照原样留着。
        let resolved = if raw == Raw::None {
            ATTRIBUTES
                .contains(&identifier_before(rest, at))
                .then(|| resolve(value, context))
        } else {
            None
        };

        out.html.push_str(&rest[..at + 2]);
        match resolved {
            Some(Ok((from, to))) => {
                let relative = to
                    .strip_prefix(context.assets_dir)
                    .expect("目标就在 assets 里")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.wanted.push(Wanted { from, to });
                out.html.push_str(&format!(
                    "{}/{}",
                    context.assets_prefix,
                    url::encode_href(&relative)
                ));
            }
            Some(Err(Skip::NotOurs(reason))) => {
                tracing::debug!(value, reason, "这个地址不归我管");
                out.html.push_str(value);
            }
            Some(Err(Skip::Missing(value))) => {
                out.missing.push(value.clone());
                out.html.push_str(&value);
            }
            None => out.html.push_str(value),
        }
        out.html.push('"');

        // 从闭引号后面接着扫
        rest = &rest[at + 2 + value.len() + 1..];
    }
}

/// 扫描时在不在 `<script>` / `<style>` 里。
///
/// 脚本与样式的**内容**（`::js` / `::css` 块、`::html js` 的原样 HTML）里出现
/// `src="图.png"` 是字面量 —— `document.write('<img src="图.png">')` 里的那一处是
/// 一段程序的字符串，不是 HTML 属性。扫到它就给改写成 `../assets/…`、再拷一份文件，
/// 等于**改写了脚本的语义**（输出内容变了），而且拷出来的文件还指错了地方。
///
/// 只认成对的标签（`<script …>` 到 `</script>`、`<style …>` 到 `</style>`），大小写
/// 不敏感；开标签里的 `>` 之后的才算内容，所以 `<script src="x">` 自己的属性不在此列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Raw {
    /// 不在任何一块里 —— 正常扫属性
    None,
    /// 在 `<script>…</script>` 里
    Script,
    /// 在 `<style>…</style>` 里
    Style,
}

/// 喂一段 HTML，翻 [`Raw`] 的状态。只数成对的开关标签，不嵌套。
///
/// 找 `<` 之后的标签名，逐位置看是不是 `<script` / `</script` / `<style` / `</style`
/// 之一（大小写不敏感）。只认**标签名后面**是空白、`>`、`/` 或行尾的（`<scriptx>` 不是
/// `<script>`，`<scripted>` 也不是）。
fn update_raw(state: &mut Raw, chunk: &str) {
    let mut at = 0usize;
    while at < chunk.len() {
        let Some(rel) = chunk[at..].find('<') else {
            return;
        };
        let start = at + rel;
        let rest = &chunk[start + 1..];
        // 开标签 `<script`；闭标签 `</script` 先剥那个 `/`
        let (name, closing) = match rest.strip_prefix('/') {
            Some(tail) => (tag_name(tail), true),
            None => (tag_name(rest), false),
        };
        match (name, closing) {
            (Some("script"), false) => *state = Raw::Script,
            (Some("script"), true) => {
                if *state == Raw::Script {
                    *state = Raw::None;
                }
            }
            (Some("style"), false) => *state = Raw::Style,
            (Some("style"), true) => {
                if *state == Raw::Style {
                    *state = Raw::None;
                }
            }
            _ => {}
        }
        at = start + 1;
    }
}

/// 从 `<`（或 `</`）之后那一段取出标签名（小写）。名字后必须是空白、`>`、`/` 或行尾，
/// 否则不是那个标签（`<scripted>` 不是 `<script>`）。只认 `script` / `style` 两个，
/// 别的名字一律 `None`。
fn tag_name(after: &str) -> Option<&'static str> {
    let name_end = after
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_alphanumeric())
        .map(|(at, _)| at)
        .unwrap_or(after.len());
    let (name, tail) = after.split_at(name_end);
    if name.is_empty() {
        return None;
    }
    let boundary_ok = tail.chars().next().is_none_or(|ch| {
        matches!(ch, ' ' | '\t' | '\n' | '\r' | '>' | '/')
    });
    if !boundary_ok {
        return None;
    }
    if name.eq_ignore_ascii_case("script") {
        Some("script")
    } else if name.eq_ignore_ascii_case("style") {
        Some("style")
    } else {
        None
    }
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
    if !source.is_file() {
        return Err(Skip::Missing(decoded.clone()));
    }
    let target = context.assets_dir.join(relative);
    Ok((source, target))
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

/// `="` 之前那几个字母是什么属性名
fn identifier_before(text: &str, at: usize) -> &str {
    let head = &text[..at];
    let start = head
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
        .map(|position| position + 1)
        .unwrap_or(0);
    &head[start..]
}

/// 引号里那一段（到闭引号为止）
fn value_of(text: &str, from: usize) -> Option<&str> {
    let rest = &text[from..];
    let end = rest.find('"')?;
    Some(&rest[..end])
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
            r#"<a class="wikilink" href="另一页/">站内</a>"#,
        );
        let scan = scan(html, &context(&pages, &pages, &assets));
        assert_eq!(scan.html, html, "不该动的动了");
        assert!(scan.wanted.is_empty(), "{:?}", scan.wanted);
        assert!(scan.missing.is_empty(), "{:?}", scan.missing);
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

    /// 只有**成对**的 `<script …>` 才进「脚本区」—— 不闭合的 `<script>` 一路吞下去，
    /// 与 HTML 解析器的行为一致（脚本内容到 `</script>` 为止）
    #[test]
    fn script_tag_boundaries_are_paired() {
        let mut state = Raw::None;
        // 开 → 进脚本
        update_raw(&mut state, "<script>");
        assert_eq!(state, Raw::Script);
        // 关 → 出来
        update_raw(&mut state, "</script>");
        assert_eq!(state, Raw::None);
        // 大写、带属性都认
        update_raw(&mut state, "<SCRIPT type=\"x\">");
        assert_eq!(state, Raw::Script);
        update_raw(&mut state, "</SCRIPT>");
        assert_eq!(state, Raw::None);
        // `<scriptx>` 不是 `<script>` —— 名字后那个边界字符得对
        update_raw(&mut state, "<scripted>");
        assert_eq!(state, Raw::None, "<scripted> 不该进脚本区");
        // `<style>` 与 `<script>` 各归各
        update_raw(&mut state, "<style>");
        assert_eq!(state, Raw::Style);
        update_raw(&mut state, "</style>");
        assert_eq!(state, Raw::None);
    }
}
