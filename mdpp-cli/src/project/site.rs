//! 站点：`pages/` 扫出来的那一组页，以及 `[[目标]]` 的站内地址。
//!
//! 这是**红链唯一能派上用场的地方**。渲染单篇笔记时 `[[入门]]` 指到哪无所谓 ——
//! 命令行没有「那一页存不存在」这回事，于是红链只是「没配前缀」的另一种说法。
//! 而在一个项目里，`pages/` 就是全部的真相：这一页写 `[[入门]]` 是**说这一页应该存在**，
//! 而它不存在就该被指出来 —— 那是作者少写了一页，不是工具的毛病。
//!
//! ## 目标怎么落到某一页
//!
//! 按顺序试两种写法，都不中才是红链：
//!
//! 1. **整条路径**：`pages/指南/入门.mdpp` 写成 `[[指南/入门]]`（后缀可省，`/` 与 `\` 都认）；
//! 2. **文件名**：`[[入门]]` —— 全站只有一个 `入门` 时就是它；有**好几个**时不猜，
//!    当红链并在报告里把那几处都列出来（猜错一个链接比缺一个链接糟得多）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mdpp::link::{LinkResolver, LinkTarget, ResolvedLink};

use tracing::{debug, info};

/// 首页的键。输出上它是 `build/index.html`，而别的页靠 `[[index]]` 指回来
pub const INDEX_KEY: &str = "index";

/// 算成页的后缀。`.mdpp` 是本方言的写法，`.md` 是普通 markdown —— 两者都算页面
pub const PAGE_SUFFIXES: [&str; 2] = ["mdpp", "md"];

/// 站点里的一页
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// 源文件（完整路径）
    pub source: PathBuf,
    /// 键：相对 `pages/`、去掉后缀、`/` 分隔。`pages/指南/入门.mdpp` 是 `指南/入门`
    pub key: String,
    /// 输出文件（相对 `site_dir`）。永远是 `<键>/index.html`
    pub output: PathBuf,
}

impl Page {
    /// 显示用的名字：键的最后一段（`指南/入门` → `入门`）
    pub fn stem(&self) -> &str {
        self.key.rsplit('/').next().unwrap_or(&self.key)
    }
}

/// 一处红链：**哪一页**的哪个目标，以及**为什么**没落到某一页
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// 出问题的页（键）
    pub page: String,
    /// 正文里原样写的目标
    pub target: String,
    /// 找不到 / 有歧义 —— 后者要给出候选，否则作者无从下手
    pub why: String,
}

/// 站点：页表 + 查表
#[derive(Debug, Default)]
pub struct Site {
    pages: Vec<Page>,
    /// 归一后的键 → 页下标
    by_key: HashMap<String, usize>,
    /// 归一后的文件名 → 页下标（一个文件名可能对应好几页）
    by_stem: HashMap<String, Vec<usize>>,
    /// 撞了键的页（`a.md` 与 `a.mdpp` 并存），报告里要点出来
    duplicates: Vec<String>,
    /// `pages/` 下**不是**页面的文件（图片之类）。这一版不把它们拷进站点，
    /// 但要报出来 —— 静悄悄地略过会让作者以为图也在站点里
    others: Vec<String>,
}

