//! 整页外壳：**这个二进制**的活，不是库的。
//!
//! 库（`mdpp`）给的是 HTML **片段**，「外面长什么样、点一下会发生什么」一概不问。所以
//! 外壳、以及它带的那几样东西，都在 `mdpp-cli/assets/`：
//!
//! | 文件 | 是什么 |
//! |---|---|
//! | `site.html` | 页面**模板**：带 `{{占位符}}`，是这个二进制的页面骨架 |
//! | `base.css` | 配色变量 + 通用排版 + 出错时摆的那几个框 |
//! | `callouts.css` `text.css` `media.css` `panels.css` `embeds.css` | 按块分的那几组 |
//! | `print.css` | `@media print`：屏幕上折叠的，打印时都展开 |
//! | `mdpp.js` | **最简**的默认行为：点一下就切选项卡 |
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
//! 双花括号包着名字，一共六个：
//!
//! | 占位符 | 填什么 |
//! |---|---|
//! | `{{lang}}` | `<html lang>`，如 `zh-Hans` |
//! | `{{theme_attr}}` | 主题属性，如 ` data-mdpp-theme="dark"`；没主题就是空 |
//! | `{{title}}` | `<title>` 的内容 |
//! | `{{styles}}` | 样式：要么一段内联的 `<style>`，要么一串 `<link>` |
//! | `{{scripts}}` | 脚本：要么一段内联的 `<script>`，要么一串 `<script src>` |
//! | `{{body}}` | 正文（已渲染的 HTML 片段） |
//!
//! `{{styles}}` / `{{scripts}}` 里也含用户用 `--css` / `--js` 附的那几份 —— 它们接在
//! **默认的后面**，所以「改一下」不必重写默认那份。
//!
//! **模板里不要写注释**：注释会原样进到每一个产物页面里，而注释里若写了这六个名字，
//! 填的时候它们照样会被替换掉 —— 「文档写在模板里」这条路两头都堵。要写说明就写在
//! 本文件的模块头（就是上面这张表），那里它是源码的一部分，不会流到产物里。
//!
//! ## 为什么模板不简单化
//!
//! 「要不要带占位符的模板」看上去是个人口味题，其实是**排障**问题：外壳写死在代码里，
//! 「标题转义对不对」「脚本里混进 `</script` 会怎样」这些都只能靠读代码回答。模板摆在那
//! 儿，页面的骨架就是可以看的。

use std::path::Path;

use include_dir::{Dir, File, include_dir};

/// 页面模板的文件名。它**不**跟着 [`copy_assets`] 拷进产物 —— 它是模板，不是资源
pub const TEMPLATE_FILE: &str = "site.html";

/// 默认脚本的文件名
const SCRIPT_FILE: &str = "mdpp.js";

/// 资源在产物里的目录名（页面上是 `assets/base.css` 这样）
pub const ASSET_DIR: &str = "assets";

/// 编译期嵌进来的那一整个目录（[`include_dir!`]：编译时读磁盘，运行时不碰文件系统）
static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

/// 样式表按这个次序拼起来。
///
/// **次序有意义**：base 在前（它定义那组 `--mdpp-*` 变量，后面的都在用），print 在最后
/// （打印规则是覆盖，得压过前面的）。
const CSS_FILES: [&str; 7] = [
    "base.css",
    "callouts.css",
    "text.css",
    "media.css",
    "panels.css",
    "embeds.css",
    "print.css",
];

/// 默认样式表：下面那几个按次序拼起来的
pub fn default_css() -> String {
    CSS_FILES
        .iter()
        .map(|name| asset(name))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 默认脚本（`assets/mdpp.js` 的内容）
pub fn default_js() -> &'static str {
    asset(SCRIPT_FILE)
}

/// 读一个嵌进来的文件。读不到就带名字炸 —— 那只可能是编译期少了 `include_dir!`
fn asset(name: &str) -> &'static str {
    ASSETS
        .get_file(name)
        .and_then(File::contents_utf8)
        .unwrap_or_else(|| panic!("assets/{name} 没被嵌进来"))
}

