//! `mdpp.toml`：项目里那几样「东西在哪、站点叫什么」。
//!
//! 字段是**扁平**的（不像 mkdocs 那样分 `[project]` / `[build]` 两张表）：一份配置
//! 要读的次数屈指可数，分表只多一次缩进。
//!
//! `deny_unknown_fields` 是有意的 —— 拼错一个键（`pages_dir` 写成 `page_dir`）在**解析**
//! 时就该说，而默默用默认值的后果是「目录是空的」这种很难查的现象。
//!
//! 每个字段都有默认值，所以一份**空的** `mdpp.toml` 也是好配置。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 配置文件名
pub const FILE_NAME: &str = "mdpp.toml";

/// 一份项目配置。路径都是**相对项目根**的（项目根 = 有这份文件的那一层）。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// 站点名，进 `<title>`（`页面名 —— 站点名`）
    #[serde(default = "default_site_name")]
    pub site_name: String,

    /// 一句话说明，进 `<meta name="description">`（也是搜索结果里那段话）
    #[serde(default)]
    pub site_description: String,

    /// 站点在网上的**绝对**地址，如 `https://example.github.io/repo/`。
    ///
    /// 写了就出 `<link rel="canonical">`，而**页内链接一律仍是相对的** ——
    /// 相对地址让整站能挂到任意子路径下；`site_url` 只回答「你在网上哪儿」那
    /// 一件事（给搜索引擎、给分享用的那个绝对地址）。
    ///
    /// 末尾那个 `/` 会在需要时补上。不写就没有 `canonical` —— 本地预览、
    /// 或者只是想 `build` 出来看看的人，不必先想这件事。
    #[serde(default)]
    pub site_url: Option<String>,

    /// 正文在哪：扫描这个目录，`.md` 与 `.mdpp` 都算页面
    #[serde(default = "default_pages_dir")]
    pub pages_dir: PathBuf,

    /// 模板页在哪：`<目录>/<名字>.mdpp` 是一页，`::名字` 从这里取
    #[serde(default = "default_templates_dir")]
    pub templates_dir: PathBuf,

    /// 站点输出到哪。`mdpp build` 写在这里
    #[serde(default = "default_site_dir")]
    pub site_dir: PathBuf,

    /// 底栏那句话。**空的就是没有底栏**（与 mkdocs 的 `copyright` 一个意思）
    #[serde(default)]
    pub copyright: String,

    /// 顶栏右端那个仓库链接。空的就是没有那个链接
    #[serde(default)]
    pub repo_url: String,

    /// 标签页上那个小图标（favicon）。**相对项目根**，不是相对 pages/。
    /// 不写就没有那个 `<link>`，浏览器自己画一个
    #[serde(default)]
    pub favicon: Option<PathBuf>,

    /// 整页的主题：`light` 或 `dark`。不写就是默认的 `light`
    #[serde(default)]
    pub theme: Option<String>,

    /// `<html lang>`
    #[serde(default = "default_lang")]
    pub lang: String,

    /// 附加的 CSS，按给定次序接在默认样式之后
    #[serde(default)]
    pub extra_css: Vec<PathBuf>,

    /// 附加的 JS，按给定次序接在默认脚本之后
    #[serde(default)]
    pub extra_js: Vec<PathBuf>,

    /// **覆盖层目录**：build 完成后，把这个目录里的东西原样拷到产物里。
    /// 重复就覆盖 —— 可以往里放 `CNAME`、自定义 `robots.txt`、把某页换成手写的 HTML……
    /// 不写就没有这一层。相对项目根，默认 `override`。
    #[serde(default = "default_override_dir")]
    pub override_dir: PathBuf,
}

/// `Default` 与 serde 的那些 `default = …` 是**同一批**函数 ——
/// 两边各写一份的话，「读一份空配置」与「`Config::default()`」会给出不同的答案，
/// 而那种差异只在测试里显形。
impl Default for Config {
    fn default() -> Self {
        Config {
            site_name: default_site_name(),
            site_description: String::new(),
            site_url: None,
            pages_dir: default_pages_dir(),
            templates_dir: default_templates_dir(),
            site_dir: default_site_dir(),
            copyright: String::new(),
            repo_url: String::new(),
            favicon: None,
            theme: None,
            lang: default_lang(),
            extra_css: Vec::new(),
            extra_js: Vec::new(),
            override_dir: default_override_dir(),
        }
    }
}

fn default_site_name() -> String {
    "我的站点".to_string()
}

fn default_pages_dir() -> PathBuf {
    PathBuf::from("pages")
}

fn default_templates_dir() -> PathBuf {
    PathBuf::from("templates")
}

fn default_site_dir() -> PathBuf {
    PathBuf::from("build")
}

