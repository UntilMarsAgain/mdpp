//! `mdpp init` —— 建一个项目骨架。
//!
//! 骨架是 [`crate::project::SITE`] 那个目录（`mdpp-cli/site/`），编译期整个嵌进二进制，
//! `init` 只是把它**原样拷过去**。想改新项目长什么样，改那个目录里的文件，不必碰代码。
//!
//! 产出的是一个**能立刻 build 的目录**：首页、示例页与一个模板页都已经在那儿，第一次
//! `mdpp build` 就出得来一个能看的站点。
//!
//! ## 只在空目录里工作
//!
//! 目标目录里有任何东西（含隐藏文件）就**报错退出**，不覆盖、不合并、不问。
//! 「悄悄把文件盖在别人头上」是那种事后没人查得出来的破坏，而问一句
//! 「要不要继续」在这里又太容易答错 —— 答错了还是没法找回来。要往已有的目录里 init，
//! 先自己把里面清空。

use std::path::{Path, PathBuf};

use crate::options::Init;
use crate::project::{SITE, ensure_dir, shown, substitute_site_name};

use tracing::info;

/// 骨架里那个空的产物目录。**不在模板目录里** —— git 与 `include_dir!` 都带不动
/// 空目录，而 `build` 自己也会建；这里建它只是为了让目录结构一眼看得见
const SITE_DIR_NAME: &str = "build";

pub fn run(options: &Init) -> Result<(), String> {
    let root = options.dir.clone().unwrap_or_else(|| PathBuf::from("."));
    check_empty(&root)?;

    // 站点名：给了就用，没给就拿目录名（`.` 或 `/` 那种没有名字的除外）
    let name = options
        .name
        .clone()
        .or_else(|| root.file_name().map(|name| name.to_string_lossy().into_owned()))
        .filter(|name| !name.is_empty() && name != ".")
        .unwrap_or_else(|| "我的站点".to_string());

    ensure_dir(&root)?;
    copy_template(&root)?;
    // 拷过来的是骨架**原文**，那里面有站点名的占位符 —— 换上真的名字
    substitute_site_name(&root, &name)?;
    ensure_dir(&root.join(SITE_DIR_NAME))?;

    info!(dir = shown(&root), site = %name, "站点骨架");
    eprintln!("建了 {}", shown(&root));
    eprintln!("站点名：{name}");
    eprintln!("下一步：cd 到那里，`mdpp new 甲`，然后 `mdpp build`");
    Ok(())
}

/// 目标目录得是空的，或者压根不存在
fn check_empty(root: &Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(format!("{} 不是一个目录", shown(root)));
    }
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map_err(|error| format!("读不了 {}：{error}", shown(root)))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    names.sort();
    // 只列几个：几十个文件的目录不需要全部倒出来
    let shown_names = names
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    let more = names.len().saturating_sub(5);
    Err(format!(
        "{} 里已经有东西了（{}{}）\n`mdpp init` 要一个空目录 —— 建错了地方就换个目录名再来",
        shown(root),
        shown_names,
        if more > 0 { format!(" 等 {more} 个") } else { String::new() }
    ))
}

