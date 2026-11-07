use std::path::Path;

use include_dir::{Dir, File, include_dir};

use super::text::escape;

/// 页面模板的文件名。它**不**跟着 [`copy_assets`] 拷进产物 —— 它是模板，不是资源
pub const TEMPLATE_FILE: &str = "site.html";

/// 默认脚本的文件名
pub(super) const SCRIPT_FILE: &str = "mdpp.js";

/// 资源在产物里的目录名（页面上是 `assets/mdpp.css` 这样）
pub const ASSET_DIR: &str = "assets";

/// 编译期嵌进来的那一整个目录（[`include_dir!`]：编译时读磁盘，运行时不碰文件系统）
static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

/// 正文样式表的文件名。**正文只有这一份**：配色变量、通用排版、每个块的
/// 样子、打印规则都在里面 —— 分发时少一堆文件，也少一处「漏拼一个文件」的
/// 静默故障。
pub(super) const CORE_CSS: &str = "mdpp.css";

/// **站点模式额外**的那一份：顶栏、那一列的限宽与卡片、页首页尾。
///
/// 与正文分开放 —— `render --full` 要的是一个自足的单文件页面，而这一份是给
/// 整站用的外壳，对单文件页面没有意义。
pub(super) const SITE_CSS: &str = "site.css";

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

/// 默认样式表：正文那唯一的一份
pub fn default_css() -> String {
    asset(CORE_CSS).to_string()
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

/// 把 `assets/` 里那些资源拷到 `<dest>/assets/`，模板不拷。
///
/// 站点模式用它：页面指的是 `assets/mdpp.css`，那些字节得真的在那儿。
pub fn copy_assets(dest: &Path) -> Result<usize, String> {
    let dir = dest.join(ASSET_DIR);
    std::fs::create_dir_all(&dir).map_err(|error| format!("建不了 {}：{error}", dir.display()))?;

    let mut copied = 0usize;
    for file in ASSETS.files() {
        if file.path() == std::path::Path::new(TEMPLATE_FILE) {
            continue;
        }
        let to = dir.join(file_name(file));
        std::fs::write(&to, file.contents())
            .map_err(|error| format!("写不了 {}：{error}", to.display()))?;
        copied += 1;
    }
    Ok(copied)
}

/// 模板原文
pub(crate) fn template() -> &'static str {
    asset(TEMPLATE_FILE)
}

/// 一个资源文件的名字（那一层是平的，目录里没有子目录）
pub(super) fn file_name(file: &File) -> String {
    file.path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
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
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut cursor = 0usize;
    while let Some(at) = lower[cursor..].find(&needle) {
        let start = cursor + at;
        let end = rest[start..]
            .find('>')
            .map(|at| start + at + 1)
            .unwrap_or(text.len());
        out.push_str(&rest[..start]);
        rest = &rest[end..];
        cursor = end;
    }
    out.push_str(rest);
    out
}
