use std::path::PathBuf;

use crate::project::shown;

#[derive(Debug, Default, Clone)]
pub(super) struct Broken {
    /// 名字没注册的块有几个
    pub(super) unknown: usize,
    /// 认识但用错了的块有几个
    pub(super) misused: usize,
    /// 出问题的页（键）
    pub(super) pages: Vec<String>,
}

/// 一趟 `build` 的结果 —— 报告与 `serve` 的「重出」那一行都要读它
#[derive(Debug, Default)]
pub struct Built {
    /// 出到哪儿了
    pub site_dir: PathBuf,
    /// 页数
    pub pages: usize,
    /// 进了 `assets/` 的资源数
    pub resources: usize,
    /// 红链（`[[目标]]` 指不到站内某一页）
    pub links: Vec<String>,
    /// 块有问题（名字没注册 / 用错了）的那些页
    pub broken: Vec<String>,
    /// 上面那几页里的块问题**各几个**：名字没注册的、参数用错了的。
    ///
    /// 报告要分着说 —— 「不认识这个名字」与「名字对、写法不对」是两件事，
    /// 顺着哪一条去改也完全不同
    pub broken_unknown: usize,
    pub broken_misused: usize,
    /// 正文引了而 `pages/` 里没有的文件
    pub absent: Vec<String>,
    /// 撞了键的页
    pub duplicates: Vec<String>,
    /// `pages/` 下没被引用的文件
    pub idle: Vec<String>,
}

impl Built {
    /// 有没有问题（`--strict` 看的就是这个）
    pub fn has_problems(&self) -> bool {
        !(self.links.is_empty() && self.broken.is_empty() && self.absent.is_empty())
    }

    /// 「N 页，M 个资源」—— 报告与 `serve` 那一行都用它
    pub fn summary(&self) -> String {
        format!("{} 页，{} 个资源", self.pages, self.resources)
    }
}

impl Broken {
    pub(super) fn add(&mut self, page: &crate::project::site::Page, html: &str) {
        // 类名由 `block::dispatch` 一处写死，数法在库那边（`mdpp::count_problem_boxes`）——
        // 这里是 CLI，不抄第二份
        let (unknown, misused) = mdpp::count_problem_boxes(html);
        if unknown == 0 && misused == 0 {
            return;
        }
        self.unknown += unknown;
        self.misused += misused;
        self.pages.push(page.key.clone());
    }
}

/// 报告一律走**标准错误** —— 与其余子命令一样，stdout 是产物要去的地方。
///
/// 只读 [`Built`]：它已经装好了要说的每一句，`serve` 重出时拿的是同一份。
pub fn report(built: &Built) {
    eprintln!("站点建在 {}（{}）", shown(&built.site_dir), built.summary());
    if built.pages == 0 {
        eprintln!("正文目录里还没有页面 —— `mdpp new 甲` 建一页试试");
    }

    if !built.links.is_empty() {
        eprintln!("红链 {} 条（站内没有这样的页）：", built.links.len());
        for one in &built.links {
            eprintln!("  {one}");
        }
    }

    if !built.broken.is_empty() {
        eprintln!(
            "块有问题 {} 个（名字没注册 {} 个、参数用错 {} 个），在这些页：",
            built.broken_unknown + built.broken_misused,
            built.broken_unknown,
            built.broken_misused
        );
        for key in &built.broken {
            eprintln!("  {key}");
        }
    }

    if !built.absent.is_empty() {
        eprintln!("正文引了这些文件，而 pages/ 里没有：");
        for one in &built.absent {
            eprintln!("  {one}");
        }
    }

    if !built.duplicates.is_empty() {
        eprintln!(
            "这几页撞了同一个键，只渲染了先排到的那一份：{}",
            built.duplicates.join("、")
        );
    }

    if !built.idle.is_empty() {
        eprintln!(
            "pages/ 下这些文件没被任何一页引用，没有进站点：{}",
            built.idle.join("、")
        );
    }

    if !built.has_problems() {
        eprintln!("没有红链，也没有块问题。");
    }
}