impl Site {
    /// 递归扫 `pages_dir`，收下 `.mdpp` 与 `.md`。
    ///
    /// 次序是排过的（先按键，再按后缀）—— 目录里的顺序是不定的，而输出与
    /// 「哪一页胜出」都得稳定，否则同一个项目两次 build 会不一样。
    pub fn collect(pages_dir: &Path) -> Result<Site, String> {
        let mut found: Vec<Page> = Vec::new();
        let mut others: Vec<String> = Vec::new();
        walk(pages_dir, pages_dir, &mut found, &mut others)
            .map_err(|error| format!("扫不了 {}：{error}", pages_dir.display()))?;
        others.sort();
        // `.mdpp` 优先于 `.md`：同一个键出现两次时，生效的是本方言那一份
        found.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then_with(|| suffix_rank(&left.source).cmp(&suffix_rank(&right.source)))
        });

        let mut site = Site {
            others,
            ..Site::default()
        };
        for page in found {
            let key = normalize(&page.key).unwrap_or_default();
            let at = site.pages.len();
            if site.by_key.contains_key(&key) {
                site.duplicates.push(page.key.clone());
                continue;
            }
            site.by_key.insert(key.clone(), at);
            let stem = key.rsplit('/').next().unwrap_or(&key).to_string();
            site.by_stem.entry(stem).or_default().push(at);
            site.pages.push(page);
        }

        info!(pages = site.pages.len(), "扫到的页面");
        for duplicate in &site.duplicates {
            // warn 而不是 debug：两份同名页时后者永远不会被渲染，作者得知道
            tracing::warn!(page = %duplicate, "两页撞了同一个键，只渲染了先排到的那一页");
        }
        Ok(site)
    }

    /// 全部页，按扫描次序
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// 撞键的页（键）
    pub fn duplicates(&self) -> &[String] {
        &self.duplicates
    }

    /// `pages/` 下不是页面的文件（相对 `pages/`）
    pub fn others(&self) -> &[String] {
        &self.others
    }

    /// 一页都没有
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// 按归一后的键查
    pub fn by_key(&self, key: &str) -> Option<&Page> {
        self.by_key.get(key).map(|at| &self.pages[*at])
    }

    /// 按文件名查。返回**全部**候选 —— 有好几个时要交给作者决定，而不是替他挑一个
    pub fn by_stem(&self, stem: &str) -> Vec<&Page> {
        self.by_stem
            .get(stem)
            .map(|list| list.iter().map(|at| &self.pages[*at]).collect())
            .unwrap_or_default()
    }
}

/// `.mdpp` 排前，`.md` 排后
fn suffix_rank(path: &Path) -> usize {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("mdpp") => 0,
        _ => 1,
    }
}

/// 递归收页。目录项的顺序是不定的，排序放到 [`Site::collect`] 里统一做
fn walk(
    root: &Path,
    dir: &Path,
    into: &mut Vec<Page>,
    others: &mut Vec<String>,
) -> std::io::Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // 隐藏的一律不收：`.git` 里没有页面，而 `.` 开头的目录多半是工具生成的
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, into, others)?;
            continue;
        }
        match key_of(root, &path) {
            Some(key) => into.push(Page {
                output: output_of(&key),
                source: path,
                key,
            }),
            // 不是页面的记下来：这一版不把它们拷进站点，但作者该知道自己放了张图
            None => others.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            ),
        }
    }
    Ok(())
}

/// 相对 `pages/` 的键：去掉后缀，换成 `/`
fn key_of(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let is_page = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| PAGE_SUFFIXES.contains(&ext));
    if !is_page {
        return None;
    }
    Some(strip_suffix(&relative.to_string_lossy()).replace('\\', "/"))
}

/// 砍掉末尾的页面后缀（带点，比大小写不敏感 —— 归一本来就要转小写）。
/// 不是那两个之一就原样留着：`2.0.说明` 里的 `.说明` 不该被当成后缀砍掉
fn strip_suffix(text: &str) -> &str {
    PAGE_SUFFIXES
        .iter()
        .find_map(|suffix| {
            let cut = text.len().checked_sub(suffix.len() + 1)?;
            // 先比一比再切：`cut` 未必落在字符边界上（`入门` 的长度正好让 `.mdpp`
            // 那次尝试切在字的中间），而 `text[..cut]` 会当场 panic
            let dotted = format!(".{suffix}");
            text.get(cut..)
                .filter(|tail| tail.eq_ignore_ascii_case(&dotted))
                .map(|_| &text[..cut])
        })
        .unwrap_or(text)
}

/// 输出文件：`<键>/index.html`。
///
/// **首页是个例外**：`index` 落在 `build/index.html`，而不是
/// `build/index/index.html` —— 后者会让站点根上多出一层没人访问得到的目录。
fn output_of(key: &str) -> PathBuf {
    let mut out = PathBuf::new();
    if key != INDEX_KEY {
        for segment in key.split('/').filter(|segment| !segment.is_empty()) {
            out.push(segment);
        }
    }
    out.push("index.html");
    out
}

