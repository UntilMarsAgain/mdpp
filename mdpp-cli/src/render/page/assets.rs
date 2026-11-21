use std::path::Path;

use include_dir::{Dir, File, include_dir};

use crate::project::Project;
use crate::support::limits::read_text;

use tracing::warn;

use super::text::escape;

/// 页面模板的文件名。它**不**跟着 [`copy_assets`] 拷进产物 —— 它是模板，不是资源
pub const TEMPLATE_FILE: &str = "template.html";

/// 默认脚本的文件名
pub const SCRIPT_FILE: &str = "mdpp.js";

/// 资源在产物里的目录名（页面上是 `assets/mdpp.css` 这样）
pub const ASSET_DIR: &str = "assets";

/// 编译期嵌进来的那一整个目录（[`include_dir!`]：编译时读磁盘，运行时不碰文件系统）
static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

/// 正文样式表的文件名。**正文只有这一份**：配色变量、通用排版、每个块的
/// 样子、打印规则都在里面 —— 分发时少一堆文件，也少一处「漏拼一个文件」的
/// 静默故障。
pub const CORE_CSS: &str = "mdpp.css";

/// **只在 `render --full` 里内联**的整页层：浏览器 reset 与 1080px 正文列。
///
/// 站点模式不引它 —— 那一层由 [`SITE_CSS`] 负责，页与站的外壳不同，硬塞进
/// 同一份会让单文件页面带上顶栏/卡片，或者让整站多一套重复的 reset。
pub const PAGE_CSS: &str = "page.css";

/// **站点模式额外**的那一份：顶栏、那一列的限宽与卡片、页首页尾。
///
/// 与正文分开放 —— `render --full` 要的是一个自足的单文件页面，而这一份是给
/// 整站用的外壳，对单文件页面没有意义。
pub const SITE_CSS: &str = "site.css";

/// mermaid.js 的 CDN 地址（jsDelivr）。
pub(super) const MERMAID_CDN: &str = "https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js";

/// KaTeX 的 CDN 地址（jsDelivr）。
pub(super) const KATEX_CDN: &str = "https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js";

/// KaTeX 的样式表。**JS 只管把 TeX 排成 DOM，分式/根号/间距靠这份 CSS**；
/// 只引 JS 的话，公式会出现但排不对。
pub(super) const KATEX_CSS_CDN: &str =
    "https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css";

/// highlight.js 的 CDN 地址（jsDelivr）。
pub(super) const HLJS_CDN: &str =
    "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.7.0/build/highlight.min.js";

/// highlight.js 的 GitHub 主题（浅色 / 深色）的 CDN 地址（jsDelivr）。
///
/// 浅色那份的文件名就没有 `-light` —— highlight.js 的老规矩，`github` 是浅色，
/// 深色才带 `-dark`。两份**都**引着：`<link>` 上的 `media="prefers-color-scheme: …"`
/// 让当时那套深浅色自己挑，切主题不必重新渲染。
pub(super) const HLJS_CSS_LIGHT_CDN: &str =
    "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.7.0/build/styles/github.min.css";
pub(super) const HLJS_CSS_DARK_CDN: &str =
    "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.7.0/build/styles/github-dark.min.css";

/// 一个站点自己的页面外壳与样式/脚本。
///
/// 正常构建优先读项目根下的 `template.html` 与 `assets/` 里那三份；
/// 读不到就用编译期嵌入的默认值 —— 旧项目因此不必先补文件。
/// `--builtin` 直接取嵌入值，不碰项目里的任何一份。
#[derive(Debug, Clone)]
pub struct SiteFiles {
    pub template: String,
    pub core_css: String,
    pub site_css: String,
    pub script: String,
}

impl SiteFiles {
    /// 编译期嵌进来的那一套。
    pub fn embedded() -> Self {
        Self {
            template: template().to_string(),
            core_css: default_css(),
            site_css: asset(SITE_CSS).to_string(),
            script: default_js().to_string(),
        }
    }

    /// 正常构建读站点里的覆盖文件，缺一份就回退那一份的默认值；
    /// `builtin` 时全部用默认值。
    pub fn load(project: &Project, builtin: bool) -> Result<Self, String> {
        if builtin {
            return Ok(Self::embedded());
        }
        let mut fallback = Self::embedded();
        if let Some(template) = read_site_file(project, Path::new(TEMPLATE_FILE), "template.html")?
        {
            warn_missing_placeholders(&template);
            fallback.template = template;
        }
        fallback.core_css = read_site_file(
            project,
            &Path::new(ASSET_DIR).join(CORE_CSS),
            "assets/mdpp.css",
        )?
        .unwrap_or(fallback.core_css);
        fallback.site_css = read_site_file(
            project,
            &Path::new(ASSET_DIR).join(SITE_CSS),
            "assets/site.css",
        )?
        .unwrap_or(fallback.site_css);
        fallback.script = read_site_file(
            project,
            &Path::new(ASSET_DIR).join(SCRIPT_FILE),
            "assets/mdpp.js",
        )?
        .unwrap_or(fallback.script);
        Ok(fallback)
    }
}

/// 自定义模板缺了关键占位符时提醒一句，但不阻止构建：
/// 模板是站点的，作者可能确实想省略某一块。正文丢了通常不是本意。
fn warn_missing_placeholders(template: &str) {
    let missing: Vec<String> = ["body", "styles", "scripts"]
        .into_iter()
        .filter(|name| !template.contains(&format!("{{{{{name}}}}}")))
        .map(|name| format!("{{{{{name}}}}}"))
        .collect();
    if !missing.is_empty() {
        warn!(
            placeholders = %missing.join("、"),
            "站点模板 template.html 缺少占位符；对应内容不会出现在产物里（若有意省略可忽略）"
        );
    }
}

