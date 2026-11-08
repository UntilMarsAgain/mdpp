//! `mdpp.toml`：项目里那几样「东西在哪、站点叫什么」。
//!
//! 字段是**扁平**的（不像 mkdocs 那样分 `[project]` / `[build]` 两张表）：一份配置
//! 要读的次数屈指可数，分表只多一次缩进。
//!
//! `deny_unknown_fields` 是有意的 —— 拼错一个键（`pages_dir` 写成 `page_dir`）在**解析**
//! 时就该说，而默默用默认值的后果是「目录是空的」这种很难查的现象。
//!
//! 字段分三类：
//! - **必填**：`site_name`、`pages_dir`、`templates_dir`、`build_dir`，都不允许空串；
//! - **可选**：`site_description`、`lang`、`base_url`、`repo_url`、`favicon`、`logo`、`footer`、`override_dir`，
//!   不填写或写空串都等于不提供，各自的功能也就没有；
//! - **有默认值**：`theme` 默认 `system`，`theme_color` 默认 `#2f6fd0`，
//!   `max_template_depth` 默认 64；
//! - **无功能**：`extra_css`、`extra_js`、`nav`、`toc`，不写就空/不生成。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// 只**读** `mdpp.toml`，从不往回写（`mdpp init` 拷的是骨架目录，不是序列化一份配置）
use serde::Deserialize;

/// 配置文件名
pub const FILE_NAME: &str = "mdpp.toml";

/// 一份项目配置。路径都是**相对项目根**的（项目根 = 有这份文件的那一层）。
///
/// 字段顺序与 `mdpp.toml` 里的写法一致，方便对照。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// 站点名，进 `<title>`（`页面名 —— 站点名`）。**必填**，不能为空。
    pub site_name: String,

    /// 一句话说明，进 `<meta name="description">`（也是搜索结果里那段话）。
    /// 不填写就不提供、不显示。
    pub site_description: Option<String>,

    /// 站点语言，进 `<html lang>`。不填写就不提供。
    pub lang: Option<String>,

    /// 站点在网上的**绝对**地址，如 `https://example.github.io/repo/`。
    ///
    /// 写了就出 `<link rel="canonical">`，而**页内链接一律仍是相对的** ——
    /// 相对地址让整站能挂到任意子路径下；`base_url` 只回答「你在网上哪儿」那
    /// 一件事（给搜索引擎、给分享用的那个绝对地址）。
    ///
    /// 末尾那个 `/` 会在需要时补上。不写就没有 `canonical` —— 本地预览、
    /// 或者只是想 `build` 出来看看的人，不必先想这件事。
    pub base_url: Option<String>,

    /// 站点的仓库地址，没有时可不填写。
    pub repo_url: Option<String>,

    /// 标签页上那个小图标（favicon）。**相对项目根**，不是相对 pages/。
    /// 不写就没有那个 `<link>`，浏览器自己画一个
    pub favicon: Option<PathBuf>,

    /// 顶栏里、站点名**旁边**那个图标。**相对项目根** —— 与 `favicon` 同一条路：
    /// 读进来、拷进产物的 `assets/`、页面上按各自深度给相对地址。
    ///
    /// **与 `favicon` 是两件事，不写就各是各的**：favicon 是浏览器标签页上那一枚
    /// 小方块（尺寸固定、离得远），logo 是站在站点名旁边、要看清的那个图。常见做法
    /// 是两个都指向同一张图（`logo = favicon = "assets/icon.svg"`），但不必如此。
    ///
    /// 不写就没有那个 `` —— 顶栏只是少了它，其余一字不变。
    pub logo: Option<PathBuf>,

    /// 底栏那句话。**不填写就没有底栏**（与 mkdocs 的 `copyright` 一个意思）。
    pub footer: Option<String>,

    /// 正文文件夹（必填）：扫描这个目录，`.md` 与 `.mdpp` 都算页面。
    pub pages_dir: PathBuf,

    /// 模板文件夹（必填）：`<目录>/<名字>.mdpp` 是一页，`::名字` 从这里取。
    pub templates_dir: PathBuf,

    /// 用户模板最多嵌套多少层。不写就是 64；`0` 会让模板全部停在第一层。
    #[serde(default = "default_max_template_depth")]
    pub max_template_depth: usize,

    /// 站点输出文件夹（必填）：`mdpp build` 写在这里。
    pub build_dir: PathBuf,

    /// **覆盖层目录**：build 完成后，把这个目录里的东西原样拷到产物里。
    /// 重复就覆盖 —— 可以往里放 `CNAME`、自定义 `robots.txt`、把某页换成手写的 HTML……
    /// 不填写就没有覆盖层。相对项目根。
    pub override_dir: Option<PathBuf>,

    /// 整页的主题：`light` / `dark` 是钉死，`system` 是**跟随系统**。
    /// 不提供则为 `system`。
    #[serde(default = "default_theme")]
    pub theme: String,

    /// 顶栏那块底的**主题色**，十六进制：`theme_color = "#2f6fd0"`（`#rgb` 也认）。
    ///
    /// 顶栏是**一整块有色的底**（站点名那一行与它下面的导航），压在它上面的字由这个
    /// 颜色当场算（亮底配近黑字、暗底配白字），深色下再自动压暗一档 —— 亮块在暗页面
    /// 上是一条刺眼的亮带。
    ///
    /// 不提供则为 `#2f6fd0`。
    #[serde(default = "default_theme_color")]
    pub theme_color: String,

    /// 附加的 CSS，按给定次序接在默认样式之后。
    #[serde(default)]
    pub extra_css: Vec<PathBuf>,

    /// 附加的 JS，按给定次序接在默认脚本之后。
    #[serde(default)]
    pub extra_js: Vec<PathBuf>,

    /// 导航栏：顶栏下面那一行链接，mkdocs 的 `nav` 那套写法。
    ///
    /// 每一项要么是一个字符串（显示文字，指「全站唯一叫这个名字的页」），
    /// 要么是一个单项表（显示文字 → 站内页的键）。例如：
    ///
    /// ```toml
    /// nav = ["首页", { "指南" = "指南/入门" }]
    /// ```
    ///
    /// 站内页的键就是 `pages/` 下的路径（不带后缀）；`index` 是首页。
    /// 不填写就没有那一行。
    #[serde(default)]
    pub nav: Vec<NavEntry>,

    /// 目录页的名字（键）。`mdpp build` 会额外生成一页「页面清单」——
    /// 列出全站每一页，并自动在导航栏末尾加一个按钮指过去。
    /// 不填写就不生成。
    pub toc: Option<String>,
}

