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

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::context::template_pages;
use crate::options::Build;
use crate::page;
use crate::page::{ExternalScript, NavLink};
use crate::project::assets;
use crate::project::config::NavEntry;
use crate::project::site::{
    INDEX_KEY, Links, Missing, Page, Site, assets_href, href, normalize, output_of,
};
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

pub fn run(options: &Build) -> Result<(), String> {
    let project = Project::open()?;
    let site_dir = project.site_dir();

    // 先验最终目录：它要能安全地成为站点根。**验完不删** —— 这一版先出到
    // 同级的临时目录，全部成功了才换名；失败时旧站点一个字都不动。
    check_clean(&project, &site_dir, options.allow_outside)?;
    let staging = staging_dir(&site_dir)?;
    let staging_options = Build {
        // 临时目录里可能有上一次崩溃留下的半截；先清掉再出
        clean: true,
        strict: options.strict,
        external_script: options.external_script.clone(),
        jobs: options.jobs,
        allow_outside: options.allow_outside,
    };
    let mut built = match build_into(&project, &staging_options, &staging) {
        Ok(built) => built,
        Err(error) => {
            // 失败的那一遍不换名，临时目录也不留 —— 下一次构建从干净的地方开始
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    built.site_dir = site_dir.clone();
    report(&built);

    // `--strict`：把「有问题」变成非零退出码，好挂进 CI。
    // 这一条要在换名之前 —— 失败的那一遍不该把旧站点顶掉。
    if options.strict && built.has_problems() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "有问题没解决（--strict）：红链 {} 条，块问题 {} 页，资源缺 {} 个；\
             旧站点没有替换",
            built.links.len(),
            built.broken.len(),
            built.absent.len()
        ));
    }

    // 全部写好之后才换名。备份 → 新目录换名 → 删备份；中间任何一步失败都尽量还原。
    swap_into_place(&staging, &site_dir)?;
    Ok(())
}