/// 一个链接目标的**归一**写法：拿来查表的那把钥匙。
///
/// 宽松到能容下人手写出来的东西，严格到不会认错：`\` 当 `/`、前后空白、
/// 前导 `./` 与 `/`、结尾的 `/`，以及 `.md` / `.mdpp` 后缀，一律去掉；比大小写不敏感。
/// 空串不算目标。
pub fn normalize(target: &str) -> Option<String> {
    let mut text = target.trim().replace('\\', "/");
    text = text.trim_start_matches('/').to_string();
    text = text.strip_prefix("./").unwrap_or(&text).to_string();
    text = strip_suffix(text.trim_end_matches('/')).to_string();
    (!text.is_empty()).then(|| text.to_lowercase())
}

/// `template:` 开头的目标是**模板页**，不是站内页面
pub fn is_template_target(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed
        .split_once(':')
        .is_some_and(|(prefix, _)| prefix.trim().eq_ignore_ascii_case("template"))
}

/// 从 `from`（一��的键）走到 `to` 的相对地址。
///
/// 每退一层加一个 `../`，目标总是以 `/` 结尾 —— 产物是**目录**，不是文件。
/// 相对地址而不是从站点根算的绝对地址：这样 `build/index.html` 拷到任何地方、
/// 挂在子路径下，链接都还对。
pub fn relative_href(from: &str, to: &str) -> String {
    let depth = from.rsplit_once('/').map_or(0, |(dir, _)| dir.split('/').count());
    let mut href = "../".repeat(depth);
    if to == INDEX_KEY {
        // 目录式输出下首页就是根：写 `../` 而不是冗余的 `../index/`
        if depth == 0 {
            href.push_str("./");
        }
        return href;
    }
    href.push_str(&encode_href(to));
    href.push('/');
    href
}

/// `href` 里不能原样出现的字符：`#` 会把地址截断，空格会被浏览器改写。
///
/// **中文原样留着** —— 浏览器与服务端都认 UTF-8 的路径，而百分号编码过的中文
/// 在站点里没法看、也没法手改。这与库那边 `targets::encode_target_spaces` 的取法一致：
/// 只编码真正碍事的那几个。
fn encode_href(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        let plain = !ch.is_ascii()
            || ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '.' | '_' | '/' | '~');
        if plain {
            out.push(ch);
        } else {
            let mut buffer = [0u8; 4];
            for byte in ch.encode_utf8(&mut buffer).as_bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

/// 站内的链接解析器：**每一页一份**（相对地址要看当前页在哪），
/// 但红链的账记在**全站一份**的表里。
pub struct Links {
    site: Arc<Site>,
    /// 当前页的键（算相对地址用）
    from: String,
    /// 全站的红链表
    missing: Arc<Mutex<Vec<Missing>>>,
}

impl Links {
    /// 给一页配一个解析器。红链记进 `missing`
    pub fn for_page(site: Arc<Site>, page: &Page, missing: Arc<Mutex<Vec<Missing>>>) -> Links {
        debug!(page = %page.key, "这一页的链接解析器");
        Links {
            site,
            from: page.key.clone(),
            missing,
        }
    }

    /// 记一笔红链。**锁不住就当没发生** —— 报告缺一条比 panic 强
    fn note(&self, target: &str, why: String) {
        if let Ok(mut list) = self.missing.lock() {
            let already = list
                .iter()
                .any(|one| one.page == self.from && one.target == target.trim());
            if !already {
                list.push(Missing {
                    page: self.from.clone(),
                    target: target.trim().to_string(),
                    why,
                });
            }
        }
    }
}

impl LinkResolver for Links {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        // 模板页不是站内的一页 —— 它是写模板的文件，不进站点。落成红链是对的，
        // 但那不是「少写了一页」，所以不进报告
        if is_template_target(target) {
            debug!(target, "指向模板页，不是站内页面");
            return None;
        }

        let key = normalize(target)?;

        if let Some(page) = self.site.by_key(&key) {
            return Some(self.link_to(page));
        }

        let candidates = self.site.by_stem(&key);
        match candidates.as_slice() {
            [page] => Some(self.link_to(page)),
            [] => {
                self.note(target, "站内没有这一页".to_string());
                None
            }
            _ => {
                let keys: Vec<&str> = candidates.iter().map(|page| page.key.as_str()).collect();
                self.note(target, format!("好几页都叫这个名字：{}", keys.join("、")));
                None
            }
        }
    }
}