/// 把 `mdpp-cli/site/` 整个拷到 `root`
fn copy_template(root: &Path) -> Result<(), String> {
    SITE
        .extract(root)
        .map_err(|error| format!("拷不出站点骨架：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Config, SITE_NAME_TOKEN, site::Site};

    /// 一个空目录 + 跑一次 init
    fn init_at(dir: &Path, name: Option<&str>) -> Result<(), String> {
        run(&Init {
            dir: Some(dir.to_path_buf()),
            name: name.map(str::to_string),
        })
    }

    /// 骨架拷过去的样子：目录结构原样，没有多也没有少
    #[test]
    fn the_skeleton_lands_whole() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), None).expect("建得出");

        for path in [
            "mdpp.toml",
            ".gitignore",
            "pages/index.mdpp",
            "pages/示例.mdpp",
            "templates/卡片.mdpp",
            "build",
        ] {
            assert!(dir.path().join(path).exists(), "少了 {path}");
        }
        // 骨架里没有的，init 也不该造出来：根上正好是 mdpp.toml、.gitignore 与三个目录
        let top: Vec<String> = std::fs::read_dir(dir.path())
            .expect("读得动")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(top.len(), 5, "根上多了或少了东西：{top:?}");
    }

    /// init 的产出得真的能 build —— 骨架里的块名/参数一改，这里就先红
    #[test]
    fn the_skeleton_pages_have_no_unknown_blocks() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), None).expect("建得出");
        let project = crate::project::Project::load(dir.path()).expect("这是个项目");
        let pages = Site::collect(&project.pages_dir()).expect("扫得动");
        assert!(pages.pages().len() >= 2);

        let templates = crate::context::template_pages(&project.templates_dir()).expect("读得动");
        let context = mdpp::RenderContext::new().with_templates(templates);
        for page in pages.pages() {
            let source = std::fs::read_to_string(&page.source).expect("读得动");
            let html = mdpp::Mdpp::new().render_with(&source, &context);
            assert!(
                !html.contains("template--unknown"),
                "{} 里有未知块：\n{html}",
                page.key
            );
        }
    }

    /// 站点名落到两处：`mdpp.toml` 与首页的标题
    #[test]
    fn the_site_name_lands_in_the_config_and_the_home_page() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), Some("桥梁笔记")).expect("建得出");

        assert_eq!(
            Config::load(dir.path()).expect("读得动").site_name,
            "桥梁笔记"
        );
        let home = std::fs::read_to_string(dir.path().join("pages/index.mdpp")).expect("读得动");
        assert!(home.starts_with("# 桥梁笔记"), "{home}");
    }

    /// 没给名字就用目录名 —— 与 mkdocs 那套习惯一致
    #[test]
    fn without_a_name_the_directory_names_the_site() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let root = dir.path().join("桥梁笔记");
        init_at(&root, None).expect("建得出");
        assert_eq!(
            Config::load(&root).expect("读得动").site_name,
            "桥梁笔记"
        );
    }

    /// 名字里带引号不能让配置文件解析不了 —— 站点名是人写的
    #[test]
    fn a_quoted_site_name_still_yields_a_parsable_config() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), Some(r#"甲"乙"#)).expect("建得出");
        assert_eq!(
            Config::load(dir.path()).expect("读得动").site_name,
            r#"甲"乙"#
        );
    }

    /// 骨架里那些占位符**都**被换掉了 —— 漏一个，用户看到的项目里就留着一个
    /// `{{站点名}}`。钉住「骨架里只有那两处用它」
    #[test]
    fn no_placeholder_survives_the_copy() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), Some("某站")).expect("建得出");

        for path in ["mdpp.toml", "pages/index.mdpp", "pages/示例.mdpp", "templates/卡片.mdpp"] {
            let text = std::fs::read_to_string(dir.path().join(path)).expect("读得动");
            assert!(
                !text.contains(SITE_NAME_TOKEN),
                "{path} 里还留着 {SITE_NAME_TOKEN}"
            );
        }
    }

    /// 模板页里的 `{{标题}}` / `{{body}}` **不是**站点名 —— 那是块填空用的，
    /// 由渲染时填。init 要是把它也换了，那一页就废了
    #[test]
    fn the_template_pages_keep_their_own_placeholders() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), Some("某站")).expect("建得出");
        let card =
            std::fs::read_to_string(dir.path().join("templates/卡片.mdpp")).expect("读得动");
        assert!(card.contains("{{标题}}"), "{card}");
        assert!(card.contains("{{body}}"), "{card}");
    }

    /// **只在空目录里工作** —— 这是这一条命令唯一的前置条件
    #[test]
    fn a_directory_with_anything_in_it_is_refused() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("作者写的.md"), "别动我").expect("写得下");

        let message = init_at(dir.path(), None).unwrap_err();
        assert!(message.contains("已经有东西了"), "{message}");
        assert!(
            std::fs::read_to_string(dir.path().join("作者写的.md")).expect("读得动") == "别动我",
            "它不该被动过"
        );
    }

    /// 隐藏文件也算「有东西」 —— 半路塞进来的 `.DS_Store` 之类不该被当成空目录
    #[test]
    fn a_hidden_file_also_counts_as_content() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join(".gitkeep"), "").expect("写得下");
        assert!(init_at(dir.path(), None).unwrap_err().contains("已经有东西了"));
    }

    /// 重跑一次要停下来 —— 而不是把用户已经写的首页盖掉
    #[test]
    fn running_init_twice_refuses() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), None).expect("第一次建得出");
        let message = init_at(dir.path(), None).unwrap_err();
        assert!(message.contains("已经有东西了"), "{message}");
    }

    /// 目录不存在时建出来 —— `mdpp init 新站点` 是最常见的一条
    #[test]
    fn a_missing_directory_is_created() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let fresh = dir.path().join("新站点");
        init_at(&fresh, None).expect("建得出");
        assert!(fresh.join("mdpp.toml").is_file());
    }

    /// 站点名里带斜杠会让 `site_name` 写坏配置吗？不会（转义过），但路径相关的
    /// 那几处是目录 —— 这一条盯的是 config 读得回来
    #[test]
    fn the_copied_config_is_readable() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path(), None).expect("建得出");
        let config = Config::load(dir.path()).expect("读得动");
        assert_eq!(config.site_dir, PathBuf::from("build"));
    }
}