/// 导航栏里的一项。两种写法：
///
/// - 纯字符串：`"首页"` —— 显示文字是「首页」，指全站**唯一**叫这个名字的页
///   （按 `[[名字]]` 那套规则找：键名、或全站唯一的文件名）；
/// - 单项表：`{ "指南" = "指南/入门" }` —— 显示文字是「指南」，指 `pages/指南/入门`。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum NavEntry {
    /// 字符串：显示文字 = 站内页的名字（全站唯一）
    SameName(String),
    /// 单项表：显示文字 → 站内页的键
    Named(BTreeMap<String, String>),
}

/// `Default` 与 serde 的那些 `default = …` 是**同一批**函数 ——
/// 两边各写一份的话，「读一份空配置」与「`Config::default()`」会给出不同的答案，
/// 而那种差异只在测试里显形。
impl Default for Config {
    fn default() -> Self {
        Config {
            site_name: default_site_name(),
            site_description: None,
            lang: None,
            base_url: None,
            repo_url: None,
            favicon: None,
            logo: None,
            footer: None,
            pages_dir: default_pages_dir(),
            templates_dir: default_templates_dir(),
            max_template_depth: default_max_template_depth(),
            build_dir: default_site_dir(),
            override_dir: None,
            theme: default_theme(),
            theme_color: default_theme_color(),
            extra_css: Vec::new(),
            extra_js: Vec::new(),
            nav: Vec::new(),
            toc: None,
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

fn default_max_template_depth() -> usize {
    mdpp::context::MAX_TEMPLATE_DEPTH
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_theme_color() -> String {
    "#2f6fd0".to_string()
}

/// `theme` 认的三个取值，与 `render --theme` 同一套。
/// `system` 不是一种配色，是「跟着系统走」
pub const THEMES: [&str; 3] = ["system", "light", "dark"];

impl Config {
    /// 读一份配置。读不了或写错了就说清是**哪一处**写错了
    pub fn load(root: &Path) -> Result<Config, String> {
        let path = root.join(FILE_NAME);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("读不了 {}：{error}", path.display()))?;
        let mut config: Config = toml::from_str(&text)
            .map_err(|error| format!("{} 里写错了：\n{error}", path.display()))?;
        config.normalize();
        config.check()?;
        Ok(config)
    }

    /// 把「写了空串」归一为「没写」：空串等于不填写。
    ///
    /// 有默认值的两项（`theme` / `theme_color`）空串也回落默认值。
    fn normalize(&mut self) {
        fn clean(value: &mut Option<String>) {
            if value.as_deref().is_some_and(|text| text.trim().is_empty()) {
                *value = None;
            }
        }
        clean(&mut self.site_description);
        clean(&mut self.lang);
        clean(&mut self.base_url);
        clean(&mut self.repo_url);
        clean(&mut self.footer);
        clean(&mut self.toc);

        fn clean_path(value: &mut Option<PathBuf>) {
            if value
                .as_deref()
                .is_some_and(|path| path.as_os_str().is_empty())
            {
                *value = None;
            }
        }
        clean_path(&mut self.favicon);
        clean_path(&mut self.logo);
        clean_path(&mut self.override_dir);

        if self.theme.trim().is_empty() {
            self.theme = default_theme();
        }
        if self.theme_color.trim().is_empty() {
            self.theme_color = default_theme_color();
        }
    }

    /// 能在解析时查出来的错就在解析时查 —— 「主题只能是那三个值」这种事，
    /// 不该等到 `build` 跑了一半才发现。
    pub fn check(&self) -> Result<(), String> {
        // 必填字段不能为空
        if self.site_name.trim().is_empty() {
            return Err("site_name 不能为空".to_string());
        }
        if self.pages_dir.as_os_str().is_empty() {
            return Err("pages_dir 不能为空".to_string());
        }
        if self.templates_dir.as_os_str().is_empty() {
            return Err("templates_dir 不能为空".to_string());
        }
        if self.build_dir.as_os_str().is_empty() {
            return Err("build_dir 不能为空".to_string());
        }
        if self.max_template_depth == 0 {
            return Err("max_template_depth 至少是 1".to_string());
        }

        // 可选字段里，空串等于不填写：统一把空串当成 None（在使用时统一处理，
        // 这里只做格式校验）
        if !THEMES.contains(&self.theme.as_str()) {
            return Err(format!(
                "theme 只能是 system、light 或 dark，写成了「{}」",
                self.theme
            ));
        }
        // 主题色要在**构建期**算出字色与深色那一档，所以只认十六进制 —— 名字与函数
        // 写法能塞进字符串的东西比颜色多，而这个值要落进 `<style>` 与 `<meta>`。
        // 坏值会在整站的顶栏上现形，报错该在这儿而不是渲染到一半时
        if crate::support::color::Color::parse(&self.theme_color).is_none() {
            return Err(format!(
                "theme_color 得是十六进制颜色，如 \"#2f6fd0\"（#rgb 与 #rrggbb 都行），\
                 写成了「{}」",
                self.theme_color
            ));
        }
        // 协议只认 http/https —— 配错了的话 canonical 就是一个坏地址，
        // 而这类错误在产物里完全看不出来（只是搜索引擎不认）
        if let Some(url) = &self.base_url {
            let trimmed = url.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
            {
                return Err(format!(
                    "base_url 要以 http:// 或 https:// 开头，写成了「{trimmed}」"
                ));
            }
        }
        // 导航里每一项都得「看得懂」：单项表只能有一项，显示文字与键都不能是空的
        for entry in &self.nav {
            match entry {
                NavEntry::SameName(label) => {
                    if label.trim().is_empty() {
                        return Err(
                            "nav 里有一项是空的 —— 写 `{ \"首页\" = \"index\" }` 那样".to_string()
                        );
                    }
                }
                NavEntry::Named(map) => {
                    if map.len() != 1 {
                        return Err(format!(
                            "nav 里的单项表只能写一项（显示文字 → 站内页的键），写成了 {map:?}"
                        ));
                    }
                    let (label, key) = map.iter().next().expect("上面刚验过只有一项");
                    if label.trim().is_empty() || key.trim().is_empty() {
                        return Err(format!("nav 里的 {label:?} → {key:?} 有一头是空的"));
                    }
                }
            }
        }
        // 目录页的键也会变成产物路径；在这里先说，不要等到 build 到一半
        if let Some(toc) = self
            .toc
            .as_deref()
            .map(str::trim)
            .filter(|toc| !toc.is_empty())
        {
            crate::project::site::check_key(toc)?;
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
        let mut config: Config = toml::from_str(&filled).expect("骨架配置该能被解析");
        config.normalize();
        config.check().expect("骨架自己该是份好配置");

        assert_eq!(config.site_name, "我的站点");
        assert_eq!(config.pages_dir, PathBuf::from("pages"));
        assert_eq!(config.templates_dir, PathBuf::from("templates"));
        assert_eq!(config.build_dir, PathBuf::from("build"));
        assert_eq!(config.max_template_depth, 64);
        assert_eq!(config.theme, "system");
        assert_eq!(config.theme_color, "#2f6fd0");
        // 骨架里显式给了这几项
        assert_eq!(config.lang.as_deref(), Some("zh-Hans"));
        assert_eq!(config.footer.as_deref(), Some("底栏显示在这里"));
        assert_eq!(config.toc.as_deref(), Some("contents"));
    }

    /// 模板嵌套上限默认 64；写 0 是配置错误，不是「不限制」
    #[test]
    fn max_template_depth_defaults_to_64_and_refuses_zero() {
        let base =
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n";
        let mut config: Config = toml::from_str(base).expect("能解析");
        config.normalize();
        config.check().expect("默认值该是好配置");
        assert_eq!(config.max_template_depth, 64);

        let mut zero: Config =
            toml::from_str(&format!("{base}max_template_depth = 0\n")).expect("结构上能解析");
        zero.normalize();
        let error = zero.check().expect_err("0 该被拒绝");
        assert!(error.contains("max_template_depth"), "{error}");
    }

    /// 空文件不再是合法配置：四个必填项缺失，解析时就该说
    #[test]
    fn an_empty_file_is_missing_required_fields() {
        let error = toml::from_str::<Config>("").expect_err("空的该报缺必填项");
        let message = error.to_string();
        assert!(message.contains("site_name"), "{message}");
    }

    /// 拼错的键必须在**解析时**就说 —— 默默用默认值的后果是「目录是空的」
    #[test]
    fn a_misspelled_key_is_refused() {
        let error = toml::from_str::<Config>("page_dir = \"pages\"").expect_err("本该报错");
        assert!(error.to_string().contains("page_dir"), "{error}");
    }

    /// 四个必填项不能是空串 —— 空串等于不填写，而必填必须给
    #[test]
    fn required_fields_refuse_empty_strings() {
        for (field, text) in [
            (
                "site_name",
                "site_name = \"\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n",
            ),
            (
                "pages_dir",
                "site_name = \"甲\"\npages_dir = \"\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n",
            ),
            (
                "templates_dir",
                "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"\"\nbuild_dir = \"b\"\n",
            ),
            (
                "build_dir",
                "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"\"\n",
            ),
        ] {
            let mut config: Config = toml::from_str(text).expect("结构上能解析");
            config.normalize();
            let error = config.check().expect_err("空必填项该被拒绝");
            assert!(error.contains(field), "{field} 没被点名：{error}");
        }
    }

    /// 可选字段写空串等于不填写：归一之后是 None
    #[test]
    fn empty_optional_strings_become_none() {
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\nsite_description = \"\"\nlang = \"\"\nbase_url = \"\"\nrepo_url = \"\"\nfooter = \"\"\ntoc = \"\"\nfavicon = \"\"\nlogo = \"\"\noverride_dir = \"\"\n",
        )
        .expect("能解析");
        config.normalize();
        assert_eq!(config.site_description, None);
        assert_eq!(config.lang, None);
        assert_eq!(config.base_url, None);
        assert_eq!(config.repo_url, None);
        assert_eq!(config.footer, None);
        assert_eq!(config.toc, None);
        assert_eq!(config.favicon, None);
        assert_eq!(config.logo, None);
        assert_eq!(config.override_dir, None);
    }

    /// 主题只认那三个值，写错的在**读配置时**就说；不写默认 system
    #[test]
    fn theme_only_takes_system_light_or_dark() {
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n",
        )
        .expect("能解析");
        config.normalize();
        assert_eq!(config.theme, "system");

        for theme in ["system", "light", "dark"] {
            config.theme = theme.to_string();
            assert!(config.check().is_ok(), "{theme} 该是好的");
        }
        config.theme = "blue".to_string();
        let error = config.check().expect_err("blue 本该报错");
        for value in ["system", "light", "dark"] {
            assert!(error.contains(value), "{error}");
        }
    }

    /// 主题色默认 #2f6fd0，只认十六进制
    #[test]
    fn theme_color_defaults_and_only_takes_hex() {
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n",
        )
        .expect("能解析");
        config.normalize();
        assert_eq!(config.theme_color, "#2f6fd0");

        for good in ["#2f6fd0", "#F0C", " #2f6fd0 "] {
            config.theme_color = good.to_string();
            assert!(config.check().is_ok(), "「{good}」该是好的");
        }
        for bad in ["red", "rgb(1,2,3)", "#12345", "#12345678", " onload=\"x"] {
            config.theme_color = bad.to_string();
            let error = config.check().expect_err("「{bad}」本该报错");
            assert!(error.contains("theme_color"), "{error}");
        }
    }

    /// `base_url` 只认 http/https —— 写别的那个地址在产物里完全看不出来
    #[test]
    fn base_url_takes_http_or_https() {
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n",
        )
        .expect("能解析");
        config.normalize();

        for bad in ["ftp://x/", "/repo/"] {
            config.base_url = Some(bad.to_string());
            assert!(config.check().is_err(), "{bad} 该被拒绝");
        }
        for good in ["https://x.github.io/repo/", "http://127.0.0.1:8080/"] {
            config.base_url = Some(good.to_string());
            assert!(config.check().is_ok(), "{good} 该被接受");
        }
        // 不写或空串都该过（空串归一后是 None）
        config.base_url = None;
        assert!(config.check().is_ok());
    }

