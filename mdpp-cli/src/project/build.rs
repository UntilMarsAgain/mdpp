//! `mdpp build` —— 把 `pages/` 渲染成一个可静态部署的站点。
//!
//! ## 一趟下来做了什么
//!
//! 1. 找着项目（从当前目录向上找 `mdpp.toml`）；
//! 2. 扫 `pages/`，收下 `.mdpp` 与 `.md`（见 [`Site`]）；
//! 3. 每一页渲染一次，套上整页外壳，写到 `site_dir/<键>/index.html`；
//! 4. 报告里说清**出了什么事**：红链各在哪、哪些块不认识、`pages/` 里哪些文件没进站点。
//!
//! ## 产物为什么是目录而不是文件
//!
//! `pages/指南/入门.mdpp` → `build/指南/入门/index.html`。这样站内链接是**相对**的
//! `../手册/`，整站拷走、挂在子路径下、或者用浏览器直接打开 `build/index.html`，
//! 链接都还对 —— 而绝对路径 `/手册/` 只在「站点正好在域名根上」时才成立。
//!
//! ## 红链
//!
//! 站内链接解析器只认 `pages/` 里的页，认不出就是红链（渲染成 `wikilink--missing`，
//! 不带 `href`）。默认**只是报出来**，不让 build 失败 —— 写到还没写的页面是常态。
//! `--strict` 才把它变成非零退出码。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::context::template_pages;
use crate::options::Build;
use crate::page;
use crate::project::assets;
use crate::project::site::{Links, Missing, Site, assets_href};
use crate::project::{self, Project, ensure_dir, shown, under};

use mdpp::{Mdpp, RenderContext};
use tracing::{debug, info};

/// 让 GitHub Pages 跳过 Jekyll 的那个空文件的名字
const NOJEKYLL: &str = ".nojekyll";

