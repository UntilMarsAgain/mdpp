//! **项目**：一个目录里有 `mdpp.toml` 与 `pages/` / `templates/` / `build/`。
//!
//! 与 [`crate::render`] 那一趟的区别就在这里：渲染单篇笔记不需要「项目」这个概念，
//! 而整站需要 —— 页与页之间要互相找得到（`[[入门]]`），块要有一份固定的模板页，
//! 产物还要落在一个固定的地方。这些约定都记在 `mdpp.toml` 里。
//!
//! ## 怎么找着一个项目
//!
//! 从当前目录**向上**找 `mdpp.toml`（与 cargo、git 同一套习惯）—— 于是
//! `mdpp build` 在项目里的任何一层都能敲，不必先 `cd` 到根。
//!
//! 找不到就说「这不是一个项目」，并指向 `mdpp init`。**不猜**：在一个普通的笔记目录里
//! 悄悄建出站点，比报错糟糕得多。

pub mod assets;
pub mod build;
pub mod config;
pub mod init;
pub mod new_page;
pub mod serve;
pub mod site;

use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};

pub use config::Config;

/// 站点模板：`mdpp init` 拷走的那一个目录（`mdpp-cli/site/`）。
///
/// 编译期整个嵌进二进制，`init` 把它原样拷到目标目录 —— 想改新项目长什么样，改那个
/// 目录里的文件，不必碰代码。它与 [`crate::page`] 里那一套 `assets/` 是两回事：
/// 那一套是**页面外壳**（build 时拷进产物），这一套是**新项目的骨架**（init 时拷走）。
pub static SITE: Dir = include_dir!("$CARGO_MANIFEST_DIR/site");

/// 站点模板里那个站点名的占位符
pub const SITE_NAME_TOKEN: &str = "{{站点名}}";

/// 骨架里带占位符的那些文件（拷过去之后要把占位符换成真名字）
const SITE_NAME_FILES: [&str; 2] = [config::FILE_NAME, "pages/index.mdpp"];

/// 一个项目：配置，以及那几个目录的**完整**位置（相对项目根的那些已经接上了）。
#[derive(Debug, Clone)]
pub struct Project {
    /// 项目根：装 `mdpp.toml` 的那一层
    pub root: PathBuf,
    /// 那份配置
    pub config: Config,
}

impl Project {
    /// 从 `dir` 开始向上找 `mdpp.toml`
    pub fn discover(dir: &Path) -> Result<Project, String> {
        let mut at = Some(dir);
        while let Some(current) = at {
            if current.join(config::FILE_NAME).is_file() {
                return Project::load(current);
            }
            at = current.parent();
        }
        Err(format!(
            "这里不是一个 mdpp 项目（{} 及其上层都没有 {}）\n先跑 `mdpp init` 建一个新项目",
            dir.display(),
            config::FILE_NAME
        ))
    }

    /// 从当前目录开始找
    pub fn open() -> Result<Project, String> {
        let here = std::env::current_dir().map_err(|error| format!("问不到当前目录：{error}"))?;
        Project::discover(&here)
    }

    /// 已经知道根在哪（`mdpp init` 刚建出来的时候）
    pub fn load(root: &Path) -> Result<Project, String> {
        let config = Config::load(root)?;
        Ok(Project {
            root: root.to_path_buf(),
            config,
        })
    }

    /// 正文目录
    pub fn pages_dir(&self) -> PathBuf {
        under(&self.root, &self.config.pages_dir)
    }

    /// 模板页目录
    pub fn templates_dir(&self) -> PathBuf {
        under(&self.root, &self.config.templates_dir)
    }

    /// 站点输出目录
    pub fn site_dir(&self) -> PathBuf {
        under(&self.root, &self.config.site_dir)
    }

    /// `<title>` 里站点名那一半（没写站点名就当没有那一半）
    pub fn title_suffix(&self) -> Option<&str> {
        let name = self.config.site_name.trim();
        (!name.is_empty()).then_some(name)
    }
}

/// 把配置里的一个相对路径接到项目根下。绝对路径原样留着
pub fn under(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// 把骨架里那几处的 [`SITE_NAME_TOKEN`] 换成给定的站点名
///
/// **只动 [`SITE_NAME_FILES`] 里那几个**：其余文件里的双花括号是块填空用的
/// （模板页的 `{{标题}}` / `{{body}}`，渲染时才填），换了那一页就废了。
///
/// 配置里那一处要**转义**（占位符是写在引号里的），正文那一处原样换。
pub fn substitute_site_name(root: &Path, name: &str) -> Result<(), String> {
    for relative in SITE_NAME_FILES {
        let path = root.join(relative);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("读不了 {}：{error}", shown(&path)))?;
        let value = if relative == config::FILE_NAME {
            toml_escape(name)
        } else {
            name.to_string()
        };
        write_file(&path, &text.replace(SITE_NAME_TOKEN, &value))?;
    }
    Ok(())
}