    /// 导航的两种写法都能解析；写坏了（空文字、多项表）在解析时就该说
    #[test]
    fn nav_accepts_both_shapes_and_refuses_bad_ones() {
        let config: Config = toml::from_str(
            r#"
site_name = "甲"
pages_dir = "p"
templates_dir = "t"
build_dir = "b"
nav = ["首页", { "指南" = "指南/入门" }]
"#,
        )
        .expect("该解析得动");
        assert_eq!(config.nav.len(), 2);
        assert_eq!(config.nav[0], NavEntry::SameName("首页".to_string()));
        assert_eq!(
            config.nav[1],
            NavEntry::Named(BTreeMap::from([(
                "指南".to_string(),
                "指南/入门".to_string()
            )]))
        );

        // 单项表里写了两项 —— 解析得过（毕竟是个表），check 时该说
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\nnav = [{ \"甲\" = \"a\", \"乙\" = \"b\" }]\n",
        )
        .expect("表里多少项都能解析");
        config.normalize();
        assert!(config.check().is_err(), "多项表不该被接受");

        // 空显示文字也是配置错误
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\nnav = [\"   \"]\n",
        )
        .expect("能解析");
        config.normalize();
        assert!(config.check().is_err());
    }

    /// `toc` 不提供就不生成：默认 None
    #[test]
    fn toc_defaults_to_none() {
        let mut config: Config = toml::from_str(
            "site_name = \"甲\"\npages_dir = \"p\"\ntemplates_dir = \"t\"\nbuild_dir = \"b\"\n",
        )
        .expect("能解析");
        config.normalize();
        assert_eq!(config.toc, None);
        assert!(config.check().is_ok());
    }
}