/// 样式与脚本怎么进页面
#[derive(Debug, Clone, Copy)]
pub enum Assets {
    /// **全内联**：一个文件就是整个页面，拷走、发给别人、丢进网盘都还是那样。
    /// `render --full` 走这条
    Inline,
    /// **外链**：页面指到 `<前缀>/<文件名>`，文件由 [`copy_assets`] 拷到那儿。
    /// 站点走这条 —— 几十个页面共用一份资源
    Linked(&'static str),
}

/// 一个页面要填进模板的那几样
#[derive(Debug)]
pub struct Page<'a> {
    /// `<title>` 的内容
    pub title: &'a str,
    /// `<html lang>`
    pub lang: &'a str,
    /// 主题：`Some("light" | "dark")` 时设 `data-mdpp-theme`，`::theme` 那两节由它挑
    pub theme: Option<&'a str>,
    /// 正文（已渲染的 HTML 片段）
    pub body: &'a str,
    /// `--css` / `extra_css` 附的那些，接在默认的**后面**
    pub extra_css: &'a [String],
    /// `--js` / `extra_js` 附的那些，接在默认的**后面**
    pub extra_js: &'a [String],
    /// 资源怎么进页面
    pub assets: Assets,
}

/// 按模板填出一个整页。
pub fn document(page: &Page) -> String {
    let styles = match page.assets {
        Assets::Inline => format!("<style>\n{}\n</style>", close_tag(&default_css(), "style")),
        Assets::Linked(prefix) => CSS_FILES
            .iter()
            .map(|name| link_tag(prefix, name))
            .collect::<Vec<_>>()
            .join("\n"),
    };
    let styles = join_blocks(styles, inline_blocks(page.extra_css, "style"));

    let scripts = match page.assets {
        Assets::Inline => format!("<script>\n{}\n</script>", close_tag(default_js(), "script")),
        Assets::Linked(prefix) => script_tag(prefix, SCRIPT_FILE),
    };
    let scripts = join_blocks(scripts, inline_blocks(page.extra_js, "script"));

    let theme_attr = match page.theme {
        Some(theme) => format!(" data-mdpp-theme=\"{}\"", escape(theme)),
        None => String::new(),
    };

    fill(
        template(),
        &[
            ("lang", &escape(page.lang)),
            ("theme_attr", &theme_attr),
            ("title", &escape(page.title)),
            ("styles", &styles),
            ("scripts", &scripts),
            // 正文排最后：它是 HTML，不该被当成还能填占位符的模板
            ("body", page.body),
        ],
    )
}

/// 把 `assets/` 里那些资源拷到 `<dest>/assets/`，模板不拷。
///
/// 站点模式用它：页面指的是 `assets/base.css`，那些字节得真的在那儿。
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
fn template() -> &'static str {
    asset(TEMPLATE_FILE)
}

