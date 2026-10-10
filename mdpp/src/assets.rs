//! 随库附带的东西：目前只有那份默认样式表。
//!
//! 样式**在库里**而不是在调用方那边，理由是 `::css` 注入的 CSS 要用
//! `var(--mdpp-accent)` 取到本库的配色 —— 变量名与这份文件是同一件事，
//! 拆到两个仓库里就会各自漂移。
//!
//! 要换一份样式：复制 [`DEFAULT_CSS`] 那一段文字改，自己内联进页面。
//!
//! ## 这里**没有**整页外壳
//!
//! 「把片段套成一个页面、里面内联什么」是**界面**那一层的事，不是解析器的。
//! 命令行那边有一份（`mdpp-cli` 的 `page.rs`），它同时内联了样式与那段让
//! `::tabs` 切得动的脚本 —— 页面不只能看，还能用。

/// 默认样式表（[`assets/mdpp.css`](../../assets/mdpp.css) 的内容）。
///
/// 内联进文档就能用。`mdpp --full` 就是这么干的（它那边还多内联一段脚本，
/// 好让页面不只能看、还能用）。
pub const DEFAULT_CSS: &str = include_str!("../assets/mdpp.css");

#[cfg(test)]
mod tests {
    use super::DEFAULT_CSS;

    #[test]
    fn the_stylesheet_is_embedded_and_not_empty() {
        assert!(
            DEFAULT_CSS.len() > 500,
            "样式表该是内进来的：{}",
            DEFAULT_CSS.len()
        );
        assert!(DEFAULT_CSS.contains(".mdpp-content"), "正文那一层得有样式");
    }

    /// 样式表里带变量 —— `::css` 注入的 CSS 靠 `var(--mdpp-…)` 取配色，
    /// 而变量名要与这份文件对得上
    #[test]
    fn the_palette_variables_the_callers_rely_on_are_all_here() {
        for name in [
            "--mdpp-accent",
            "--mdpp-text",
            "--mdpp-text-dim",
            "--mdpp-link",
            "--mdpp-border",
            "--mdpp-surface",
            "--mdpp-danger",
        ] {
            assert!(DEFAULT_CSS.contains(name), "少了 {name}");
        }
    }

    /// 折叠的东西（选项卡、深浅色两节）打印时都要展开 —— 否则印出来是整页的
    /// 第一栏，而读者以为那就是全部。
    ///
    /// 钉的是**这段在**，钉不住它「有效」；那要看 CSS 写得对不对。
    #[test]
    fn the_print_rules_are_in_the_stylesheet() {
        assert!(DEFAULT_CSS.contains("@media print"), "没有打印规则");
        assert!(
            DEFAULT_CSS.contains(".tabs__panel[hidden]"),
            "打印时选项卡的面板应当展开"
        );
        assert!(
            DEFAULT_CSS.contains(".theme__part"),
            "打印时深浅色两节都要出来"
        );
    }

    /// 打印时把地址写出来：纸上看不见链接指向哪儿
    #[test]
    fn printed_links_show_their_target() {
        assert!(DEFAULT_CSS.contains("attr(href)"), "打印时没写出地址");
    }
}
