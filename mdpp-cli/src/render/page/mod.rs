//! 整页外壳：**这个二进制**的活，不是库的。
//!
//! 库（`mdpp`）给的是 HTML **片段**，「外面长什么样、点一下会发生什么」一概不问。所以
//! 外壳、以及它带的那几样东西，都在 `mdpp-cli/assets/`：
//!
//! | 文件 | 是什么 |
//! |---|---|
//! | `site.html` | 页面**模板**：带 `{{placeholder}}`，是这个二进制的页面骨架 |
//! | `mdpp.css` | **正文唯一的样式表**：配色变量、通用排版、每个块的样子、打印规则 |
//! | `mdpp.js` | **最简**的默认行为：点一下就切选项卡 |
//! | `site.css` | **只在站点里**的那一层外壳：顶栏、那一列的限宽与卡片、页首页尾 |
//!
//! 样式只有这两份：正文的 `mdpp.css` 与站点外壳的 `site.css`，**不再按块拆**。
//!
//! **模板与资源都不写在代码里**：那整个目录由 [`include_dir!`] 在**编译期**嵌进二进制，
//! 于是
//!
//! - `render --full` 把样式与脚本**内联**进去，一个文件就是整个页面，拷走还能看；
//! - `build` 把资源从二进制里**拷出来**到 `assets/`，页面用 `<link>` / `<script src>`
//!   指过去 —— 几十个页面共用一份，不必每页里都塞一遍。
//!
//! 许可与库一样是 MIT，**自取即可** —— 想改就复制一份去改。
//!
//! ## 模板里的占位符
//!
//! 双花括号包着名字，一共十个：
//!
//! | 占位符 | 填什么 |
//! |---|---|
//! | `{{lang_attr}}` | `<html lang="…">` 那一段；不提供语言时是空 |
//! | `{{theme_attr}}` | 主题属性，如 ` data-mdpp-theme="dark"`；不钉主题（`system` 或不写）就是空 |
//! | `{{title}}` | `<title>` 的内容 |
//! | `{{icons}}` | `<link rel="icon">`；没配 favicon 就是空 |
//! | `{{meta}}` | `<meta name="description">` 与 `<link rel="canonical">`；**哪样没配就没有哪样** |
//! | `{{styles}}` | 样式：要么一段内联的 `<style>`，要么一串 `<link>` |
//! | `{{scripts}}` | 脚本：要么一段内联的 `<script>`，要么一串 `<script src>` |
//! | `{{bar}}` | 顶上那一条（**含导航栏**），**连标签一起**；站点名与导航都没有就没有 |
//! | `{{foot}}` | 底下那一条，**连标签一起**；`copyright` 空着就是整段没有 |
//! | `{{body}}` | 正文（已渲染的 HTML 片段） |
//!
//! 这张表与 [`template()`] / [`document`] 两处对不对得上，由
//! `the_template_still_has_the_placeholders_the_filler_fills` 钉着 —— 模板改了名字、
//! 填的时候却还找旧的那个，那条会先红。
//!
//! `{{styles}}` / `{{scripts}}` 里也含用户用 `--css` / `--js` 附的那几份 —— 它们接在
//! **默认的后面**，所以「改一下」不必重写默认那份。
//!
//! **模板里不要写注释**：注释会原样进到每一个产物页面里，而注释里若写了这十个名字，
//! 填的时候它们照样会被替换掉 —— 「文档写在模板里」这条路两头都堵。要写说明就写在
//! 本文件的模块头（就是上面这张表），那里它是源码的一部分，不会流到产物里。
//!
//! ## 为什么模板不简单化
//!
//! 「要不要带占位符的模板」看上去是个人口味题，其实是**排障**问题：外壳写死在代码里，
//! 「标题转义对不对」「脚本里混进 `</script` 会怎样」这些都只能靠读代码回答。模板摆在那
//! 儿，页面的骨架就是可以看的。

use crate::support::color::Color;

mod assets;
mod chrome;
mod text;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use assets::asset;
use assets::template;
pub use assets::{ASSET_DIR, copy_assets, default_css, default_js};