/// 出到最终目录**同级的临时目录**：同一个文件系统，`rename` 才是原子的。
fn staging_dir(site_dir: &Path) -> Result<PathBuf, String> {
    let parent = site_dir
        .parent()
        .ok_or_else(|| format!("site_dir（{}）没有上一级目录", shown(site_dir)))?;
    let name = site_dir
        .file_name()
        .ok_or_else(|| format!("site_dir（{}）不像一个目录名", shown(site_dir)))?;
    Ok(parent.join(format!(
        ".{}.mdpp-tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    )))
}

/// 把出好的临时目录换成正式目录。
///
/// 顺序：旧目录先改名到备份 → 临时目录改名到正式 → 删备份。第二步失败时把
/// 备份改回去，于是「失败」不会留下半个站点，也不会把旧站点弄丢。
fn swap_into_place(staging: &Path, site_dir: &Path) -> Result<(), String> {
    if !staging.is_dir() {
        return Err(format!("临时产物目录不在（{}）", shown(staging)));
    }
    let parent = site_dir
        .parent()
        .ok_or_else(|| format!("site_dir（{}）没有上一级目录", shown(site_dir)))?;
    let name = site_dir
        .file_name()
        .ok_or_else(|| format!("site_dir（{}）不像一个目录名", shown(site_dir)))?;
    let backup = parent.join(format!(
        ".{}.mdpp-old-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    remove_any(&backup);

    let had_old = site_dir.exists();
    if had_old {
        std::fs::rename(site_dir, &backup)
            .map_err(|error| format!("挪不开旧站点 {}：{error}", shown(site_dir)))?;
    }
    if let Err(error) = std::fs::rename(staging, site_dir) {
        // 换名失败：把旧站点挪回去，再报错
        if had_old {
            let _ = std::fs::rename(&backup, site_dir);
        }
        return Err(format!(
            "换不了站点目录 {}：{error}（旧站点已尽量还原）",
            shown(site_dir)
        ));
    }
    if had_old {
        remove_any(&backup);
    }
    Ok(())
}

/// 删一个文件或目录；不关心它是什么、也不在意它已经不在了
fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// 一页渲染完的产物（还没套外壳、还没落盘）。
struct PageBody {
    title: String,
    html: String,
    assets: Vec<mdpp::Asset>,
    wanted: Vec<assets::Wanted>,
    absent: Vec<String>,
}

/// 读一页正文 → 渲染 → 扫地址，**不写盘**。
///
/// 写盘与记账留在外面串行做：这样多线程只碰解析器与只读数据，资源文件不会
/// 被两页同时写。`Mdpp` 现在是 `Send + Sync`，一个解析器可以被所有工作线程共享。
#[allow(clippy::too_many_arguments)]
fn render_page(
    project: &Project,
    site: &Arc<Site>,
    pages_dir: &Path,
    site_dir: &Path,
    parser: &Mdpp,
    templates: &Option<mdpp::block::template::TemplatePages>,
    missing: &Arc<Mutex<Vec<Missing>>>,
    page: &Page,
    assets_prefix: &str,
) -> Result<PageBody, String> {
    let source = crate::limits::read_text(&page.source)?;
    let mut context = RenderContext::new().with_links(Arc::new(Links::for_page(
        site.clone(),
        page,
        missing.clone(),
    )));
    if let Some(templates) = templates {
        context = context.with_templates(templates.clone());
    }

    // 站点模式：::css / ::js / ::html js 输出链接，资源随 rendered.assets 返回
    let rendered = parser.render_with_assets(&source, &context, assets_prefix);
    let scan = assets::scan(
        &rendered.html,
        &assets::Context {
            page_dir: page.source.parent().unwrap_or(pages_dir),
            pages_dir,
            assets_dir: &site_dir.join(page::ASSET_DIR),
            assets_prefix,
        },
    );
    Ok(PageBody {
        title: document_title(project, &source, page.stem()),
        html: scan.html,
        assets: rendered.assets,
        wanted: scan.wanted,
        absent: scan
            .missing
            .iter()
            .map(|value| format!("{} → {value}", page.key))
            .collect(),
    })
}

/// 目录页的正文是现成生成的，不读文件
fn generated_body(project: &Project, site: &Site, page: &Page) -> PageBody {
    PageBody {
        title: document_title(project, "", page.stem()),
        html: toc_body(site, &project.config.site_name, &page.key),
        assets: Vec::new(),
        wanted: Vec::new(),
        absent: Vec::new(),
    }
}

/// 实际用几个线程：`0` = 按 CPU 核数，且不超过页数
fn effective_jobs(requested: usize, pages: usize) -> usize {
    let requested = if requested == 0 {
        std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
    } else {
        requested
    };
    requested.min(pages).max(1)
}

/// 出一遍站点到**给定的那个目录**。
///
/// 目录是参数而不是项目自己的 `site_dir`：`serve` 出到临时目录，不去动用户手
/// 里的 `build/`（预览到一半的东西不该被当成产物）。
///
/// 与 [`run`] 分开也是为了能直接测：项目根是给定的，不必去动进程当前目录
/// （测试是并发的，动它就有串味的风险）。
pub fn build_into(project: &Project, options: &Build, site_dir: &Path) -> Result<Built, String> {
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
        clean(project, site_dir, options.allow_outside)?;
    }
    ensure_dir(site_dir)?;

    let parser = Mdpp::new();
    let mut site = Arc::new(site);
    let missing: Arc<Mutex<Vec<Missing>>> = Arc::new(Mutex::new(Vec::new()));

    // mermaid / math 前端脚本的加载方式
    let external_script = match options.external_script.as_str() {
        "none" => ExternalScript::None,
        _ => ExternalScript::Cdn,
    };

    // 目录页：配置里 `toc` 那一页（默认「目录」），生成后插进站点 ——
    // 导航解析与站内链接都能指到它，报告里也数得进页数
    let toc_key = project
        .config
        .toc
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if !toc_key.is_empty() {
        Arc::make_mut(&mut site).insert(Page {
            source: PathBuf::new(),
            key: toc_key.clone(),
            output: output_of(&toc_key),
        })?;
    }

    // 导航：配置里那几项 + 自动补上的目录按钮，对着站点查好相对地址
    let nav = resolve_nav(&project.config.nav, &toc_key, &site)?;

    // 资源从二进制里拷到产物里：页面指的是 `assets/mdpp.css`，那些字节得真的在那儿。
    // 每个页面都内联一遍的话，几十个页面就是几十份拷贝
    let assets = page::copy_assets(site_dir)?;
    info!(assets, "资源已拷进产物");

    // 标签页上那个图标：也是一页资源，一样落在 `assets/` 里。
    // 页面上的地址按各自的深度加前缀，所以这里存的是**相对站点根**的那一份
    let favicon = favicon(project, site_dir)?;

    // 顶栏里站点名旁边那个图标：与 favicon 同一条路（同一种配置、同一处落地）
    let logo = logo(project, site_dir)?;

    // 一个空文件，作用是让 GitHub Pages **别跑 Jekyll**。
    //
    // Jekyll 把 `_` 开头或叫 `.git` 的目录当内部数据结构处理，**静默丢掉**它们 ——
    // 于是 `pages/_草稿/某页.mdpp` 出了 `build/_草稿/某页/index.html`，推上去却
    // 404，而且没有任何报错。别的静态托管不认这个文件，多这一个字节无害。
    project::write_file(&site_dir.join(NOJEKYLL), "")?;

    // 先把每一页的正文并出来（可以并行），再串行落盘。
    //
    // 并行只做「读 → 解析 → 扫地址」：这一段是 CPU 大头，而且只碰只读数据；
    // 写文件、拷资源、记账留在下面串行，避免两页同时写同一个 `assets/` 文件。
    let pages = site.pages();
    let jobs = effective_jobs(options.jobs, pages.len());
    let mut bodies: Vec<Option<Result<PageBody, String>>> =
        (0..pages.len()).map(|_| None).collect();

    if jobs <= 1 {
        for (index, page) in pages.iter().enumerate() {
            let body = if page.source.as_os_str().is_empty() {
                Ok(generated_body(project, &site, page))
            } else {
                render_page(
                    project,
                    &site,
                    &pages_dir,
                    site_dir,
                    &parser,
                    &templates,
                    &missing,
                    page,
                    &assets_href(&page.key),
                )
            };
            bodies[index] = Some(body);
        }
    } else {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let slots = Mutex::new(&mut bodies);
        std::thread::scope(|scope| {
            for _ in 0..jobs {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(page) = pages.get(index) else {
                            break;
                        };
                        let body = if page.source.as_os_str().is_empty() {
                            Ok(generated_body(project, &site, page))
                        } else {
                            render_page(
                                project,
                                &site,
                                &pages_dir,
                                site_dir,
                                &parser,
                                &templates,
                                &missing,
                                page,
                                &assets_href(&page.key),
                            )
                        };
                        slots.lock().expect("工作线程的锁不该中毒")[index] = Some(body);
                    }
                });
            }
        });
    }

    let mut absent: Vec<String> = Vec::new();
    let mut copied: Vec<PathBuf> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    let mut broken = Broken::default();

    for (index, page) in pages.iter().enumerate() {
        let body = bodies[index].take().expect("每一页都该有一个结果")?;

        // 资源落盘：`assets/mdpp-<hash>.css|js`，已存在则跳过（多页共享同一份）
        let assets_dir = site_dir.join(page::ASSET_DIR);
        if !body.assets.is_empty() {
            std::fs::create_dir_all(&assets_dir)
                .map_err(|error| format!("建不了 {}：{error}", assets_dir.display()))?;
            for asset in &body.assets {
                let path = assets_dir.join(asset.file_name());
                if !path.exists() {
                    std::fs::write(&path, &asset.content)
                        .map_err(|error| format!("写不了 {}：{error}", path.display()))?;
                }
            }
        }

        assets::copy(&body.wanted, &mut copied).map_err(|error| format!("拷不了资源：{error}"))?;
        for one in &body.wanted {
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
        absent.extend(body.absent);
        broken.add(page, &body.html);

        let assets_prefix = assets_href(&page.key);
        let title = body.title;
        let body_html = body.html;
        let nav = nav_for(&nav, &page.key);
        // 顶栏那个图标也是同一套地址：按这一页的深度加前缀（`../../assets/…`）
        let logo = logo
            .as_deref()
            .map(|name| format!("{assets_prefix}/{name}"))
            .unwrap_or_default();
        let document = page::document(&page::Page {
            title: &title,
            lang: project.config.lang.as_deref(),
            theme: Some(project.config.theme.as_str()),
            chrome: chrome(project, &parser, &nav, &logo),
            canonical: page::Canonical {
                root: project.config.base_url.clone(),
                path: page.url_path(),
            },
            // 嵌套页里的图标地址也要数对层数（`../../assets/favicon.svg`）——
            // `favicon` 存的是相对站点根的那一份
            favicon: favicon
                .as_deref()
                .map(|name| format!("{assets_prefix}/{name}"))
                .as_deref(),
            body: &body_html,
            extra_css: &extra_css,
            extra_js: &extra_js,
            assets: page::Assets::Linked(assets_prefix.clone()),
            external_script,
        });
        project::write_file(&site_dir.join(&page.output), &document)?;
        debug!(page = %page.key, out = %page.output.display(), "一页");
    }

    // 覆盖层：把 `override/` 里的东西原样拷到产物里，重复就覆盖
    if let Some(override_dir) = &project.config.override_dir {
        copy_override(override_dir, &project.root, site_dir)?;
    }

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
        broken_unknown: broken.unknown,
        broken_misused: broken.misused,
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
    copy_asset_into(
        project,
        site_dir,
        project.config.favicon.as_deref(),
        "favicon",
    )
}

/// 顶栏里、站点名旁边那个图标：与 [`favicon`] 同一条路 —— 同一个配置形状、同样拷进
/// `assets/`、同样给出「相对站点根」的那一份地址。
fn logo(project: &Project, site_dir: &Path) -> Result<Option<String>, String> {
    copy_asset_into(project, site_dir, project.config.logo.as_deref(), "logo")
}

/// 把配置里指的一个文件拷进产物的 `assets/`，给出它的**文件名**（相对站点根）。
///
/// `favicon` 与 `logo` 走的是同一条路：读进来、按字节拷进 `assets/`、只把文件名
/// 交出去（页面按各自深度加前缀，见 [`assets_href`]）。两份各写一份的话，
/// 「读不到就报错」与「按字节拷而不是按文本读」都要各写一次 —— 而后者正是当初
/// favicon 只认 `.svg`、`.png` 直接失败的那个坑。
///
/// **按字节拷**：这两个字段指的都是图片，`.png` / `.ico` / `.gif` 是二进制，
/// `read_to_string` 读它们会因「不是合法 UTF-8」失败，于是最常见的用法反而用不了。
fn copy_asset_into(
    project: &Project,
    site_dir: &Path,
    configured: Option<&std::path::Path>,
    what: &str,
) -> Result<Option<String>, String> {
    let Some(configured) = configured else {
        return Ok(None);
    };
    let from = under(&project.root, configured);
    let name = configured
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{what} 不是个文件名字：{}", shown(configured)))?;
    let to = site_dir.join(page::ASSET_DIR).join(&name);
    if let Some(parent) = to.parent() {
        ensure_dir(parent)?;
    }
    std::fs::copy(&from, &to)
        .map_err(|error| format!("读不了 {what} {}：{error}", shown(&from)))?;
    // 只给**文件名**：配置里写的是项目里的相对路径，而产物里它在 `assets/` 下
    Ok(Some(crate::url::encode_href(&name)))
}

/// 顶栏与底栏那几句话 —— 配置里那几个单行字段，原样递过去。
/// `nav` 是这一页要用的那一份（每页里「哪一项是当前页」不一样）；
/// `logo` 是站点名旁边那个图标的地址（**已带好这一页的深度前缀**）
///
/// `footer` 会用 `parser` 做一遍 mdpp 解析，于是 `footer` 里能写 `::note`、
/// `[[内部链接]]`、`**粗体**` 之类的 mdpp 语法；解析后的 HTML 存进
/// `copyright_html`，原始文本仍留在 `copyright` 里作为兜底。
fn chrome(project: &Project, parser: &Mdpp, nav: &[NavLink], logo: &str) -> page::Chrome {
    let footer_raw = project.config.footer.clone().unwrap_or_default();
    let footer_html = if footer_raw.trim().is_empty() {
        None
    } else {
        Some(parser.render(&footer_raw))
    };
    page::Chrome {
        name: project.config.site_name.clone(),
        logo: logo.to_string(),
        description: project.config.site_description.clone().unwrap_or_default(),
        repo_url: project.config.repo_url.clone().unwrap_or_default(),
        copyright: footer_raw,
        copyright_html: footer_html,
        // `check()` 已经筛过这个值，这里是把十六进制变成**能算的东西**（字色、
        // 深色那一档都要当场算）。theme_color 总有值（默认 "#2f6fd0"），不存在
        // 缺失的情况，直接解析即可
        theme_color: crate::color::Color::parse(&project.config.theme_color),
        nav: nav.to_vec(),
    }
}

/// 导航栏解析：配置里那几项 + 自动补上的目录按钮，对着站点查好**相对这一页的地址**。
///
/// 导航是「站点级」的（各页都同一条），只有「哪一项是当前页」随页变 —— 所以这里先
/// 解析出一份「键 → 目标页键」的清单，等到渲染某一页时再配上相对地址与当前项
/// （见 [`nav_for`]）。
///
/// 解析不出来的**报错**而不是当红链报：导航是作者手写的配置，写错了该当场知道，
/// 不该让整站的导航悄悄少一项。
fn resolve_nav(
    configured: &[NavEntry],
    toc_key: &str,
    site: &Site,
) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();

    for entry in configured {
        let (label, target) = match entry {
            NavEntry::SameName(label) => {
                // 字符串：显示文字就是「页名」，按 [[名字]] 那套规则找
                let key = resolve_target(label, site).ok_or_else(|| {
                    format!(
                        "nav 里的「{label}」站内没有这样的页 —— 写 `{{ \"{label}\" = \"页的键\" }}` 那样指定"
                    )
                })?;
                (label.clone(), key)
            }
            NavEntry::Named(map) => {
                let (label, key) = map.iter().next().expect("check 里验过只有一项");
                let target = normalize(key)
                    .ok_or_else(|| format!("nav 里 {label:?} → {key:?} 不是个站内页的键"))?;
                crate::project::site::check_key(&target)?;
                let page = site
                    .by_key(&target)
                    .ok_or_else(|| format!("nav 里 {label:?} → {key:?} 站内没有这样的页"))?;
                (label.clone(), page.key.clone())
            }
        };
        out.push((label.trim().to_string(), target));
    }

    // 目录页：生成的那一页自动在导航末尾补一个按钮，除非作者已经指过它
    if !toc_key.is_empty() && !out.iter().any(|(_, target)| target == toc_key) {
        out.push((toc_key.to_string(), toc_key.to_string()));
    }

    Ok(out)
}

/// 按 [[名字]] 那套规则找一个页：键名、或全站唯一叫这名字的页。
/// 认不出就是 `None`（报错的活交给 [`resolve_nav`]）
fn resolve_target(label: &str, site: &Site) -> Option<String> {
    let key = normalize(label)?;
    if let Some(page) = site.by_key(&key) {
        return Some(page.key.clone());
    }
    let candidates = site.by_stem(&key);
    match candidates.as_slice() {
        [page] => Some(page.key.clone()),
        _ => None,
    }
}

/// 一页的导航：把「站点级」的那份清单配上相对地址与当前项。
/// `from` 是当前页的键（算相对地址用）
fn nav_for(list: &[(String, String)], from: &str) -> Vec<NavLink> {
    list.iter()
        .map(|(label, target)| NavLink {
            label: label.clone(),
            href: href(from, target),
            active: from == target,
        })
        .collect()
}

/// 目录页的正文：全站每一页一个链接，配着它的键当说明。
/// 首页显示站点名（没有站点名就叫「首页」），别的页显示页名
fn toc_body(site: &Site, site_name: &str, toc_key: &str) -> String {
    let mut items = String::new();
    for page in site.pages() {
        let label = if page.key == INDEX_KEY {
            let name = site_name.trim();
            if name.is_empty() {
                "首页".to_string()
            } else {
                name.to_string()
            }
        } else {
            page.stem().to_string()
        };
        items.push_str(&format!(
            "<li><a href=\"{}\">{}</a> <span class=\"toc__path\">{}</span></li>\n",
            page::escape(&href(toc_key, &page.key)),
            page::escape(&label),
            page::escape(&page.key),
        ));
    }
    format!("<ul class=\"toc\">\n{items}</ul>")
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
/// 甚至去删项目外面的一整个目录。所以这里不是「检查一下」，而是：
///
/// - 默认**必须**严格位于项目根之内，且不能是正文/模板目录或它们的子目录；
/// - 无论谁调用，都不能是项目根、正文、模板的**祖先**，不能是 `/`、`$HOME`、
///   系统临时目录本身；
/// - 比较之前先做词法归一 + 已存在部分的符号链接解析；比较的是真路径，不是字面。
///
/// `allow_outside` 只给 `serve` 用（它出到系统临时目录里的预览缓存）。命令行
/// 上的 `build --clean` 永远拿不到这个开关。
fn check_clean(project: &Project, site_dir: &Path, allow_outside: bool) -> Result<(), String> {
    let site_dir = real_path(site_dir);
    let root = real_path(&project.root);
    let pages = real_path(&project.pages_dir());
    let templates = real_path(&project.templates_dir());

    if !allow_outside {
        if site_dir == root || !site_dir.starts_with(&root) {
            return Err(format!(
                "site_dir（{}）必须在项目目录（{}）里面，且不能就是项目根 —— \
                 `--clean` 会先清空它",
                shown(&site_dir),
                shown(&root)
            ));
        }
        for (what, keep) in [("正文", &pages), ("模板", &templates)] {
            if site_dir == *keep || site_dir.starts_with(keep) {
                return Err(format!(
                    "site_dir（{}）不能是{what}目录或它的子目录 —— 清空它会连内容一起删掉",
                    shown(&site_dir)
                ));
            }
        }
    }

    // 这几种位置，就算调用方允许「项目外」也不该清。
    if site_dir.parent().is_none() {
        return Err(format!(
            "site_dir（{}）是根目录，拒绝清空",
            shown(&site_dir)
        ));
    }
    for (what, keep) in [("项目根", &root), ("正文", &pages), ("模板", &templates)] {
        if keep.starts_with(&site_dir) {
            return Err(format!(
                "site_dir（{}）是{what}目录的上一层 —— 清空它会连内容一起删掉",
                shown(&site_dir)
            ));
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if real_path(&home) == site_dir {
            return Err("site_dir 指向家目录，拒绝清空".to_string());
        }
    }
    if real_path(&std::env::temp_dir()) == site_dir {
        return Err("site_dir 指向系统临时目录本身，拒绝清空".to_string());
    }
    Ok(())
}

/// 先验再删：`serve` 的临时目录与 `build_into` 的直写路径都走这里
fn clean(project: &Project, site_dir: &Path, allow_outside: bool) -> Result<(), String> {
    check_clean(project, site_dir, allow_outside)?;
    if site_dir.exists() {
        std::fs::remove_dir_all(site_dir)
            .map_err(|error| format!("清不了 {}：{error}", shown(site_dir)))?;
        debug!(dir = shown(site_dir), "已清空产物目录");
    }
    Ok(())
}

/// 词法归一 + 已存在部分的符号链接解析。
///
/// `a/../b` 与 `b` 要比较成同一个东西；`build` 是指向别处的符号链接时，
/// 也要按它真正指向的地方比较。不存在的尾部按字面接回去 —— 那正是
/// `--clean` 要建出来的目录。
fn real_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut lexical = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other.as_os_str()),
        }
    }
    // 已存在的最深一层做 canonicalize，剩下的尾部接回去
    let mut probe = lexical.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !probe.exists() {
        match probe.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                probe.pop();
            }
            None => break,
        }
    }
    let mut out = std::fs::canonicalize(&probe).unwrap_or(probe);
    for name in tail.iter().rev() {
        out.push(name);
    }
    out
}

