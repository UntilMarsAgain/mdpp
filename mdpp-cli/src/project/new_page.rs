//! `mdpp new` —— 在 `pages/` 下新建一页。
//!
//! 页名带 `指南/入门` 这样的子目录也行，目录会自动建 —— 站点的一级层就是文件系统
//! 里的目录（见 [`crate::project::site`]），不必再有一份「哪些页归哪一栏」的清单。
//!
//! **不覆盖**：那一页存在就是作者写了，点名让它消失是另一件事（要删就 `rm`，看得见）。

use std::path::{Path, PathBuf};

use crate::options::New;
use crate::project::{Project, shown, write_file};

use tracing::info;

/// 新页的骨架。`{{标题}}` 与 `{{回首页}}` 由 [`create`] 换掉
const SKELETON: &str = "\
# {{标题}}

这一页是 `mdpp new` 建的，写点东西吧。
{{回首页}}";

pub fn run(options: &New) -> Result<(), String> {
    let project = Project::open()?;
    create(&project, options)
}

/// 已经在项目里了，建一页。
///
/// 与 [`run`] 分开是为了能直接测：**测试里换进程当前目录是有并发风险的**，
/// 而这里要验的每一件事（页名校验、不覆盖、回首页那条）都与「从哪儿敲的命令」无关。
fn create(project: &Project, options: &New) -> Result<(), String> {
    let pages = project.pages_dir();
    if !pages.is_dir() {
        return Err(format!(
            "正文目录不在（{}）\n要么这个项目还没建好，要么 mdpp.toml 里的 pages_dir 指错了",
            shown(&pages)
        ));
    }

    let key = check(&options.name)?;
    let path = pages.join(source_of(&key));

    let title = options
        .title
        .clone()
        .unwrap_or_else(|| key.rsplit('/').next().unwrap_or(&key).to_string());

    if path.exists() {
        return Err(format!("{} 已经在了", shown(&path)));
    }

    // 回首页那条只在**真有首页**时加 —— 指一个不存在的页就是一条红链
    let home = if has_home_page(&pages) {
        "\n\n[[index|回首页]]"
    } else {
        ""
    };
    write_file(
        &path,
        &SKELETON
            .replace("{{标题}}", &title)
            .replace("{{回首页}}", home),
    )?;

    info!(page = %key, "新页");
    eprintln!("建了 {}", shown(&path));
    eprintln!("下一步：`mdpp build`");
    Ok(())
}

/// 页名校验：只能是 `pages/` 里的**相对**路径，不许往上跑，也不许是空段
fn check(name: &str) -> Result<String, String> {
    let trimmed = name.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("页名不能是空的".to_string());
    }
    let segments: Vec<&str> = trimmed.split('/').collect();
    for segment in &segments {
        if segment.is_empty() || *segment == "." || *segment == ".." {
            return Err(format!(
                "页名「{trimmed}」里有一段不是页名（空段、`.` 或 `..`）"
            ));
        }
    }
    Ok(segments.join("/"))
}

/// 键 → 源文件路径：`<页名>.mdpp`
fn source_of(key: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in key.split('/') {
        path.push(segment);
    }
    path.set_extension("mdpp");
    path
}

/// 首页在不在（`.md` 或 `.mdpp` 都算）
fn has_home_page(pages: &Path) -> bool {
    ["index.mdpp", "index.md"]
        .iter()
        .any(|name| pages.join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Init;
    use crate::project::site::Site;

    /// 一个建好骨架的项目，以及那个项目本身
    fn scaffold() -> (tempfile::TempDir, Project) {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let root = dir.path().join("站点");
        crate::project::init::run(&Init {
            dir: Some(root.clone()),
            name: None,
        })
        .expect("建得出");
        let project = Project::load(&root).expect("这是个项目");
        (dir, project)
    }

    fn new(project: &Project, name: &str) -> Result<(), String> {
        create(
            project,
            &New {
                name: name.to_string(),
                title: None,
            },
        )
    }

    #[test]
    fn a_new_page_lands_in_pages_and_gets_picked_up() {
        let (_dir, project) = scaffold();
        new(&project, "指南/入门").expect("建得出");

        let path = project.pages_dir().join("指南/入门.mdpp");
        assert!(path.is_file(), "没建出 {}", path.display());
        let text = std::fs::read_to_string(&path).expect("读得动");
        assert!(text.starts_with("# 入门"), "{text}");

        let site = Site::collect(&project.pages_dir()).expect("扫得动");
        assert!(site.by_key("指南/入门").is_some(), "{:?}", site.pages());
    }

    /// 骨架那一页在，那条「回首页」就该有；它是一个站内链接，不是红链
    #[test]
    fn the_back_link_is_only_added_when_there_is_a_home_page() {
        let (_dir, project) = scaffold();
        new(&project, "甲").expect("建得出");
        let with_home =
            std::fs::read_to_string(project.pages_dir().join("甲.mdpp")).expect("读得动");
        assert!(with_home.contains("[[index|回首页]]"), "{with_home}");

        std::fs::remove_file(project.pages_dir().join("index.mdpp")).expect("删得掉");
        new(&project, "乙").expect("建得出");
        let without = std::fs::read_to_string(project.pages_dir().join("乙.mdpp")).expect("读得动");
        assert!(!without.contains("回首页"), "没有首页就不该指它：{without}");
    }

    /// `--title` 覆盖页名，标题与文件名不必一致
    #[test]
    fn the_title_can_differ_from_the_file_name() {
        let (_dir, project) = scaffold();
        create(
            &project,
            &New {
                name: "about".to_string(),
                title: Some("关于".to_string()),
            },
        )
        .expect("建得出");

        let text = std::fs::read_to_string(project.pages_dir().join("about.mdpp")).expect("读得动");
        assert!(text.starts_with("# 关于"), "{text}");
    }

    /// 已有的一页不覆盖
    #[test]
    fn an_existing_page_is_not_overwritten() {
        let (_dir, project) = scaffold();
        let path = project.pages_dir().join("index.mdpp");
        std::fs::write(&path, "# 我写过了").expect("改得下");

        let message = new(&project, "index").unwrap_err();
        assert!(message.contains("index"), "{message}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("读得动"),
            "# 我写过了"
        );
    }

    /// 页名是 `pages/` 里的相对路径，不许往上跑
    #[test]
    fn a_page_name_may_not_climb_out_of_pages() {
        for name in [
            "../逃出",
            "指南/../../逃出",
            "",
            "   ",
            "指南//入门",
            "./入门",
        ] {
            assert!(check(name).is_err(), "{name:?} 不该被接受");
        }
        assert_eq!(check(" 指南/入门 ").unwrap(), "指南/入门");
        assert_eq!(check("入门").unwrap(), "入门");
    }

    /// `pages_dir` 指错了要说清是哪个目录 —— 报错里带路径，作者不用自己猜
    #[test]
    fn a_missing_pages_directory_says_where_it_looked() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "pages_dir = \"根本没有这个\"\n",
        )
        .expect("写得下");
        let project = Project::load(dir.path()).expect("读得动");

        let message = new(&project, "甲").unwrap_err();
        assert!(message.contains("根本没有这个"), "{message}");
    }

    /// 参数解析：`new` 的页名是必填的
    #[test]
    fn new_parses_its_page_name() {
        use crate::options::{Cli, Command};
        use clap::Parser;
        let cli = Cli::try_parse_from(["mdpp", "new", "指南/入门"]).expect("该行");
        assert!(matches!(
            cli.command,
            Command::New(New {
                name,
                title: None,
                ..
            }) if name == "指南/入门"
        ));
    }
}