use assets::{
    CORE_CSS, HLJS_CDN, HLJS_CSS_DARK_CDN, HLJS_CSS_LIGHT_CDN, KATEX_CDN, KATEX_CSS_CDN,
    MERMAID_CDN, SCRIPT_FILE, SITE_CSS, close_tag, inline_blocks, join_blocks, link_tag,
    script_tag,
};
use chrome::{bar_color_style, bottom_bar, icons, meta, top_bar};
pub use text::escape;
use text::fill;

/// 样式与脚本怎么进页面
#[derive(Debug, Clone)]
pub enum Assets {
    /// **全内联**：一个文件就是整个页面，拷走、发别人、丢进网盘都还是那样。
    /// `render --full` 走这条
    Inline,
    /// **外链**：页面指到 `<前缀>/<文件名>`，文件由 [`copy_assets`] 拷到那儿。
    /// 站点走这条 —— 几十个页面共用一份资源
    ///
    /// 前缀是**相对这一页**算出来的（`../../assets` 那样）—— 为什么要数层数、
    /// 数错了会怎样（页面光着身子出来，而且不报错），见
    /// [`crate::project::site::assets_href`]
    Linked(String),
}

/// mermaid / math / 代码高亮 前端资源的加载方式。
///
/// `::mermaid` 输出 `<pre class="mermaid">`，`::math` 输出 `<div class="math">`，
/// 代码块产出 `class="language-…"`；它们需要页面里引入 mermaid.js / katex.js /
/// highlight.js（连同 GitHub 浅深两份主题样式）才能在浏览器里画出来、高亮出来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExternalScript {
    /// 从 CDN 引入 `<script src="…">`（默认）
    #[default]
    Cdn,
    /// 不加载任何 mermaid / math / 代码高亮的脚本与样式，由使用方自己负责
    None,
}

impl Assets {
    /// 这一路要引哪几个样式表
    ///
    /// 只有站点那一路多出 [`SITE_CSS`]：单文件页面里没有「整站」这一层外壳。
    fn css_files(&self) -> Vec<&'static str> {
        match self {
            Assets::Inline => vec![CORE_CSS],
            Assets::Linked(_) => vec![CORE_CSS, SITE_CSS],
        }
    }
}

/// 站点那几个「顶栏与底栏」的字段 —— **全是单行字符串**。
///
/// 刻意不给 markdown：页首页尾那一层要改版式、重写一遍，比改配置里一行字麻烦得多。
/// 与 mkdocs 的 `site_name` / `site_description` / `copyright` / `repo_url` 是一回事 ——
/// 顶栏左边一句、右边一句，底栏一句，都是一句话能说完的东西。
///
/// `nav` 是顶栏下面那一行链接 —— 已经解析好了（显示文字、相对这一页的地址、
/// 是不是当前页），由 `build` 那边对着站点查好再递过来；`render --full` 给
/// 默认的空值，于是单文件页面没有导航。
#[derive(Debug, Clone, Default)]
pub struct Chrome {
    /// 顶栏左边的站点名。**空的就是没有顶栏**
    pub name: String,
    /// 站点名**旁边**那个图标的地址（已经带好 `assets/` 前缀与文件名）。
    /// `None` / 空串就是没有那个 `<img>` —— 顶栏只是少了它，其余一字不变
    pub logo: String,
    /// 顶栏上那句说明。顺带进 `<meta name="description">`
    pub description: String,
    /// 顶栏右端那个仓库链接（只给 http/https）
    pub repo_url: String,
    /// 底栏那句话的**原始文本**。空的就是没有底栏。
    ///
    /// 与 `copyright_html` 二选一：有 `copyright_html` 时这条不出场。
    pub copyright: String,
    /// 底栏那句话经过 mdpp 解析后的 HTML 片段。有它时底栏直接输出这段 HTML，
    /// 于是 `footer` 里可以写 `::note`、`[[内部链接]]`、`**粗体**` 之类的 mdpp 语法。
    /// 没配就没有底栏。
    pub copyright_html: Option<String>,
    /// 顶栏那块底的主题色。`None` 就是**不覆盖** —— 默认值在 `mdpp.css` 的调色板里，
    /// 不必每页都带着一份一模一样的
    pub theme_color: Option<Color>,
    /// 导航栏：顶栏下面那一行链接
    pub nav: Vec<NavLink>,
}

