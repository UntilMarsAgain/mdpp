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

const NOJEKYLL: &str = ".nojekyll";

mod clean;
mod files;
mod nav;
mod page;
mod report;
mod toc;

pub use report::{Built, report};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::cli::options::Build;
use crate::project::assets;
use crate::project::site::{Missing, Page, Site, assets_href, output_of};
use crate::project::{self, Project, ensure_dir, shown};
use crate::render::context::template_pages;
use crate::render::page as shell;
use crate::render::page::ExternalScript;

use mdpp::Mdpp;
use tracing::{debug, info};

use clean::{check_clean, clean, real_path, staging_dir, swap_into_place};
use files::{copy_override, favicon, logo};
use nav::{nav_for, resolve_nav};
use page::{PageBody, chrome, effective_jobs, generated_body, render_page};
use report::Broken;

pub fn run(options: &Build) -> Result<(), String> {
    let project = Project::open()?;
    // 用真实路径做换名：`build/` 若是指向别处的符号链接，临时目录要放在同一个
    // 文件系统上，`rename` 才是原子的。
    let site_dir = real_path(&project.site_dir());

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

    // 目录页：配置里 `toc` 那一页（骨架默认 `contents`），生成后插进站点 ——
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
    let assets = shell::copy_assets(site_dir)?;
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
        let assets_dir = site_dir.join(shell::ASSET_DIR);
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
        let document = shell::document(&shell::Page {
            title: &title,
            lang: project.config.lang.as_deref(),
            theme: Some(project.config.theme.as_str()),
            chrome: chrome(project, &parser, &nav, &logo),
            canonical: shell::Canonical {
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
            toc: &body.sidebar,
            extra_css: &extra_css,
            extra_js: &extra_js,
            assets: shell::Assets::Linked(assets_prefix.clone()),
            external_script,
        });
        project::write_file(&site_dir.join(&page.output), &document)?;
        debug!(page = %page.key, out = %page.output.display(), "一页");
    }

    // 覆盖层：把 `override/` 里的东西原样拷到产物里，重复就覆盖
    if let Some(override_dir) = &project.config.override_dir {
        copy_override(override_dir, &project.root, site_dir)?;
    }

    let mut links: Vec<String> = missing
        .lock()
        .map(|list| {
            list.iter()
                .map(|one| format!("{} → [[{}]]（{}）", one.page, one.target, one.why))
                .collect()
        })
        .unwrap_or_default();
    // 并行渲染时 `missing` 的入账顺序取决于线程；报告要可复现就按字符串排
    links.sort();
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
fn templates_of(project: &Project) -> Result<Option<mdpp::block::template::TemplatePages>, String> {
    let dir = project.templates_dir();
    if dir.is_dir() {
        Ok(Some(template_pages(&dir)?))
    } else {
        info!(dir = %dir.display(), "没有模板页目录，使用库自带标准库");
        Ok(None)
    }
}

/// 读配置里附的那几个 CSS / JS
fn read_all(paths: &[PathBuf], root: &Path) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|path| {
            let full = project::under(root, path);
            crate::support::limits::read_text(&full)
        })
        .collect()
}

#[cfg(test)]
mod tests;