impl Links {
    /// 落成一个带 `href` 的链接。顺带把**落到哪一页**记在属性上 ——
    /// `[[入门]]` 这种只写文件名的写法，光看正文看不出它指的是哪个目录下的那一页
    fn link_to(&self, page: &Page) -> ResolvedLink {
        ResolvedLink::Link(
            LinkTarget::new(relative_href(&self.from, &page.key)).attr("data-page", page.key.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pages(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        for file in files {
            let path = dir.path().join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("建得出目录");
            }
            std::fs::write(&path, "# 页").expect("写得下");
        }
        dir
    }

    fn site_of(files: &[&str]) -> (tempfile::TempDir, Site) {
        let dir = temp_pages(files);
        let site = Site::collect(dir.path()).expect("扫得动");
        (dir, site)
    }

    /// 键去掉后缀、换成 `/`，输出永远是 `<键>/index.html`
    #[test]
    fn pages_become_keys_and_index_files() {
        let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "手册.md"]);
        let keys: Vec<&str> = site.pages().iter().map(|page| page.key.as_str()).collect();
        assert_eq!(keys, vec!["index", "手册", "指南/入门"]);

        assert_eq!(
            site.by_key("指南/入门").map(|page| page.output.clone()),
            Some(PathBuf::from("指南/入门/index.html"))
        );
        assert_eq!(
            site.by_key("index").map(|page| page.output.clone()),
            Some(PathBuf::from("index.html"))
        );
    }

    /// 普通 markdown 也是页面 —— `.mdpp` 是本方言的写法，不是唯一被认的
    #[test]
    fn plain_markdown_counts_as_a_page_too() {
        let (_dir, site) = site_of(&["甲.md"]);
        assert_eq!(site.pages().len(), 1);
    }

    /// 后缀不对的不是页面，隐藏的目录也不收
    #[test]
    fn other_files_and_hidden_directories_are_skipped() {
        let (_dir, site) = site_of(&[
            "甲.mdpp",
            "笔记.txt",
            "没有后缀",
            ".git/config",
            ".草稿/被藏起来.mdpp",
        ]);
        let keys: Vec<&str> = site.pages().iter().map(|page| page.key.as_str()).collect();
        assert_eq!(keys, vec!["甲"]);
    }

    /// 同一个键出现两次时 `.mdpp` 胜出，并记一笔 —— 默默挑一个会让人以为另一份还在用
    #[test]
    fn the_same_key_twice_keeps_the_dialect_file() {
        let (_dir, site) = site_of(&["甲.md", "甲.mdpp"]);
        assert_eq!(site.pages().len(), 1);
        assert_eq!(site.pages()[0].source.extension().unwrap(), "mdpp");
        assert_eq!(site.duplicates(), ["甲"]);
    }

