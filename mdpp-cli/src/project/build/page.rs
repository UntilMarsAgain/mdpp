use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::project::assets;

use crate::project::Project;
use crate::project::site::{Links, Missing, Page, Site};
use crate::render::page;
use crate::render::page::NavLink;
use mdpp::{Mdpp, RenderContext};

use super::nav::toc_body;

pub(super) struct PageBody {
    pub(super) title: String,
    pub(super) html: String,
    pub(super) assets: Vec<mdpp::Asset>,
    pub(super) wanted: Vec<assets::Wanted>,
    pub(super) absent: Vec<String>,
}

/// 读一页正文 → 渲染 → 扫地址，**不写盘**。
///
/// 写盘与记账留在外面串行做：这样多线程只碰解析器与只读数据，资源文件不会
/// 被两页同时写。`Mdpp` 现在是 `Send + Sync`，一个解析器可以被所有工作线程共享。
#[allow(clippy::too_many_arguments)]
pub(super) fn render_page(
    project: &Project,
    site: &Arc<Site>,
    pages_dir: &Path,
    site_dir: &Path,
    parser: &Mdpp,
    templates: &mdpp::block::template::TemplatePages,
    missing: &Arc<Mutex<Vec<Missing>>>,
    page: &Page,
    assets_prefix: &str,
) -> Result<PageBody, String> {
    let source = crate::support::limits::read_text(&page.source)?;
    let context = RenderContext::new()
        .with_max_template_depth(project.config.max_template_depth)
        .with_links(Arc::new(Links::for_page(
            site.clone(),
            page,
            missing.clone(),
        )))
        .with_templates(templates.clone());

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
pub(super) fn generated_body(project: &Project, site: &Site, page: &Page) -> PageBody {
    PageBody {
        title: document_title(project, "", page.stem()),
        html: toc_body(site, &project.config.site_name, &page.key),
        assets: Vec::new(),
        wanted: Vec::new(),
        absent: Vec::new(),
    }
}

/// 实际用几个线程：`0` = 按 CPU 核数，且不超过页数
pub(super) fn effective_jobs(requested: usize, pages: usize) -> usize {
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
pub(super) fn chrome(
    project: &Project,
    parser: &Mdpp,
    nav: &[NavLink],
    logo: &str,
) -> page::Chrome {
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
        theme_color: crate::support::color::Color::parse(&project.config.theme_color),
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
pub(super) fn document_title(project: &Project, source: &str, fallback: &str) -> String {
    let title = first_heading(source).unwrap_or_else(|| fallback.to_string());
    match project.title_suffix() {
        Some(site) => format!("{title} —— {site}"),
        None => title,
    }
}

/// 第一个 `# ` 开头的行，跳过围栏代码块内部
pub(super) fn first_heading(source: &str) -> Option<String> {
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