/// 渲染时数出来的块问题
#[derive(Debug, Default, Clone)]
struct Broken {
    /// 名字没注册的块有几个
    unknown: usize,
    /// 认识但用错了的块有几个
    misused: usize,
    /// 出问题的页（键）
    pages: Vec<String>,
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

pub fn run(options: &Build) -> Result<(), String> {
    let project = Project::open()?;
    let site_dir = project.site_dir();
    let built = build_into(&project, options, &site_dir)?;
    report(&built);

    // `--strict`：把「有问题」变成非零退出码，好挂进 CI
    if options.strict && built.has_problems() {
        return Err(format!(
            "有问题没解决（--strict）：红链 {} 条，块问题 {} 页，资源缺 {} 个",
            built.links.len(),
            built.broken.len(),
            built.absent.len()
        ));
    }
    Ok(())
}

/// 出一遍站点到**给定的那个目录**。
///
/// 目录是参数而不是项目自己的 `site_dir`：`serve` 出到临时目录，不去动用户手
/// 里的 `build/`（预览到一半的东西不该被当成产物）。
///
/// 与 [`run`] 分开也是为了能直接测：项目根是给定的，不必去动进程当前目录
/// （测试是并发的，动它就有串味的风险）。
pub fn build_into(
    project: &Project,
    options: &Build,
    site_dir: &Path,
) -> Result<Built, String> {
    let pages_dir = project.pages_dir();
    if !pages_dir.is_dir() {
        return Err(format!(
            "正文目录不在（{}）\n要么这个项目还没建好，要么 mdpp.toml 里的 pages_dir 指错了",
            shown(&pages_dir)
        ));
    }

    let site = Site::collect(&pages_dir)?;
    let templates = templates_of(project)?;
    let extra_css = read_all(&project.config.extra_css, &project.root)?;
    let extra_js = read_all(&project.config.extra_js, &project.root)?;

    if options.clean {
        clean(project, site_dir)?;
    }
    ensure_dir(site_dir)?;

    let parser = Mdpp::new();
    let site = Arc::new(site);
    let missing: Arc<Mutex<Vec<Missing>>> = Arc::new(Mutex::new(Vec::new()));
    let mut broken = Broken::default();
    let mut absent = Vec::new();

    // 资源从二进制里拷到产物里：页面指的是 `assets/base.css`，那些字节得真的在那儿。
    // 每个页面都内联一遍的话，几十个页面就是几十份拷贝
    let assets = page::copy_assets(site_dir)?;
    info!(assets, "资源已拷进产物");

    // 标签页上那个图标：也是一页资源，一样落在 `assets/` 里。
    // 页面上的地址按各自的深度加前缀，所以这里存的是**相对站点根**的那一份
    let favicon = favicon(project, site_dir)?;

    // 一个空文件，作用是让 GitHub Pages **别跑 Jekyll**。
    //
    // Jekyll 把 `_` 开头或叫 `.git` 的目录当内部数据结构处理，**静默丢掉**它们 ——
    // 于是 `pages/_草稿/某页.mdpp` 出了 `build/_草稿/某页/index.html`，推上去却
    // 404，而且没有任何报错。别的静态托管不认这个文件，多这一个字节无害。
    project::write_file(&site_dir.join(NOJEKYLL), "")?;

    // 已经拷过的资源，记着，免得几十页引同一张图时拷几十回
    let mut copied: Vec<std::path::PathBuf> = Vec::new();
    // 被引用而进了 `assets/` 的那些（相对 pages/），只为最后那句报告
    let mut taken: Vec<String> = Vec::new();

    for page in site.pages() {
        let source = std::fs::read_to_string(&page.source)
            .map_err(|error| format!("读不了 {}：{error}", shown(&page.source)))?;

        let mut context = RenderContext::new().with_links(Arc::new(Links::for_page(
            site.clone(),
            page,
            missing.clone(),
        )));
        if let Some(templates) = &templates {
            context = context.with_templates(templates.clone());
        }

        let assets_prefix = assets_href(&page.key);
        let scan = assets::scan(
            &parser.render_with(&source, &context),
            &assets::Context {
                page_dir: page.source.parent().unwrap_or(&pages_dir),
                pages_dir: &pages_dir,
                assets_dir: &site_dir.join(page::ASSET_DIR),
                assets_prefix: &assets_prefix,
            },
        );
        for value in &scan.missing {
            absent.push(format!("{} → {value}", page.key));
        }
        assets::copy(&scan.wanted, &mut copied)
            .map_err(|error| format!("拷不了资源：{error}"))?;
        for one in &scan.wanted {
            let relative = one
                .from
                .strip_prefix(&pages_dir)
                .expect("资源都在正文目录里")
                .to_string_lossy()
                .replace('\\', "/");
            if !taken.contains(&relative) {
                taken.push(relative);
            }
        }

        let html = scan.html;
        broken.add(page, &html);

        let title = document_title(project, &source, page.stem());
        let document = page::document(&page::Page {
            title: &title,
            lang: &project.config.lang,
            theme: project.config.theme.as_deref(),
            chrome: chrome(project),
            canonical: page::Canonical {
                root: project.config.site_url.clone(),
                path: page.url_path(),
            },
            // 嵌套页里的图标地址也要数对层数（`../../assets/favicon.svg`）——
            // `favicon` 存的是相对站点根的那一份
            favicon: favicon
                .as_deref()
                .map(|name| format!("{assets_prefix}/{name}"))
                .as_deref(),
            body: &html,
            extra_css: &extra_css,
            extra_js: &extra_js,
            assets: page::Assets::Linked(assets_prefix.clone()),
        });
        project::write_file(&site_dir.join(&page.output), &document)?;
        debug!(page = %page.key, out = %page.output.display(), "一页");
    }

    // 覆盖层：把 `override/` 里的东西原样拷到产物里，重复就覆盖
    copy_override(&project.config.override_dir, &project.root, site_dir)?;

    let links: Vec<String> = missing
        .lock()
        .map(|list| {
            list.iter()
                .map(|one| format!("{} → [[{}]]（{}）", one.page, one.target, one.why))
                .collect()
        })
        .unwrap_or_default();
    info!(
        pages = site.pages().len(),
        links = links.len(),
        resources = copied.len(),
        "站点出完"
    );

    // `pages/` 下不是页面的文件有两种下场：被引用了就进了 `assets/`，剩下的才白放着
    let idle: Vec<String> = site
        .others()
        .iter()
        .filter(|relative| !taken.iter().any(|one| one == *relative))
        .cloned()
        .collect();

    Ok(Built {
        site_dir: site_dir.to_path_buf(),
        pages: site.pages().len(),
        resources: copied.len(),
        links,
        broken: broken.pages.clone(),
        absent,
        duplicates: site.duplicates().to_vec(),
        idle,
    })
}

/// 标签页上那个图标：拷进 `assets/`，给出的地址是相对站点根的那一份
/// （页面上按各自的深度加前缀）。
///
/// 写了却读不到就**报错** —— 那是一个笔误，而一个读不到的小图标只会静悄悄地
/// 让浏览器画一个默认的。
fn favicon(project: &Project, site_dir: &Path) -> Result<Option<String>, String> {
    let Some(configured) = &project.config.favicon else {
        return Ok(None);
    };
    let from = under(&project.root, configured);
    let name = configured
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("favicon 不是个文件名字：{}", shown(configured)))?;
    let to = site_dir.join(page::ASSET_DIR).join(&name);
    project::write_file(&to, &std::fs::read_to_string(&from).map_err(|error| {
        format!("读不了 favicon {}：{error}", shown(&from))
    })?)?;
    // 只给**文件名**：`favicon` 写的是项目里的相对路径，而产物里它在 `assets/` 下
    Ok(Some(assets::encode(&name)))
}

