use crate::support::color::Color;

use super::text::escape;
use super::{Canonical, Chrome};

/// 标签页上那个小图标。没有就一整行都没有 —— 浏览器自己会画一个
pub(super) fn icons(favicon: Option<&str>) -> String {
    let Some(href) = favicon else {
        return String::new();
    };
    format!("<link rel=\"icon\" href=\"{}\">", escape(href))
}

/// `<head>` 里那两样与「这一页是谁」有关的东西。**哪样没配就没有哪样**
///
/// 空字符串的 `content=""` 与一条指向 `/` 的 canonical 都是**有害的**：前者让搜索引擎
/// 拿一句空话当摘要，后者告诉它「全站只有一页」。所以是「有才写」，不是「写个空的」。
pub(super) fn meta(chrome: &Chrome, canonical: &Canonical) -> String {
    let mut lines = Vec::new();
    let description = chrome.description.trim();
    if !description.is_empty() {
        lines.push(format!(
            "<meta name=\"description\" content=\"{}\">",
            escape(description)
        ));
    }
    if let Some(color) = chrome.theme_color {
        // 浏览器拿它染地址栏（手机上尤其明显）。深浅两份，与顶栏那块底同一个算法
        lines.push(format!(
            "<meta name=\"theme-color\" content=\"{}\">",
            color.css()
        ));
        lines.push(format!(
            "<meta name=\"theme-color\" media=\"(prefers-color-scheme: dark)\" content=\"{}\">",
            color.darkened().css()
        ));
    }
    if let Some(url) = canonical.url() {
        lines.push(format!(
            "<link rel=\"canonical\" href=\"{}\">",
            escape(&url)
        ));
    }
    lines.join("\n")
}

/// 主题色那几层覆盖：把顶栏那块底与压在它上面的字写进 `:root`。
///
/// **四层与 `mdpp.css` 的调色板一模一样**（默认、跟随系统的深色、钉死浅色、钉死深色）——
/// 少一层就有一页掉回默认蓝：`:root[data-mdpp-theme='light']` 的权重比 `:root` 高，
/// 钉死浅色的页面会把默认值捡回来。
///
/// 字色是**构建期**按相对亮度算好的（亮底近黑、暗底白），深色那一档是同一颜色往黑里
/// 混一档 —— 两样都等不到浏览器去算，CSS 里没有可靠的亮度函数。
pub(super) fn bar_color_style(color: Option<Color>) -> String {
    let Some(color) = color else {
        // 没配就没有这一块：那两样变量由 `mdpp.css` 的调色板给默认值
        return String::new();
    };
    let dark = color.darkened();
    let (fg, dark_fg) = (color.on_color().css(), dark.on_color().css());
    let (light, dark) = (color.css(), dark.css());
    format!(
        "<style>\n\
         :root {{ --mdpp-bar-bg: {light}; --mdpp-bar-fg: {fg}; }}\n\
         @media (prefers-color-scheme: dark) {{\n  \
         :root {{ --mdpp-bar-bg: {dark}; --mdpp-bar-fg: {dark_fg}; }}\n\
         }}\n\
         :root[data-mdpp-theme='light'] {{ --mdpp-bar-bg: {light}; --mdpp-bar-fg: {fg}; }}\n\
         :root[data-mdpp-theme='dark'] {{ --mdpp-bar-bg: {dark}; --mdpp-bar-fg: {dark_fg}; }}\n\
         </style>"
    )
}