/// 导航栏里的一个链接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavLink {
    /// 显示文字
    pub label: String,
    /// 相对**这一页**的地址（`../手册/` 那样）
    pub href: String,
    /// 这一项是不是当前页 —— 只落 `aria-current="page"`，**不改样式**
    pub active: bool,
}

/// 这一页的绝对地址（`<link rel="canonical">`）。没配 `site_url` 就是 `None`
///
/// **页面内那些链接一律仍是相对的** —— 相对地址让整站能挂在任意子路径下；
/// `canonical` 只回答「你在网上是哪一个」那一个问题（给搜索引擎、给分享）。
#[derive(Debug, Clone, Default)]
pub struct Canonical {
    /// 站点根的绝对地址，末尾一定有 `/`。`None` 就是没配 `site_url`
    pub root: Option<String>,
    /// 这一页在站点里的相对路径（`指南/安装/`，首页是空串）
    pub path: String,
}

impl Canonical {
    /// 这一页的完整地址。`None` 表示没配 `site_url`，于是没有那个 `<link>`
    pub fn url(&self) -> Option<String> {
        // 空串当没配 —— 与顶栏那几个字段一个规矩。少了这一层，「配了但写空了」
        // 会产出一条指向 `/` 的 canonical，那是在告诉搜索引擎「全站只有一页」
        let root = self.root.as_deref()?.trim().trim_end_matches('/');
        if root.is_empty() {
            return None;
        }
        Some(format!("{root}/{}", self.path))
    }
}

/// 一个页面要填进模板的那几样
#[derive(Debug)]
pub struct Page<'a> {
    /// `<title>` 的内容
    pub title: &'a str,
    /// `<html lang>`。`None` 就不写这条属性 —— 不提供就不提供。
    pub lang: Option<&'a str>,
    /// 主题：`Some("light" | "dark")` 时设 `data-mdpp-theme`，`::theme` 那两节由它挑。
    /// `Some("system")` 与 `None` **一样**，都是跟随系统 —— 于是属性不落
    pub theme: Option<&'a str>,
    /// 顶栏与底栏那几个字段。`render --full` 那条路给默认的一份（全空，于是两样都不出）
    pub chrome: Chrome,
    /// `<link rel="canonical">` 那个绝对地址。`render --full` 给默认的一份（于是没有那个 `<link>`）
    pub canonical: Canonical,
    /// 标签页上那个小图标的地址（已经带好 `assets/` 前缀与文件名）。
    /// `None` 就没有那个 `<link>`，浏览器自己画一个
    pub favicon: Option<&'a str>,
    /// 正文（已渲染的 HTML 片段）
    pub body: &'a str,
    /// `--css` / `extra_css` 附的那些，接在默认的**后面**
    pub extra_css: &'a [String],
    /// `--js` / `extra_js` 附的那些，接在默认的**后面**
    pub extra_js: &'a [String],
    /// 资源怎么进页面
    pub assets: Assets,
    /// mermaid / math / 代码高亮 前端资源的加载方式
    pub external_script: ExternalScript,
}