/// 顶栏与底栏那几句话 —— 配置里那几个单行字段，原样递过去
fn chrome(project: &Project) -> page::Chrome {
    page::Chrome {
        name: project.config.site_name.clone(),
        description: project.config.site_description.clone(),
        repo_url: project.config.repo_url.clone(),
        copyright: project.config.copyright.clone(),
    }
}

/// 模板页：目录不在就当没有模板页 —— 项目还没建那一半不该拦住 build
fn templates_of(project: &Project) -> Result<Option<mdpp::block::template::TemplatePages>, String> {
    let dir = project.templates_dir();
    if dir.is_dir() {
        Ok(Some(template_pages(&dir)?))
    } else {
        info!(dir = %dir.display(), "没有模板页目录");
        Ok(None)
    }
}

/// `--clean`：清空 `site_dir` 再出。
///
/// **`site_dir` 是配置里的一行字**，打错一个字就可能让这条命令去删整个项目 ——
/// 所以先确认它不在正文、模板与项目根里面，连「是它们的上一级」也不放过。
fn clean(project: &Project, site_dir: &Path) -> Result<(), String> {
    for keep in [
        project.root.clone(),
        project.pages_dir(),
        project.templates_dir(),
    ] {
        if keep.starts_with(site_dir) {
            return Err(format!(
                "site_dir（{}）不能是项目、正文或模板目录的**上一层** —— 清空它会连内容一起删掉",
                shown(site_dir)
            ));
        }
    }
    if site_dir.exists() {
        std::fs::remove_dir_all(site_dir)
            .map_err(|error| format!("清不了 {}：{error}", shown(site_dir)))?;
        debug!(dir = shown(site_dir), "已清空产物目录");
    }
    Ok(())
}

/// 读配置里附的那几个 CSS / JS
fn read_all(paths: &[PathBuf], root: &Path) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|path| {
            let full = project::under(root, path);
            std::fs::read_to_string(&full)
                .map_err(|error| format!("读不了 {}：{error}", shown(&full)))
        })
        .collect()
}

/// build 完成后，把 `override_dir` 里的东西原样拷到 `site_dir` 里。
/// 目录不存在就算了（默认配置里也写着 `override`，但用户未必建那个目录）。
/// 重复就覆盖 —— 可以往里放 `CNAME`、自定义 `robots.txt`、把某页换成手写的 HTML……
fn copy_override(override_dir: &Path, root: &Path, site_dir: &Path) -> Result<(), String> {
    let from = project::under(root, override_dir);
    if !from.exists() {
        return Ok(());
    }
    copy_dir_all(&from, site_dir).map_err(|error| format!("拷不了 override：{error}"))
}

/// 递归把一个目录里的所有文件拷到另一个目录里（保留目录结构，重复就覆盖）
fn copy_dir_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if !to.exists() {
        std::fs::create_dir_all(to)?;
    }
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir_all(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// `<title>`：页面里第一个 ATX 一级标题，没有就退回文件名。
///
/// 围栏代码块**跳过** —— `# 这是注释` 在 shell/python 代码块里太常见了，
/// 拿它当标题是明显错的。
fn document_title(project: &Project, source: &str, fallback: &str) -> String {
    let title = first_heading(source).unwrap_or_else(|| fallback.to_string());
    match project.title_suffix() {
        Some(site) => format!("{title} —— {site}"),
        None => title,
    }
}

/// 第一个 `# ` 开头的行，跳过围栏代码块内部
fn first_heading(source: &str) -> Option<String> {
    let mut fence: Option<char> = None;
    for line in source.lines() {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|ch| *ch == '`' || *ch == '~');
        if let Some(marker) = marker {
            let run = trimmed.chars().take_while(|ch| *ch == marker).count();
            if run >= 3 {
                fence = match fence {
                    Some(open) if open == marker => None,
                    None => Some(marker),
                    _ => fence,
                };
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        if let Some(text) = line.strip_prefix("# ") {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

impl Broken {
    fn add(&mut self, page: &crate::project::site::Page, html: &str) {
        // 那两个类名由 `block::dispatch` 一处写死，不会过期
        let unknown = html.matches("template--unknown").count();
        let misused = html.matches("template--problem").count();
        if unknown == 0 && misused == 0 {
            return;
        }
        self.unknown += unknown;
        self.misused += misused;
        self.pages.push(page.key.clone());
    }
}

/// 报告走**标准错误** —— 与其余子命令一样，stdout 是产物要去的地方
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
        eprintln!("块有问题（名字没注册，或参数用错了），在这些页：");
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