/// 顶上那一条。**站点名空且没有导航就没有顶栏** —— 判据是内容而不是「用哪种资源」，
/// 于是 `render --full` 那条路不必额外知道自己「不是站点」。
pub(super) fn top_bar(chrome: &Chrome) -> String {
    let name = chrome.name.trim();
    let has_nav = !chrome.nav.is_empty();
    if name.is_empty() && !has_nav {
        return String::new();
    }
    let mut inner = String::new();
    if !name.is_empty() {
        // 图标与站点名**合成一组**（`.mdpp-bar__brand`）：flex 按基线对齐时，一张图
        // 与一行字坐在同一条基线上才对得齐 —— 摊成两个 flex 项的话，图得单独调
        // `align-self`，而那个偏移量取决于字号与图的大小，改样式就得回来改这里
        let logo = chrome.logo.trim();
        let mut brand = String::new();
        if !logo.is_empty() {
            // `alt` 留空：站点名就在旁边，图是**装饰** —— 读屏软件读一遍「图标」是
            // 把同一句话说两遍
            brand.push_str(&format!(
                "<img class=\"mdpp-bar__logo\" src=\"{}\" alt=\"\">",
                escape(logo)
            ));
        }
        brand.push_str(&format!(
            "<span class=\"mdpp-bar__title\">{}</span>",
            escape(name)
        ));
        inner.push_str(&format!("<span class=\"mdpp-bar__brand\">{brand}</span>"));
        let description = chrome.description.trim();
        if !description.is_empty() {
            inner.push_str(&format!(
                "\n<span class=\"mdpp-bar__desc\">{}</span>",
                escape(description)
            ));
        }
        // 仓库链接：协议只认 http/https —— 配置里写个 `javascript:` 就点一下执行一段
        if let Some(url) = web_url(chrome.repo_url.trim()) {
            inner.push_str(&format!(
                "\n<a class=\"mdpp-bar__repo\" href=\"{}\" rel=\"noopener\">仓库</a>",
                escape(url)
            ));
        }
    }
    let top = if inner.is_empty() {
        String::new()
    } else {
        format!("<div class=\"mdpp-bar__inner\">\n{inner}\n</div>\n")
    };
    // 导航那一行：顶栏下面，mkdocs-material 的 tabs 那个位置
    let nav = nav_bar(chrome);
    if top.is_empty() && nav.is_empty() {
        return String::new();
    }
    format!("<div class=\"mdpp-bar\">\n{top}{nav}</div>")
}

/// 导航栏：一行**同款的链接按钮**。没有链接就整行没有
///
/// 当前页那一个只多一个 `aria-current="page"` —— 那是回答读屏软件的「你在这儿」，
/// **不带任何样式**，于是 `site.css` 里也就没有 `.mdpp-nav__link--active` 那条规则
pub(super) fn nav_bar(chrome: &Chrome) -> String {
    if chrome.nav.is_empty() {
        return String::new();
    }
    let mut links = String::new();
    for item in &chrome.nav {
        links.push_str(&format!(
            "<a class=\"mdpp-nav__link\" href=\"{}\"{}>{}</a>",
            escape(&item.href),
            if item.active {
                " aria-current=\"page\""
            } else {
                ""
            },
            escape(&item.label),
        ));
    }
    format!("<nav class=\"mdpp-nav\">\n<div class=\"mdpp-nav__inner\">\n{links}\n</div>\n</nav>")
}

/// 底下那一条。**没写 copyright 就没有底栏** —— 与 mkdocs 一个规矩：
/// 空的那句不必摆在那儿。
///
/// 有 `copyright_html` 时直接输出那段已解析的 HTML（于是 `footer` 里能写
/// `::note`、`[[内部链接]]`、`**粗体**` 之类的 mdpp 语法），没有就退回到
/// 原始 `copyright` 字段（只做 HTML 转义，当纯文本）。
pub(super) fn bottom_bar(chrome: &Chrome) -> String {
    if let Some(html) = &chrome.copyright_html {
        return format!(
            "<div class=\"mdpp-foot\">\n<div class=\"mdpp-foot__inner\">\n\
             <span class=\"mdpp-foot__text\">{html}</span>\n\
             <span class=\"mdpp-foot__made\">由 mdpp 生成</span>\n</div>\n</div>"
        );
    }
    let text = chrome.copyright.trim();
    if text.is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"mdpp-foot\">\n<div class=\"mdpp-foot__inner\">\n\
         <span class=\"mdpp-foot__text\">{}</span>\n\
         <span class=\"mdpp-foot__made\">由 mdpp 生成</span>\n</div>\n</div>",
        escape(text)
    )
}

/// 只认能点开一个网页的协议（`javascript:` 之类不该能进 `href`）
pub(super) fn web_url(value: &str) -> Option<&str> {
    let lowered = value.to_ascii_lowercase();
    (lowered.starts_with("http://") || lowered.starts_with("https://")).then_some(value)
}