    /// 手写出来的几种写法都要落到同一页
    #[test]
    fn targets_are_matched_loosely() {
        for target in ["指南/入门", "./指南/入门", "指南\\入门", "指南/入门.mdpp"] {
            assert_eq!(normalize(target).as_deref(), Some("指南/入门"), "{target}");
        }
        // 只写文件名的那一条归一后仍是文件名 —— 落到哪一页由「全站唯一」那条规则管
        assert_eq!(normalize(" 入门 ").as_deref(), Some("入门"));
        assert_eq!(normalize("入门.md").as_deref(), Some("入门"));
        assert_eq!(normalize("A/B.MDPP").as_deref(), Some("a/b"));
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("   "), None);
    }

    #[test]
    fn template_targets_are_not_site_pages() {
        assert!(is_template_target("template:卡片"));
        assert!(is_template_target("Template: 卡片"));
        assert!(!is_template_target("卡片"));
        assert!(!is_template_target("帮助:入门"));
    }

    /// 每退一层加一个 `../`，目标总是以 `/` 结尾
    #[test]
    fn hrefs_are_relative_and_end_with_a_slash() {
        assert_eq!(relative_href("index", "指南/入门"), "指南/入门/");
        assert_eq!(relative_href("指南/入门", "index"), "../");
        assert_eq!(relative_href("指南/入门", "手册"), "../手册/");
        assert_eq!(
            relative_href("指南/深入/安装", "手册/入门"),
            "../../手册/入门/"
        );
        assert_eq!(relative_href("index", "index"), "./");
    }

    /// 键里的空格与 `#` 要编码；中文原样留着
    #[test]
    fn hrefs_are_percent_encoded() {
        assert_eq!(relative_href("index", "我的 页"), "我的%20页/");
        assert_eq!(relative_href("index", "a#b"), "a%23b/");
    }

    /// 解析器：认得出的给地址，认不出的记一笔红链
    #[test]
    fn the_resolver_links_inside_the_site_and_reports_the_rest() {
        let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "手册/安装.mdpp"]);
        let site = Arc::new(site);
        let missing = Arc::new(Mutex::new(Vec::new()));

        let index = site.by_key("index").expect("有首页").clone();
        let home = Links::for_page(site.clone(), &index, missing.clone());
        let html = home.resolve("指南/入门").expect("认得出");
        let ResolvedLink::Link(link) = html else {
            panic!("该给一个地址");
        };
        assert_eq!(link.url, "指南/入门/");
        assert!(home.resolve("没有这一页").is_none());
        assert!(missing.lock().expect("锁得住").len() == 1);

        let deep = site.by_key("指南/入门").expect("有那一页").clone();
        let inside = Links::for_page(site, &deep, missing.clone());
        let ResolvedLink::Link(link) = inside.resolve("手册/安装").expect("认得出") else {
            panic!("该给一个地址");
        };
        assert_eq!(link.url, "../手册/安装/");
    }

    /// `[[入门]]` 这种只写文件名的：全站只有一个时认，多个时**不猜**
    #[test]
    fn a_bare_file_name_works_only_when_it_is_unambiguous() {
        let (_dir, site) = site_of(&["指南/入门.mdpp", "手册/入门.mdpp"]);
        let site = Arc::new(site);
        let missing = Arc::new(Mutex::new(Vec::new()));

        let home = Page {
            source: PathBuf::from("index.mdpp"),
            key: "index".to_string(),
            output: output_of("index"),
        };
        let links = Links::for_page(site, &home, missing.clone());
        assert!(links.resolve("入门").is_none(), "重名时不该替作者挑一个");

        let report = missing.lock().expect("锁得住").clone();
        assert_eq!(report.len(), 1);
        assert!(report[0].why.contains("好几页"), "{report:?}");
        assert!(report[0].why.contains("手册/入门"), "{report:?}");
    }

    /// 只写文件名的那条要能落回**唯一**的那一页
    #[test]
    fn a_bare_file_name_finds_the_only_page_with_that_name() {
        let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp"]);
        let site = Arc::new(site);
        let missing = Arc::new(Mutex::new(Vec::new()));
        let page = site.by_key("index").expect("有首页").clone();

        let ResolvedLink::Link(link) = Links::for_page(site, &page, missing.clone())
            .resolve("入门")
            .expect("认得出")
        else {
            panic!("该给一个地址");
        };
        assert_eq!(link.url, "指南/入门/");
        assert_eq!(link.attrs, vec![("data-page".to_string(), "指南/入门".to_string())]);
    }

    /// 模板页不是站内页面 —— 落成红链，但**不进报告**
    #[test]
    fn template_targets_stay_out_of_the_report() {
        let (_dir, site) = site_of(&["index.mdpp"]);
        let site = Arc::new(site);
        let missing = Arc::new(Mutex::new(Vec::new()));
        let page = site.pages()[0].clone();
        let links = Links::for_page(site, &page, missing.clone());

        assert!(links.resolve("template:卡片").is_none());
        assert!(missing.lock().expect("锁得住").is_empty());
    }

    /// 同一页里同一个目标写两次只报一次 —— 报告里全是重复行的话，真正的漏网之鱼就看不见了
    #[test]
    fn the_same_target_twice_is_reported_once() {
        let (_dir, site) = site_of(&["index.mdpp"]);
        let site = Arc::new(site);
        let missing = Arc::new(Mutex::new(Vec::new()));
        let page = site.pages()[0].clone();
        let links = Links::for_page(site, &page, missing.clone());

        links.resolve("没有这一页");
        links.resolve("没有这一页");
        assert_eq!(missing.lock().expect("锁得住").len(), 1);
    }
}