/// 一个资源文件的名字（那一层是平的，目录里没有子目录）
fn file_name(file: &File) -> String {
    file.path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 资源被抄成 URL 时用的名字：文件名与扩展名都进属性，转义一下
fn asset_url_name(name: &str) -> String {
    escape(name)
}

fn link_tag(prefix: &str, name: &str) -> String {
    format!(
        "<link rel=\"stylesheet\" href=\"{}/{}\">",
        escape(prefix),
        asset_url_name(name)
    )
}

fn script_tag(prefix: &str, name: &str) -> String {
    format!(
        "<script src=\"{}/{}\"></script>",
        escape(prefix),
        asset_url_name(name)
    )
}

/// 用户附带的那几份各自裹一段标签。带闭合标签的那份要处理 —— 它们是任意写来的
fn inline_blocks(extra: &[String], tag: &str) -> Vec<String> {
    extra
        .iter()
        .map(|text| format!("<{tag}>\n{}\n</{tag}>", close_tag(text, tag)))
        .collect()
}

/// 几段接起来，空的丢掉
fn join_blocks(head: String, rest: Vec<String>) -> String {
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
fn close_tag(text: &str, tag: &str) -> String {
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

/// 进 HTML 属性或元素文字的转义。多转义 `&` 与引号没什么坏处
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 一趟扫过去替换占位符。
///
/// **一趟**是刻意的：先填标题再填正文的话，正文里写着 `{{title}}` 会被当成占位符改掉。
/// 认不出的名字与没写全的 `{{` 都原样留着 —— 让作者看见自己写错了，比悄悄吞掉好。
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 256);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(rest);
            return out;
        };
        let key = after[..end].trim();
        out.push_str(&rest[..start]);
        match values.iter().find(|(name, _)| *name == key) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page<'a>(body: &'a str, assets: Assets) -> Page<'a> {
        Page {
            title: "某篇笔记",
            lang: "zh-Hans",
            theme: None,
            body,
            extra_css: &[],
            extra_js: &[],
            assets,
        }
    }

    #[test]
    fn a_full_document_carries_the_stylesheet_the_script_and_the_body() {
        let html = document(&page("<p>正文</p>", Assets::Inline));
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains(".mdpp-content"), "样式没进页面");
        assert!(html.contains("tabs__tab--on"), "脚本没进页面");
        assert!(html.contains("<p>正文</p>"), "{html}");
    }

    /// 「能看」与「能用」是两件事 —— 少了脚本，选项卡点不动，页面仍然「看着对」。
    /// 这条钉的是**脚本在**，钉不住它「有效」；后者靠 `::tabs` 的按钮带
    /// `data-mdpp-block` 之类的约定 —— 而那也只是形状上的线索。
    #[test]
    fn the_script_is_there_so_the_page_is_usable() {
        let html = document(&page(
            "<div class=\"tabs\"></div>",
            Assets::Inline,
        ));
        assert!(html.contains("DOMContentLoaded"), "{html}");
        assert!(html.contains("tabs__tab--on"), "{html}");
    }

    #[test]
    fn a_theme_lands_on_the_html_element() {
        let html = document(&Page {
            theme: Some("dark"),
            ..page("x", Assets::Inline)
        });
        assert!(
            html.contains(r#"<html lang="zh-Hans" data-mdpp-theme="dark">"#),
            "{html}"
        );
    }

    /// 标题常常是文件名，里面带 `&` 与引号很常见 —— 不转义就进不了属性
    #[test]
    fn the_title_is_escaped_into_the_attribute() {
        let html = document(&Page {
            title: r#"a"b&c<d"#,
            ..page("x", Assets::Inline)
        });
        assert!(
            html.contains("<title>a&quot;b&amp;c&lt;d</title>"),
            "{html}"
        );
        assert!(!html.contains("<d"), "{html}");
    }

    #[test]
    fn a_hostile_theme_value_cannot_break_out_of_the_attribute() {
        let html = document(&Page {
            theme: Some("dark\" onload=\"alert(1)"),
            ..page("x", Assets::Inline)
        });
        assert!(!html.contains("onload=\"alert"), "{html}");
        assert!(html.contains("&quot;"), "{html}");
    }

    /// 外链那一路：页面里只有一个 `<link>` 一个 `<script src>`，资源由 `copy_assets` 拷出去
    #[test]
    fn linked_assets_point_at_the_copied_files() {
        let html = document(&page("x", Assets::Linked(ASSET_DIR)));
        assert!(html.contains(r#"<link rel="stylesheet" href="assets/base.css">"#), "{html}");
        assert!(html.contains(r#"<script src="assets/mdpp.js"></script>"#), "{html}");
        // 内联的那些一份都不该在
        assert!(!html.contains("<style>"), "{html}");
        assert!(html.contains("<p>正文</p>") || html.contains("x"), "{html}");
    }

    /// 资源真的被拷出去了，而且模板**不**跟着走 —— 它是模板，不是资源
    #[test]
    fn the_assets_are_copied_out_but_the_template_stays() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let copied = copy_assets(dir.path()).expect("拷得动");

        assert_eq!(copied, CSS_FILES.len() + 1, "七个 css 加一个 js");
        assert!(dir.path().join("assets/base.css").is_file());
        assert!(dir.path().join("assets/print.css").is_file());
        assert!(dir.path().join("assets/mdpp.js").is_file());
        assert!(
            !dir.path().join("assets/site.html").exists(),
            "模板不该进产物"
        );
        // 拷出来的字节与嵌进来的一样
        assert_eq!(
            std::fs::read_to_string(dir.path().join("assets/mdpp.js")).expect("读得动"),
            default_js()
        );
    }

    #[test]
    fn the_stylesheet_is_embedded_and_not_empty() {
        let css = default_css();
        assert!(css.len() > 500, "{}", css.len());
        assert!(css.contains(".mdpp-content"), "正文那一层得有样式");
    }

    /// 七个文件按次序拼成一份 —— 次序错了会静悄悄地看起来对：
    /// 变量那层必须在前（后面都在 `var(--mdpp-…)`），打印那层必须在最后（它是覆盖）。
    #[test]
    fn the_pieces_are_concatenated_in_the_right_order() {
        let css = default_css();
        let base = css.find(":root").expect("变量那层");
        let callout = css.find(".callout {").expect("提示框");
        let tabs = css.find(".tabs__bar").expect("选项卡");
        let print = css.find("@media print").expect("打印");
        assert!(base < callout, "变量要在用它的那些之前");
        assert!(callout < tabs, "按注册次序");
        assert!(tabs < print, "打印规则是覆盖，要压过前面的");
    }

    /// 每个资源都得真的被拼进去 —— 漏一个是静默的：页面少几样样式，而编译不报错。
    #[test]
    fn every_asset_file_reaches_the_stylesheet() {
        let css = default_css();
        for (file, needle) in [
            ("base.css", "--mdpp-accent"),
            ("callouts.css", "callout--warning"),
            ("text.css", "title-block"),
            ("media.css", "figcaption"),
            ("panels.css", "theme__part--dark"),
            ("embeds.css", "math--display"),
            ("print.css", "attr(data-label)"),
        ] {
            assert!(css.contains(needle), "{file} 没被拼进去（少 {needle}）");
        }
    }

    /// `::css` 注入的 CSS 靠 `var(--mdpp-…)` 取配色，变量名要与 `base.css` 对得上
    #[test]
    fn the_palette_variables_are_all_there() {
        let css = default_css();
        for name in [
            "--mdpp-accent",
            "--mdpp-text",
            "--mdpp-text-dim",
            "--mdpp-link",
            "--mdpp-border",
            "--mdpp-surface",
            "--mdpp-danger",
        ] {
            assert!(css.contains(name), "少了 {name}");
        }
    }

    #[test]
    fn the_script_is_embedded_and_does_the_minimum() {
        let js = default_js();
        assert!(js.contains("tabs__tab--on"), "{}", js);
        assert!(js.contains("DOMContentLoaded"), "{}", js);
        assert!(js.contains("addEventListener(\"click\""), "点击切换不能少");
        // 默认脚本多带一样（方向键、动画、记忆…）就有人得自己去拆，
        // 而拆一段别人的行为比不写要费劲
        assert!(!js.contains("ArrowRight"), "方向键不该由官方工具来定");
        assert!(!js.contains("localStorage"), "记住上次那栏不该由官方工具来定");
    }

    /// 附带的那些接在默认的**后面** —— 同优先级时后者胜出，
    /// 所以「改一下」不必把默认那份重写一遍
    #[test]
    fn the_extra_files_come_after_the_defaults() {
        let html = document(&Page {
            extra_css: &["/* 我自己的 */".to_string()],
            extra_js: &["// 我自己的".to_string()],
            ..page("x", Assets::Inline)
        });
        let default_css = html.find(".mdpp-content").expect("默认样式");
        let extra_css = html.find("/* 我自己的 */").expect("附带的样式");
        let default_js = html.find("tabs__tab--on").expect("默认脚本");
        let extra_js = html.find("// 我自己的").expect("附带的脚本");
        assert!(default_css < extra_css, "附带的样式该在默认之后：{html}");
        assert!(default_js < extra_js, "附带的脚本该在默认之后：{html}");
    }

    /// 附带文件里出现闭合标签也不能提前收尾 —— 那是**用户写的**，比默认那份危险。
    /// 默认样式与附带的那几份各是各自的一块 `<style>`，所以数出来是两块
    #[test]
    fn the_extra_files_cannot_close_their_own_tags() {
        let html = document(&Page {
            extra_css: &["a{}</style><p>顶出来了</p>".to_string()],
            extra_js: &[r#"var s = "</script>";"#.to_string()],
            ..page("x", Assets::Inline)
        });
        assert_eq!(html.matches("</style>").count(), 2, "{html}");
        // 用户那半个闭合标签被去掉了，剩下的字留在样式块里当无害的 CSS 文本
        assert!(!html.contains("a{}</style>"), "附带的样式提前收尾了：{html}");
        assert!(!html.contains(r#"</script>";"#), "{html}");
    }

    /// 占位符填**一趟**：正文里写着 `{{title}}` 也不能被改掉
    #[test]
    fn the_body_is_filled_last_so_placeholders_in_it_survive() {
        let html = document(&page("<p>{{title}} 与 {{没有这个}}</p>", Assets::Inline));
        assert!(html.contains("<p>{{title}} 与 {{没有这个}}</p>"), "{html}");
    }

    /// 认不出的占位符原样留着 —— 模板写错一个字要看得见，而不是悄悄吞掉
    #[test]
    fn an_unknown_placeholder_stays_visible() {
        let out = fill("a {{what}} b {{没闭合", &[("known", "1")]);
        assert_eq!(out, "a {{what}} b {{没闭合");
        assert_eq!(fill("{{known}}", &[("known", "1")]), "1");
    }

    /// 模板里要没有那个占位符 —— 少了的话页面照样出得来，只是标题或脚本悄悄没了
    #[test]
    fn the_template_still_has_the_placeholders_the_filler_fills() {
        let template = template();
        for name in ["lang", "theme_attr", "title", "styles", "scripts", "body"] {
            assert!(
                template.contains(&format!("{{{{{name}}}}}") ),
                "模板里少了 {{{{{name}}}}}"
            );
        }
    }
}