/// 按模板填出一个整页。
pub fn document(page: &Page) -> String {
    let styles = match &page.assets {
        // 内联的那一份**不含**站点外壳：单文件页面里没有顶栏、没有那一列
        Assets::Inline => format!("<style>\n{}\n</style>", close_tag(&default_css(), "style")),
        Assets::Linked(prefix) => page
            .assets
            .css_files()
            .iter()
            .map(|name| link_tag(prefix, name))
            .collect::<Vec<_>>()
            .join("\n"),
    };
    // 有没有高亮要引：页面里有 `class="language-…"`（围栏代码块、`::code` 都产出
    // 它）才算。JS 与它的主题样式同进同出，判一次，两处都用。
    let highlight_on =
        page.external_script == ExternalScript::Cdn && page.body.contains("class=\"language-");
    // 数学公式同理：JS 排版 + KaTeX 自己的样式表，缺一不可。
    let math_on = page.external_script == ExternalScript::Cdn && page.body.contains("class=\"math");
    let katex_css = if math_on {
        vec![format!(
            "<link rel=\"stylesheet\" href=\"{KATEX_CSS_CDN}\">"
        )]
    } else {
        Vec::new()
    };
    // highlight.js 的浅深两份主题。位置与主题色那几层同一档：在默认样式之后、
    // 用户附的样式之前 —— `--css` 里自带的配色仍然是最后说话的那个。
    let hljs_themes = if highlight_on {
        vec![
            format!(
                "<link rel=\"stylesheet\" href=\"{}\" media=\"(prefers-color-scheme: light)\">",
                HLJS_CSS_LIGHT_CDN
            ),
            format!(
                "<link rel=\"stylesheet\" href=\"{}\" media=\"(prefers-color-scheme: dark)\">",
                HLJS_CSS_DARK_CDN
            ),
        ]
    } else {
        Vec::new()
    };
    let styles = join_blocks(styles, katex_css);
    let styles = join_blocks(styles, hljs_themes);
    // 主题色那几层覆盖摆在默认样式**之后**、用户附的样式**之前**：它要盖掉
    // `mdpp.css` 调色板里的默认值，而用户 `extra_css` 仍是最后说话的那个
    let styles = join_blocks(styles, vec![bar_color_style(page.chrome.theme_color)]);
    let styles = join_blocks(styles, inline_blocks(page.extra_css, "style"));

    let scripts = match &page.assets {
        Assets::Inline => format!("<script>\n{}\n</script>", close_tag(default_js(), "script")),
        Assets::Linked(prefix) => script_tag(prefix, SCRIPT_FILE),
    };
    // mermaid / math / highlight.js 前端脚本：页面里若用了对应的东西，
    // 需要把对应的 JS 引进来才能在浏览器里渲染。代码块在不在，上方的
    // `highlight_on` 已经判过 —— JS 与那两份主题样式同进同出。
    let mut diagram_scripts = String::new();
    if page.external_script == ExternalScript::Cdn {
        if page.body.contains("class=\"mermaid\"") {
            diagram_scripts.push_str(&format!("<script src=\"{}\"></script>\n", MERMAID_CDN));
        }
        if math_on {
            diagram_scripts.push_str(&format!("<script src=\"{}\"></script>\n", KATEX_CDN));
        }
        if highlight_on {
            diagram_scripts.push_str(&format!("<script src=\"{}\"></script>\n", HLJS_CDN));
        }
    }
    let scripts = join_blocks(scripts, vec![diagram_scripts]);
    let scripts = join_blocks(scripts, inline_blocks(page.extra_js, "script"));

    // 只认 `light` / `dark` 两个值。`system` 与**不写**是一回事：属性不落，
    // `mdpp.css` 里那条 `prefers-color-scheme` 就是它，深浅色当场跟系统走。
    //
    // 白名单之外的字符串连 `<html>` 都碰不到 —— 于是这里没有「转义得对不对」这回事，
    // 配置写错也轮不到这一层来兜（`Config::check` 与 clap 的 value_parser 先就报了）
    let theme_attr = match page.theme {
        Some(theme) if matches!(theme, "light" | "dark") => {
            format!(" data-mdpp-theme=\"{theme}\"")
        }
        _ => String::new(),
    };

    // 不提供语言就**不写**这条属性（空串也一样）—— 与配置里「不填写就不提供」一致
    let lang_attr = match page.lang.map(str::trim).filter(|lang| !lang.is_empty()) {
        Some(lang) => format!(" lang=\"{}\"", escape(lang)),
        None => String::new(),
    };

    fill(
        template(),
        &[
            ("lang_attr", &lang_attr),
            ("theme_attr", &theme_attr),
            ("title", &escape(page.title)),
            ("styles", &styles),
            ("icons", &icons(page.favicon)),
            ("meta", &meta(&page.chrome, &page.canonical)),
            ("scripts", &scripts),
            ("bar", &top_bar(&page.chrome)),
            ("foot", &bottom_bar(&page.chrome)),
            // 正文排最后：它是 HTML，不该被当成还能填占位符的模板
            ("body", page.body),
        ],
    )
}
