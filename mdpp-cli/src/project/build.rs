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
use crate::project::site::{Links, Missing, Site, assets_href};
use crate::project::{self, Project, ensure_dir, shown};

use mdpp::{Mdpp, RenderContext};
use tracing::{debug, info};

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

pub fn run(options: &Build) -> Result<(), String> {
    let project = Project::open()?;
    build(&project, options)
}

/// 已经在项目里了，出一遍站点。
///
/// 与 [`run`] 分开是为了能直接测：项目根是给定的，不必去动进程当前目录
/// （测试是并发的，动它就有串味的风险）。
fn build(project: &Project, options: &Build) -> Result<(), String> {
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
        clean(project)?;
    }
    let site_dir = project.site_dir();
    ensure_dir(&site_dir)?;

    let parser = Mdpp::new();
    let site = Arc::new(site);
    let missing: Arc<Mutex<Vec<Missing>>> = Arc::new(Mutex::new(Vec::new()));
    let mut broken = Broken::default();

    // 资源从二进制里拷到产物里：页面指的是 `assets/base.css`，那些字节得真的在那儿。
    // 每个页面都内联一遍的话，几十个页面就是几十份拷贝
    let assets = page::copy_assets(&site_dir)?;
    info!(assets, "资源已拷进产物");

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

        let html = parser.render_with(&source, &context);
        broken.add(page, &html);

        let title = document_title(project, &source, page.stem());
        let document = page::document(&page::Page {
            title: &title,
            lang: &project.config.lang,
            theme: project.config.theme.as_deref(),
            chrome: chrome(project),
            body: &html,
            extra_css: &extra_css,
            extra_js: &extra_js,
            assets: page::Assets::Linked(assets_href(&page.key)),
        });
        project::write_file(&site_dir.join(&page.output), &document)?;
        debug!(page = %page.key, out = %page.output.display(), "一页");
    }

    let missing = missing.lock().map(|list| list.clone()).unwrap_or_default();
    info!(pages = site.pages().len(), missing = missing.len(), "站点出完");
    report(project, &site, &missing, &broken);

    // `--strict`：把「有问题」变成非零退出码，好挂进 CI
    if options.strict && !(missing.is_empty() && broken.pages.is_empty()) {
        return Err(format!(
            "有问题没解决（--strict）：红链 {} 条，块问题 {} 页",
            missing.len(),
            broken.pages.len()
        ));
    }
    Ok(())
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
fn clean(project: &Project) -> Result<(), String> {
    let site_dir = project.site_dir();
    for keep in [
        project.root.clone(),
        project.pages_dir(),
        project.templates_dir(),
    ] {
        if keep.starts_with(&site_dir) {
            return Err(format!(
                "site_dir（{}）不能是项目、正文或模板目录的**上一层** —— 清空它会连内容一起删掉",
                shown(&site_dir)
            ));
        }
    }
    if site_dir.exists() {
        std::fs::remove_dir_all(&site_dir)
            .map_err(|error| format!("清不了 {}：{error}", shown(&site_dir)))?;
        debug!(dir = shown(&site_dir), "已清空产物目录");
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
fn report(project: &Project, site: &Site, missing: &[Missing], broken: &Broken) {
    let site_dir = shown(&project.site_dir());
    eprintln!("站点建在 {site_dir}（{ } 页）", site.pages().len());
    if site.is_empty() {
        eprintln!("正文目录里还没有页面 —— `mdpp new 甲` 建一页试试");
    }

    if !missing.is_empty() {
        eprintln!("红链 {} 条（站内没有这样的页）：", missing.len());
        for one in missing {
            eprintln!("  {} → [[{}]]（{}）", one.page, one.target, one.why);
        }
    }

    if !broken.pages.is_empty() {
        eprintln!(
            "块有问题：未知块 {} 个、误用 {} 个，分布在 {} 页",
            broken.unknown,
            broken.misused,
            broken.pages.len()
        );
        for key in &broken.pages {
            eprintln!("  {key}");
        }
    }

    if !site.duplicates().is_empty() {
        eprintln!(
            "这几页撞了同一个键，只渲染了先排到的那一份：{}",
            site.duplicates().join("、")
        );
    }

    if !site.others().is_empty() {
        eprintln!(
            "pages/ 下这些文件不是页面，没有进站点（这一版不拷附件）：{}",
            site.others().join("、")
        );
    }

    if missing.is_empty() && broken.pages.is_empty() {
        eprintln!("没有红链，也没有块问题。");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Build, Init};

    /// 测试里站点名定死 —— 否则它会跟着临时目录的名字跑
    const TEST_SITE: &str = "测试站点";

    /// 建好骨架的项目，`pages/` 里再添几页
    fn project_with(pages: &[(&str, &str)]) -> (tempfile::TempDir, Project) {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let root = dir.path().join("站点");
        crate::project::init::run(&Init {
            dir: Some(root.clone()),
            name: Some(TEST_SITE.to_string()),
        })
        .expect("建得出");
        let project = Project::load(&root).expect("读得动");
        for (path, text) in pages {
            let full = project.pages_dir().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("建得出");
            }
            std::fs::write(&full, text).expect("写得下");
        }
        (dir, project)
    }

    fn build_ok(project: &Project) -> Result<(), String> {
        build(
            project,
            &Build {
                clean: false,
                strict: false,
            },
        )
    }

    /// 读某一页的产物。输出路径取自 [`Site`] 本身 —— 在测试里另算一遍的话，
    /// 「首页落在根上」那个例外就得抄两份
    fn output_of(project: &Project, key: &str) -> String {
        let site = Site::collect(&project.pages_dir()).expect("扫得动");
        let path = project.site_dir().join(
            &site
                .by_key(key)
                .unwrap_or_else(|| panic!("没有 {key} 这一页"))
                .output,
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("读不了 {}：{error}", path.display()))
    }

    #[test]
    fn a_project_builds_into_nested_index_files() {
        let (_dir, project) = project_with(&[("指南/入门.mdpp", "# 入门\n\n回 [[index]]。\n")]);
        build_ok(&project).expect("出得来");

        assert!(project.site_dir().join("index.html").is_file(), "首页没出");
        assert!(
            project.site_dir().join("指南/入门/index.html").is_file(),
            "嵌套页没出"
        );
        let html = output_of(&project, "指南/入门");
        assert!(html.contains("<h1"), "正文没渲染：{html}");
        // 回首页的那条要给站点根，不是 `./`（那会指回本页）
        assert!(html.contains(r#"href="/""#), "{html}");
    }

    /// 站内链接指到页，红链指不到 —— 这是「有项目」与「渲染单篇」最大的分别
    #[test]
    fn a_link_to_an_existing_page_is_resolved_and_the_rest_are_red() {
        let (_dir, project) = project_with(&[
            ("甲.mdpp", "# 甲\n\n[[乙]] 与 [[没有这一页]]\n"),
            ("乙.mdpp", "# 乙\n"),
        ]);
        build_ok(&project).expect("出得来");

        let html = output_of(&project, "甲");
        // 属性次序是库那边定的（`data-doc` 写在 `href` 前面），这里只关心有没有地址
        assert!(html.contains(r#"class="wikilink" data-doc="乙" href="乙/""#), "{html}");
        assert!(html.contains(r#"class="wikilink wikilink--missing""#), "{html}");
        assert!(html.contains(r#"data-missing="true""#), "{html}");
    }

    /// 嵌套页之间的相对地址要算对，否则站点挪个位置就全是死链
    #[test]
    fn links_between_nested_pages_are_relative() {
        let (_dir, project) = project_with(&[
            ("指南/深入/安装.mdpp", "# 安装\n\n[[手册]]\n"),
            ("手册.mdpp", "# 手册\n"),
            ("指南/入门.mdpp", "# 入门\n\n[[index]]\n"),
        ]);
        build_ok(&project).expect("出得来");

        assert!(output_of(&project, "指南/深入/安装").contains(r#"href="../../手册/""#));
        assert!(output_of(&project, "指南/入门").contains(r#"href="/""#));
    }

    /// 红链默认不拦住 build —— 写到还没写的页面是常态
    #[test]
    fn red_links_alone_still_build() {
        let (_dir, project) = project_with(&[("甲.mdpp", "# 甲\n\n[[还没写的那页]]\n")]);
        assert!(build_ok(&project).is_ok());
    }

    /// `--strict` 才把它变成失败 —— CI 里要的就��这一条
    #[test]
    fn strict_fails_on_a_red_link() {
        let (_dir, project) = project_with(&[("甲.mdpp", "# 甲\n\n[[还没写的那页]]\n")]);
        let message = build(
            &project,
            &Build {
                clean: false,
                strict: true,
            },
        )
        .expect_err("本该失败");
        assert!(message.contains("红链"), "{message}");
    }

    /// 块名写错了也要能让 `--strict` 失败 —— 那比红链更硬
    #[test]
    fn strict_fails_on_an_unknown_block() {
        let (_dir, project) = project_with(&[("甲.mdpp", "# 甲\n\n::根本没有这个块\n  内容\n")]);
        let message = build(
            &project,
            &Build {
                clean: false,
                strict: true,
            },
        )
        .expect_err("本该失败");
        assert!(message.contains("块问题"), "{message}");
    }

    /// `<title>` 取第一个一级标题，代码块里那行不算
    #[test]
    fn the_document_title_comes_from_the_first_heading() {
        let (_dir, project) = project_with(&[("甲.mdpp", "# 甲页\n")]);
        build_ok(&project).expect("出得来");
        assert!(
            output_of(&project, "甲").contains("<title>甲页 —— 测试站点</title>"),
            "{}",
            output_of(&project, "甲")
        );

        let (_dir2, project2) = project_with(&[("乙.mdpp", "```\n# 这是注释\n```\n\n没写标题\n")]);
        build_ok(&project2).expect("出得来");
        assert!(
            output_of(&project2, "乙").contains("<title>乙 —— 测试站点</title>"),
            "代码块里的 # 不该被当标题：{}",
            output_of(&project2, "乙")
        );
    }

    /// 没写站点名就不多出那半截标题
    #[test]
    fn a_site_without_a_name_gets_a_bare_title() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let root = dir.path().join("站点");
        crate::project::init::run(&Init {
            dir: Some(root.clone()),
            name: Some(TEST_SITE.to_string()),
        })
        .expect("建得出");
        std::fs::write(root.join("mdpp.toml"), "site_name = \"\"\n").expect("改得下");
        let project = Project::load(&root).expect("读得动");
        build_ok(&project).expect("出得来");
        assert!(output_of(&project, "index").contains("<title>测试站点</title>"));
    }

    /// `--clean` 去掉上一轮的残留；而它**不许**把项目本身清掉
    #[test]
    fn clean_removes_stale_output() {
        let (_dir, project) = project_with(&[("甲.mdpp", "# 甲\n")]);
        let stale = project.site_dir().join("上一轮/index.html");
        project::write_file(&stale, "<p>旧的</p>").expect("写得下");

        build(
            &project,
            &Build {
                clean: true,
                strict: false,
            },
        )
        .expect("出得来");
        assert!(!stale.exists(), "上一轮的残留还在");
        assert!(project.site_dir().join("甲/index.html").is_file());
    }

    /// `site_dir` 写错成项目根 / 正文目录时，`--clean` 必须拒绝 —— 它会去删东西
    #[test]
    fn clean_refuses_to_wipe_the_project_itself() {
        let (_dir, project) = project_with(&[]);
        for bad in ["site_dir = \".\"\n", "site_dir = \"pages\"\n"] {
            std::fs::write(project.root.join("mdpp.toml"), bad).expect("改得下");
            let broken = Project::load(&project.root).expect("读得动");
            let message = build(
                &broken,
                &Build {
                    clean: true,
                    strict: false,
                },
            )
            .expect_err("本该拒绝");
            assert!(message.contains("site_dir"), "{message}");
        }
    }

    /// 模板页从 `templates/` 取；不在那儿时只是没有模板页，不该拦住 build
    #[test]
    fn templates_come_from_the_project_directory() {
        let (_dir, project) = project_with(&[("甲.mdpp", "# 甲\n\n::卡片 标题=首页\n")]);
        build_ok(&project).expect("出得来");
        assert!(output_of(&project, "甲").contains("首页"));

        std::fs::remove_dir_all(project.templates_dir()).expect("删得掉");
        build_ok(&project).expect("没有模板页目录也该出得来");
    }

    /// 正文目录不在要说清是哪个目录 —— 「build 失败」没人查得动
    #[test]
    fn a_missing_pages_directory_says_where_it_looked() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "pages_dir = \"根本没有这个\"\n",
        )
        .expect("写得下");
        let project = Project::load(dir.path()).expect("读得动");
        let message = build_ok(&project).unwrap_err();
        assert!(message.contains("根本没有这个"), "{message}");
    }

    /// 站点页面的外壳：顶栏（站点名 + 那句话说明 + 仓库链接）与底栏（copyright）。
    /// 那几句话**全是纯文本**，配置里写什么就摆什么
    #[test]
    fn a_built_page_gets_the_site_shell() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let root = dir.path().join("站点");
        crate::project::init::run(&crate::options::Init {
            dir: Some(root.clone()),
            name: Some(TEST_SITE.to_string()),
        })
        .expect("建得出");
        std::fs::write(
            root.join("mdpp.toml"),
            "site_name = \"测试站点\"\n\
             site_description = \"桥梁与河\"\n\
             repo_url = \"https://example.com/notes\"\n\
             copyright = \"© 某人\"\n",
        )
        .expect("改得下");
        let project = Project::load(&root).expect("读得动");

        build_ok(&project).expect("出得来");
        let html = output_of(&project, "index");

        // 顶栏：站点名 + 说明 + 仓库链接
        assert!(html.contains("<span class=\"mdpp-bar__title\">测试站点</span>"), "{html}");
        assert!(html.contains("<span class=\"mdpp-bar__desc\">桥梁与河</span>"), "{html}");
        assert!(html.contains(r#"<a class="mdpp-bar__repo" href="https://example.com/notes""#), "{html}");
        // 底栏
        assert!(html.contains("<span class=\"mdpp-foot__text\">© 某人</span>"), "{html}");
        assert!(html.contains("由 mdpp 生成"), "{html}");
        // 顶栏在正文之上、底栏在正文之下
        assert!(html.find("mdpp-bar").expect("有顶栏") < html.find("mdpp-content").expect("有正文"));
        assert!(html.find("mdpp-content").expect("有正文") < html.find("mdpp-foot").expect("有底栏"));
    }

    /// 没写 copyright 就没有底栏 —— 与 mkdocs 一个规矩，空的那句不必摆在那儿
    #[test]
    fn no_copyright_means_no_bottom_bar() {
        let (_dir, project) = project_with(&[]);
        build_ok(&project).expect("出得来");
        let html = output_of(&project, "index");
        assert!(!html.contains("mdpp-foot"), "{html}");
    }

    /// 站点那份外壳样式只在 build 里 —— 单文件页面里不该有那一列
    #[test]
    fn the_built_site_is_the_only_one_with_the_shell() {
        let (_dir, project) = project_with(&[("指南/安装.mdpp", "# 安装\n")]);
        build_ok(&project).expect("出得来");

        let html = output_of(&project, "index");
        assert!(html.contains(r#"href="assets/site.css""#), "{html}");
        let css = std::fs::read_to_string(project.site_dir().join("assets/site.css"))
            .expect("资源没拷出来");
        assert!(css.contains("max-width: 1080px"), "限宽不见了：{css}");
        assert!(css.contains(".mdpp-bar"), "顶栏不见了：{css}");

        // 深层页的资源路径要**数对层数** —— 少一个 `../`，浏览器会把 404 的样式
        // 当成空样式，页面于是光着身子出来，而且**不报错**
        let nested = output_of(&project, "指南/安装");
        assert!(
            nested.contains(r#"href="../../assets/base.css""#),
            "深层页的资源路径不对：{nested}"
        );
        assert!(
            nested.contains(r#"src="../../assets/mdpp.js""#),
            "深层页的脚本路径不对：{nested}"
        );
    }

    /// 产物是**能直接部署**的一堆文件：页面外链资源，而资源真的在那儿
    #[test]
    fn the_output_is_a_deployable_directory() {
        let (_dir, project) = project_with(&[]);
        build_ok(&project).expect("出得来");

        let html = output_of(&project, "index");
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        // 页面外链资源，样式不进页面
        assert!(html.contains(r#"href="assets/base.css""#), "{html}");
        assert!(html.contains("<div class=\"mdpp-bar\">"), "{html}");
        assert!(!html.contains(".mdpp-content {"), "样式不该内联：{html}");
        // 而资源确实被拷到了产物里
        let css = std::fs::read_to_string(project.site_dir().join("assets/base.css"))
            .expect("资源没拷出来");
        assert!(css.contains(".mdpp-content"), "资源是空的");
        assert!(
            project.site_dir().join("assets/mdpp.js").is_file(),
            "脚本没拷出来"
        );
    }
}