/// 读配置里附的那几个 CSS / JS
fn read_all(paths: &[PathBuf], root: &Path) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|path| {
            let full = project::under(root, path);
            crate::limits::read_text(&full)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::site::{INDEX_KEY, output_of};
    use std::collections::BTreeMap;

    /// 一个小站点：首页 + 两页，`指南/入门` 与 `手册`
    fn site_of(files: &[&str]) -> (tempfile::TempDir, Site) {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        for file in files {
            let path = dir.path().join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("建得出目录");
            }
            std::fs::write(&path, "# 页").expect("写得下");
        }
        let site = Site::collect(dir.path()).expect("扫得动");
        (dir, site)
    }

    /// 生成的目录页：`insert` 进去之后导航与站内链接都指得到它
    #[test]
    fn an_inserted_page_lands_in_the_site() {
        let (_dir, mut site) = site_of(&["index.mdpp", "指南/入门.mdpp"]);
        site.insert(Page {
            source: PathBuf::new(),
            key: "目录".to_string(),
            output: output_of("目录"),
        })
        .expect("插得进");
        assert!(site.by_key("目录").is_some(), "{:?}", site.pages());
        assert_eq!(site.pages().len(), 3);

        // 撞了已有的键就报错 —— 生成页不该默默盖住作者自己写的页
        let mut again = site.clone();
        let error = again
            .insert(Page {
                source: PathBuf::new(),
                key: "目录".to_string(),
                output: output_of("目录"),
            })
            .expect_err("重复键该报错");
        assert!(error.contains("目录"), "{error}");
    }

    /// 导航解析：字符串按 `[[名字]]` 那套规则，单项表按键；目录按钮自动补在末尾
    #[test]
    fn nav_resolves_against_the_site_and_appends_the_toc_button() {
        let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp", "手册.md"]);
        let resolved = resolve_nav(
            &[
                NavEntry::SameName("手册".to_string()),
                NavEntry::Named(BTreeMap::from([(
                    "指南".to_string(),
                    "指南/入门".to_string(),
                )])),
            ],
            "目录",
            &site,
        )
        .expect("都指得到");
        assert_eq!(
            resolved,
            vec![
                ("手册".to_string(), "手册".to_string()),
                ("指南".to_string(), "指南/入门".to_string()),
                // 自动补上的目录按钮
                ("目录".to_string(), "目录".to_string()),
            ]
        );

        // 作者已经指过目录页，就不再自动补 —— 补两个按钮就重了
        let mut site = site;
        site.insert(Page {
            source: PathBuf::new(),
            key: "目录".to_string(),
            output: output_of("目录"),
        })
        .expect("插得进");
        let resolved = resolve_nav(
            &[NavEntry::Named(BTreeMap::from([(
                "全部页面".to_string(),
                "目录".to_string(),
            )]))],
            "目录",
            &site,
        )
        .expect("指得到");
        assert_eq!(resolved.len(), 1, "{resolved:?}");

        // 指不到的就报错，报错里带着那一项
        let error = resolve_nav(&[NavEntry::SameName("没有这页".to_string())], "", &site)
            .expect_err("该报错");
        assert!(error.contains("没有这页"), "{error}");
        let error = resolve_nav(
            &[NavEntry::Named(BTreeMap::from([(
                "别名".to_string(),
                "指南/不存在".to_string(),
            )]))],
            "",
            &site,
        )
        .expect_err("该报错");
        assert!(error.contains("指南/不存在"), "{error}");
    }

    /// 一页的导航：地址是相对那一页的，当前页只多一个 `aria-current`
    #[test]
    fn nav_for_marks_the_current_page_and_uses_relative_hrefs() {
        let list = vec![
            ("首页".to_string(), INDEX_KEY.to_string()),
            ("手册".to_string(), "手册".to_string()),
        ];
        let on_manual = nav_for(&list, "手册");
        assert_eq!(on_manual.len(), 2);
        // 手册 → 首页：退一层到根
        assert_eq!(on_manual[0].href, "../");
        assert!(!on_manual[0].active);
        // 手册 → 手册：退一层再进目录 —— 页与页之间永远是相对地址
        assert_eq!(on_manual[1].href, "../手册/");
        assert!(on_manual[1].active);

        let on_index = nav_for(&list, INDEX_KEY);
        // 首页 → 首页：地址是当前目录自己
        assert_eq!(on_index[0].href, "./");
        assert!(on_index[0].active);
        // 首页 → 手册：直接进目录
        assert_eq!(on_index[1].href, "手册/");
        assert!(!on_index[1].active);
    }

    /// 目录页的正文：每一页一个链接，首页显示站点名
    #[test]
    fn the_toc_body_lists_every_page() {
        let (_dir, site) = site_of(&["index.mdpp", "指南/入门.mdpp"]);
        let body = toc_body(&site, "桥梁笔记", "目录");
        assert!(body.contains("<ul class=\"toc\">"), "{body}");
        assert!(body.contains(">桥梁笔记<"), "首页该显示站点名：{body}");
        assert!(body.contains(">入门<"), "{body}");
        assert!(body.contains("指南/入门"), "键该陪着链接显示：{body}");
        // 链接是相对目录页的：目录页在 `build/目录/`，回首页要退一层
        assert!(body.contains(r#"href="../""#), "{body}");
        assert!(body.contains(r#"href="../指南/入门/""#), "{body}");
    }
}
