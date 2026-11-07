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

use crate::support::url::encode_href;

use tracing::info;

mod links;
mod paths;

pub use links::Links;
use paths::strip_suffix;
pub use paths::{assets_href, check_key, href, normalize, output_of};

#[cfg(test)]
mod tests;

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

    /// 这一页在站点里的**地址路径**（`指南/入门/`，以 `/` 结尾；首页是空串）
    ///
    /// 与 [`href`] 一样是目录式的：产物是 `<键>/index.html`，于是地址是 `<键>/`。
    /// 首页落在站点根上，没有自己那一层，所以是空串。
    pub fn url_path(&self) -> String {
        if self.key == INDEX_KEY {
            return String::new();
        }
        format!("{}/", encode_href(&self.key))
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
#[derive(Debug, Default, Clone)]
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
        for mut page in found {
            // 键最终会变成产物里的路径。先校验再算 `output` —— 文件名里带
            // `a\..\..\x` 这种写法时，拼出来的路径能跑到站点目录外面去。
            check_key(&page.key).map_err(|why| format!("{}：{why}", page.source.display()))?;
            page.output = output_of(&page.key);
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

    /// 往站点里加一页**不是从 `pages/` 扫出来的**页（目录页之类）。
    ///
    /// 键撞了现有的（或者撞了之前加进来的）就报错 —— 生成页不该默默
    /// 盖住作者自己写的那一页。
    pub fn insert(&mut self, mut page: Page) -> Result<(), String> {
        // 生成页的键来自 `mdpp.toml` 的 `toc`，同样是配置里的一行字，
        // 同样不能让它把产物写到站点目录外面。
        check_key(&page.key)?;
        page.output = output_of(&page.key);
        let key = normalize(&page.key).unwrap_or_default();
        if self.by_key.contains_key(&key) {
            return Err(format!(
                "站点里已经有「{}」这一页了 —— 目录页起个别的名字（mdpp.toml 的 toc）",
                page.key
            ));
        }
        let at = self.pages.len();
        self.by_key.insert(key.clone(), at);
        let stem = key.rsplit('/').next().unwrap_or(&key).to_string();
        self.by_stem.entry(stem).or_default().push(at);
        self.pages.push(page);
        Ok(())
    }

    /// 撞键的页（键）
    pub fn duplicates(&self) -> &[String] {
        &self.duplicates
    }

    /// `pages/` 下不是页面的文件（相对 `pages/`）
    pub fn others(&self) -> &[String] {
        &self.others
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
        // **不跟随符号链接**：链接指向哪里是链接自己的事，而产物路径是按
        // 它在这一层看见的名字拼的 —— 跟过去就可能把 `pages/` 外面的东西
        // 纳进站点。要共享内容就放真目录，或者把共享目录放在 `pages/` 里面。
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            others.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            continue;
        }
        if meta.is_dir() {
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
    // 用 `Path::components` 而不是对整条字符串做 `replace('\\', "/")`：
    // 在 Unix 上反斜杠是普通文件名字符，替换之后 `a\..\..\x` 会变成
    // `a/../../x`，键就带上了 `..`。这里只收正常段，段里还剩反斜杠的，
    // 交给 [`check_key`] 拒绝。
    let mut segments: Vec<String> = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => {
                segments.push(part.to_string_lossy().into_owned());
            }
            // CurDir / ParentDir / RootDir / Prefix 都不该出现在
            // `strip_prefix` 之后；出现了就说明这条路不正常，不当页面收。
            _ => return None,
        }
    }
    let key = segments.join("/");
    Some(strip_suffix(&key).to_string())
}
