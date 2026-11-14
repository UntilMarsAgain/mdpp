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

use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use include_dir::{Dir, include_dir};

pub use config::Config;

/// 站点模板：`mdpp init` 拷走的那一个目录（`mdpp-cli/site/`）。
///
/// 编译期整个嵌进二进制，`init` 把它原样拷到目标目录 —— 想改新项目长什么样，改那个
/// 目录里的文件，不必碰代码。它与 [`crate::render::page`] 里那一套 `assets/` 是两回事：
/// 那一套是**页面外壳**（build 时拷进产物），这一套是**新项目的骨架**（init 时拷走）。
pub static SITE: Dir = include_dir!("$CARGO_MANIFEST_DIR/site");

/// 站点模板里那个站点名的占位符
pub const SITE_NAME_TOKEN: &str = "{{site_name}}";

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
    ///
    /// 根会先换成真实路径：配置里的路径都相对它，而符号链接根下算出来的
    /// 「项目内」不能是一条看起来像、实际却指向外面的路。
    pub fn load(root: &Path) -> Result<Project, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("找不着项目目录 {}：{error}", root.display()))?;
        let config = Config::load(&root)?;
        Ok(Project { root, config })
    }

    /// 把配置里的一条相对路径接到项目根下，并确认它没有借 `..` 或符号链接
    /// 跑到项目外面。`what` 只用于报错，写成配置字段名。
    pub fn contained(&self, path: &Path, what: &str) -> Result<PathBuf, String> {
        contained(&self.root, path, what)
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
        under(&self.root, &self.config.build_dir)
    }

    /// `<title>` 里站点名那一半（没写站点名就当没有那一半）
    pub fn title_suffix(&self) -> Option<&str> {
        let name = self.config.site_name.trim();
        (!name.is_empty()).then_some(name)
    }
}

/// 把配置里的一个相对路径接到项目根下。**不检查边界**；只给已经验过的路径用。
pub fn under(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// 把一条配置路径接到 `root` 下，并守住运行目录的边界。
///
/// 只认相对路径；`..` 一律拒绝（哪怕最终仍落在根里 —— 这种写法没有需要
/// 支持的理由，拒绝比解释便宜）；路径上任何一段是符号链接也拒绝，因为链接
/// 的下一跳可能就在项目外面。目标可以还不存在（例如 `build/`），这时检查
/// 已经存在的那几段即可。
pub fn contained(root: &Path, configured: &Path, what: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("找不着项目目录 {}：{error}", root.display()))?;
    if configured.is_absolute() {
        return Err(format!(
            "{what}（{}）不能是绝对路径；只认项目目录里的相对路径",
            configured.display()
        ));
    }

    let mut relative = PathBuf::new();
    for component in configured.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "{what}（{}）不能用 `..` 绕到项目外面",
                    configured.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "{what}（{}）不能是绝对路径；只认项目目录里的相对路径",
                    configured.display()
                ));
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(format!(
            "{what} 不能是项目根目录本身（{}）",
            configured.display()
        ));
    }

    let mut at = root.clone();
    for component in relative.components() {
        at.push(component);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "{what}（{}）经过了符号链接 {}；项目外的东西不碰",
                    configured.display(),
                    shown(&at)
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!("看不了 {}：{error}", shown(&at)));
            }
        }
    }

    let candidate = root.join(&relative);
    if candidate.exists() {
        let real = candidate
            .canonicalize()
            .map_err(|error| format!("看不了 {}：{error}", shown(&candidate)))?;
        if !real.starts_with(&root) {
            return Err(format!(
                "{what}（{}）指到了项目目录外面（{}）",
                configured.display(),
                shown(&real)
            ));
        }
    }
    Ok(candidate)
}

/// 把骨架里那几处的 [`SITE_NAME_TOKEN`] 换成给定的站点名
///
/// **只动 [`SITE_NAME_FILES`] 里那几个**：其余文件里的双花括号是块填空用的
/// （模板页的 `{{title}}` / `{{body}}`，渲染时才填），换了那一页就废了。
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
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "site_name = \"甲\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\n",
        )
        .expect("写得下");
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
            "site_name = \"甲\"\npages_dir = \"正文\"\ntemplates_dir = \"模板\"\nbuild_dir = \"产物\"\n",
        )
        .expect("写得下");
        let project = Project::load(dir.path()).expect("读得动");

        assert_eq!(project.pages_dir(), dir.path().join("正文"));
        assert_eq!(project.site_dir(), dir.path().join("产物"));
        assert_eq!(project.templates_dir(), dir.path().join("模板"));
    }

    /// 必填的站点名**不能写空串** —— 空串等于不填写，而它是必填
    #[test]
    fn an_empty_site_name_is_refused() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "site_name = \"\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\n",
        )
        .expect("写得下");
        let error = Project::load(dir.path()).expect_err("空站点名该被拒绝");
        assert!(error.contains("site_name"), "{error}");

        std::fs::write(
            dir.path().join("mdpp.toml"),
            "site_name = \"甲\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\n",
        )
        .expect("写得下");
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
    /// `init` 就漏换一个，用户看到的项目里留着一个 `{{site_name}}`
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

    /// 配置路径不能借 `..` 或绝对路径出项目；这是运行目录边界的第一道检查
    #[test]
    fn a_configured_path_may_not_leave_the_project() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "site_name = \"甲\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\n",
        )
        .expect("写得下");
        let project = Project::load(dir.path()).expect("读得动");

        let error = project
            .contained(Path::new("../victim"), "pages_dir")
            .expect_err("`..` 该被拒绝");
        assert!(error.contains(".."), "{error}");

        let absolute = dir.path().join("outside");
        let error = project
            .contained(&absolute, "build_dir")
            .expect_err("绝对路径该被拒绝");
        assert!(error.contains("绝对路径"), "{error}");
    }

    /// 路径中间的任何一段是符号链接都不跟 —— 链接的下一跳可能就在项目外面
    #[test]
    fn a_symlinked_directory_is_not_a_way_out() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "site_name = \"甲\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\n",
        )
        .expect("写得下");
        std::fs::create_dir(dir.path().join("real")).expect("建得出");
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link"))
            .expect("建得出符号链接");
        let project = Project::load(dir.path()).expect("读得动");

        let error = project
            .contained(Path::new("link/页面.mdpp"), "pages_dir")
            .expect_err("符号链接该被拒绝");
        assert!(error.contains("符号链接"), "{error}");
    }

    /// 目标还不存在（例如第一次 build 的 `build/`）不算错，只要已存在的几段安全
    #[test]
    fn a_path_that_does_not_exist_yet_is_fine() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(
            dir.path().join("mdpp.toml"),
            "site_name = \"甲\"\npages_dir = \"pages\"\ntemplates_dir = \"templates\"\nbuild_dir = \"build\"\n",
        )
        .expect("写得下");
        let project = Project::load(dir.path()).expect("读得动");
        assert_eq!(
            project
                .contained(Path::new("build/nested"), "build_dir")
                .expect("可以"),
            dir.path()
                .canonicalize()
                .expect("真路径")
                .join("build/nested")
        );
    }
}