/// 站点名进 TOML 基本字符串前的转义（**不带**外层引号 —— 占位符本来就在引号里）。
/// 站点名是人写的，里面带引号不该让刚建好的项目读不出配置
fn toml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// 把 `path` 建成目录（已经在了不算错 —— `create_dir_all` 本来就那样）
pub fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("建不了目录 {}：{error}", path.display()))
}

/// 写一个文件，顺手把它所在的目录建出来
pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(path, content).map_err(|error| format!("写不了 {}：{error}", path.display()))
}

/// 把路径显示成**相对当前目录**的样子 —— 长长的绝对路径会把报错撑成一屏
pub fn shown(path: &Path) -> String {
    let here = std::env::current_dir().ok();
    match here {
        Some(here) => match path.strip_prefix(&here) {
            Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
            Ok(relative) => relative.display().to_string(),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use include_dir::File;

    /// 项目要**一层层往上找**：站在 `pages/指南` 里敲命令也得认出这个项目 ——
    /// 认不出的话，那句报错会让人以为项目坏了
    #[test]
    fn a_project_is_found_from_a_subdirectory() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("mdpp.toml"), "").expect("写得下");
        let deep = dir.path().join("pages").join("指南");
        std::fs::create_dir_all(&deep).expect("建得出");

        let project = Project::discover(&deep).expect("找得到");
        assert_eq!(project.root, dir.path());
    }

    /// 找不到就说「这不是项目」，并指向 `init` —— 不猜，也不默默建站点
    #[test]
    fn outside_a_project_it_says_so_and_points_at_init() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        // 临时目录的上层是 /tmp，那儿不会正好有 mdpp.toml；真要有也不算错，
        // 所以这条只断言「找到的根里必须有 mdpp.toml」
        match Project::discover(dir.path()) {
            Ok(project) => assert!(
                project.root.join("mdpp.toml").is_file(),
                "找到的根里没有配置文件"
            ),
            Err(message) => assert!(message.contains("mdpp init"), "{message}"),
        }
    }

    /// 项目根就是配置文件那一层，目录都从它接出来
    #[test]
    fn the_directories_hang_off_the_project_root() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "pages_dir = \"正文\"\nsite_dir = \"产物\"\n",
        )
        .expect("写得下");
        let project = Project::load(dir.path()).expect("读得动");

        assert_eq!(project.pages_dir(), dir.path().join("正文"));
        assert_eq!(project.site_dir(), dir.path().join("产物"));
        // 没写的那几个走默认值
        assert_eq!(project.templates_dir(), dir.path().join("templates"));
    }

    /// 不写站点名就不该在标题里多出那一半 —— 免得页面标题变成「页面名 —— 」
    #[test]
    fn an_empty_site_name_contributes_nothing_to_the_title() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("mdpp.toml"), "site_name = \"\"\n").expect("写得下");
        assert_eq!(
            Project::load(dir.path()).expect("读得动").title_suffix(),
            None
        );
        std::fs::write(dir.path().join("mdpp.toml"), "site_name = \"甲\"\n").expect("写得下");
        assert_eq!(
            Project::load(dir.path()).expect("读得动").title_suffix(),
            Some("甲")
        );
    }

    /// 绝对路径原样留着，相对路径接到项目根下 —— 配置里写 `/pages` 与 `pages` 要两样
    #[test]
    fn relative_paths_hang_off_the_project_root() {
        assert_eq!(
            under(Path::new("/项目"), Path::new("pages")),
            PathBuf::from("/项目/pages")
        );
        assert_eq!(
            under(Path::new("/项目"), Path::new("/别处/pages")),
            PathBuf::from("/别处/pages")
        );
    }

    /// 骨架里的站点名占位符只出现在那两个文件里 —— 多一个文件用上它，
    /// `init` 就漏换一个，用户看到的项目里留着一个 `{{站点名}}`
    #[test]
    fn the_site_name_token_only_lives_where_init_replaces_it() {
        for relative in SITE_NAME_FILES {
            assert!(
                crate::project::SITE
                    .get_file(relative)
                    .and_then(File::contents_utf8)
                    .is_some_and(|text| text.contains(SITE_NAME_TOKEN)),
                "骨架里的 {relative} 没有用 {SITE_NAME_TOKEN}"
            );
        }
    }
}