fn default_lang() -> String {
    "zh-Hans".to_string()
}

fn default_override_dir() -> PathBuf {
    PathBuf::from("override")
}

/// 认这两个主题，与 `render --theme` 同一套取值
pub const THEMES: [&str; 2] = ["light", "dark"];

impl Config {
    /// 读一份配置。读不了或写错了就说清是**哪一处**写错了
    pub fn load(root: &Path) -> Result<Config, String> {
        let path = root.join(FILE_NAME);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("读不了 {}：{error}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .map_err(|error| format!("{} 里写错了：\n{error}", path.display()))?;
        config.check()?;
        Ok(config)
    }

    /// 能在解析时查出来的错就在解析时查 —— 「主题只能是 light 或 dark」这种事，
    /// 不该等到 `build` 跑了一半才发现。
    pub fn check(&self) -> Result<(), String> {
        if let Some(theme) = &self.theme {
            if !THEMES.contains(&theme.as_str()) {
                return Err(format!("theme 只能是 light 或 dark，写成了「{theme}」"));
            }
        }
        // 协议只认 http/https —— 配错了的话 canonical 就是一个坏地址，
        // 而这类错误在产物里完全看不出来（只是搜索引擎不认）
        if let Some(url) = &self.site_url {
            let trimmed = url.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
            {
                return Err(format!(
                    "site_url 要以 http:// 或 https:// 开头，写成了「{trimmed}」"
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SITE;
    use include_dir::File;

    /// 骨架里的 `site/mdpp.toml` 必须真的能被读回来 —— 它是刚 init 出来的项目的配置，
    /// 而写坏它要等到用户 `mdpp init` 之后才发作
    #[test]
    fn the_skeleton_config_parses_and_says_what_it_says() {
        let text = SITE
            .get_file(FILE_NAME)
            .and_then(File::contents_utf8)
            .expect("站点模板里有 mdpp.toml");
        // 骨架里带着站点名的占位符（那是 init 要替换的），它本身也是一份合法配置
        let filled = text.replace(crate::project::SITE_NAME_TOKEN, "我的站点");
        let config: Config = toml::from_str(&filled).expect("骨架配置该能被解析");
        assert_eq!(config.pages_dir, PathBuf::from("pages"));
        assert_eq!(config.site_dir, PathBuf::from("build"));
        assert_eq!(config.templates_dir, PathBuf::from("templates"));
        // 模板里主题写明了「light」，而「不写主题」是另一回事（`None`）
        assert_eq!(config.theme.as_deref(), Some("light"));
        assert!(config.check().is_ok(), "骨架自己该是份好配置");

        // 除主题之外，模板里写的都等于默认值 —— 写重复了就有两份答案
        assert_eq!(
            Config {
                theme: None,
                ..config
            },
            Config::default()
        );
    }

    /// 一份空的 mdpp.toml 也是好配置：每个字段都有默认值
    #[test]
    fn an_empty_file_is_a_valid_config() {
        let config: Config = toml::from_str("").expect("空的该行");
        assert_eq!(config, Config::default());
    }

    /// 拼错的键必须在**解析时**就说 —— 默默用默认值的后果是「目录是空的」
    #[test]
    fn a_misspelled_key_is_refused() {
        let error = toml::from_str::<Config>("page_dir = \"pages\"").expect_err("本该报错");
        assert!(error.to_string().contains("page_dir"), "{error}");
    }

    #[test]
    fn theme_only_takes_light_or_dark() {
        let config = Config {
            theme: Some("blue".to_string()),
            ..Config::default()
        };
        assert!(config.check().is_err());
        assert!(Config::default().check().is_ok(), "不写主题该是好的");
        assert!(
            Config {
                theme: Some("dark".into()),
                ..Config::default()
            }
            .check()
            .is_ok()
        );
    }

    /// `site_url` 只认 http/https —— 写别的那个地址在产物里完全看不出来
    #[test]
    fn site_url_takes_http_or_https() {
        assert!(
            Config {
                site_url: Some("ftp://x/".into()),
                ..Config::default()
            }
            .check()
            .is_err()
        );
        assert!(
            Config {
                site_url: Some("/repo/".into()),
                ..Config::default()
            }
            .check()
            .is_err()
        );
        for good in ["https://x.github.io/repo/", "http://127.0.0.1:8080/"] {
            assert!(
                Config {
                    site_url: Some(good.into()),
                    ..Config::default()
                }
                .check()
                .is_ok(),
                "{good} 该被接受"
            );
        }
        // 不写（本地预览）与写空串都该过
        assert!(Config::default().check().is_ok());
        assert!(
            Config {
                site_url: Some(String::new()),
                ..Config::default()
            }
            .check()
            .is_ok()
        );
    }
}