/// 项目里存在就读，不存在就 `None`。边界与符号链接交给 `Project::contained`。
fn read_site_file(project: &Project, path: &Path, what: &str) -> Result<Option<String>, String> {
    let path = project.contained(path, what)?;
    if !path.exists() {
        return Ok(None);
    }
    read_text(&path).map(Some)
}

/// 默认正文样式表：`mdpp.css` 那一份
pub fn default_css() -> String {
    asset(CORE_CSS).to_string()
}

/// 单文件 `--full` 的整页样式：`page.css` 的 reset 与 1080px 限宽
pub fn default_page_css() -> &'static str {
    asset(PAGE_CSS)
}

/// 默认脚本（`assets/mdpp.js` 的内容）
pub fn default_js() -> &'static str {
    asset(SCRIPT_FILE)
}

/// 读一个嵌进来的文件。读不到就带名字炸 —— 那只可能是编译期少了 `include_dir!`
pub(crate) fn asset(name: &str) -> &'static str {
    ASSETS
        .get_file(name)
        .and_then(File::contents_utf8)
        .unwrap_or_else(|| panic!("assets/{name} 没被嵌进来"))
}

/// 测试用：把内建的三份资源拷到 `<dest>/assets/`。
#[cfg(test)]
pub fn copy_assets(dest: &Path) -> Result<usize, String> {
    copy_assets_with(&SiteFiles::embedded(), dest)
}

/// 把站点实际使用的那三份样式/脚本写进 `<dest>/assets/`。
///
/// `template.html` 不拷：它是模板，不是浏览器要请求的资源。`page.css` 也不拷：
/// 它只给 `render --full` 内联，站点那份 reset 与限宽在 `site.css` 里。
pub fn copy_assets_with(files: &SiteFiles, dest: &Path) -> Result<usize, String> {
    let dir = dest.join(ASSET_DIR);
    std::fs::create_dir_all(&dir).map_err(|error| format!("建不了 {}：{error}", dir.display()))?;

    for (name, content) in [
        (CORE_CSS, files.core_css.as_bytes()),
        (SITE_CSS, files.site_css.as_bytes()),
        (SCRIPT_FILE, files.script.as_bytes()),
    ] {
        let to = dir.join(name);
        std::fs::write(&to, content)
            .map_err(|error| format!("写不了 {}：{error}", to.display()))?;
    }
    Ok(3)
}

/// 模板原文
pub(crate) fn template() -> &'static str {
    asset(TEMPLATE_FILE)
}

/// 资源在 URL 里用的名字：就是那个文件名，按**属性值**的规矩转义。
///
/// 转义是必须的：文件名进的是 `href="…"` / `src="…"`，里面一个引号就能把属性截断
pub(super) fn asset_url_name(name: &str) -> String {
    escape(name)
}

pub(super) fn link_tag(prefix: &str, name: &str) -> String {
    format!(
        "<link rel=\"stylesheet\" href=\"{}/{}\">",
        escape(prefix),
        asset_url_name(name)
    )
}

pub(super) fn script_tag(prefix: &str, name: &str) -> String {
    format!(
        "<script src=\"{}/{}\" data-mdpp-version=\"{}\" data-mdpp-mode=\"linked\" data-mdpp-runtime=\"mdpp\"></script>",
        escape(prefix),
        asset_url_name(name),
        mdpp::VERSION
    )
}

/// 用户附带的那几份各自裹一段标签。带闭合标签的那份要处理 —— 它们是任意写来的。
///
/// `script` 比 `style` 多带生成器版本与挂载方式：页面里的脚本可能来自内联资源、
/// 站点外链或变体延迟挂载，`data-mdpp-*` 让脚本自己和诊断工具都能分清。
pub(super) fn inline_blocks(extra: &[String], tag: &str) -> Vec<String> {
    let attrs = if tag == "script" {
        format!(
            " data-mdpp-version=\"{}\" data-mdpp-mode=\"inline\" data-mdpp-runtime=\"extra\"",
            mdpp::VERSION
        )
    } else {
        String::new()
    };
    extra
        .iter()
        .map(|text| format!("<{tag}{attrs}>\n{}\n</{tag}>", close_tag(text, tag)))
        .collect()
}

/// 几段接起来，空的丢掉
pub(super) fn join_blocks(head: String, rest: Vec<String>) -> String {
    std::iter::once(head)
        .chain(rest)
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 去掉可能提前闭合标签的 `</tag`，大小写都算。
///
/// 去掉的是「闭合标签连它后面那个 `>`」，其余原样留着 —— 用户写的是
/// `a</script>b`，出来的仍是 `ab`，而 `<\/script`（只插一个反斜杠）会把内容改掉。
pub(super) fn close_tag(text: &str, tag: &str) -> String {
    let needle = format!("</{tag}");
    // 只折 ASCII 大小写：字节数与原文一致，`lower` 里的偏移可以直接切 `text`。
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0usize;
    let mut search = 0usize;
    while let Some(at) = lower[search..].find(&needle) {
        let start = search + at;
        let end = text[start..]
            .find('>')
            .map(|at| start + at + 1)
            .unwrap_or(text.len());
        out.push_str(&text[copied..start]);
        copied = end;
        search = end;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod close_tag_tests {
    use super::close_tag;

    #[test]
    fn drops_every_close_tag_including_the_last() {
        assert_eq!(close_tag("a</style>b</style>c", "style"), "abc");
        assert_eq!(close_tag("a</STYLE>b", "style"), "ab");
        assert_eq!(close_tag("a</style", "style"), "a");
        assert_eq!(close_tag("a</script>b</script>", "script"), "ab");
    }

    #[test]
    fn keeps_repeated_tags_apart() {
        assert_eq!(close_tag("</style>x</style>y", "style"), "xy");
    }
}
