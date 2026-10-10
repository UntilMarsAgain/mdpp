//! 随库附带的东西：默认样式表与那份最简的默认脚本。
//!
//! ## 为什么它们在库里
//!
//! 这两样是**本库自己的输出**要用的：样式表的对象是本库渲染出来的那些 class，
//! `::css` 注入的 CSS 要用 `var(--mdpp-accent)` 取配色 —— 变量名与这份文件是
//! 同一件事，拆到两个地方就会各自漂移。
//!
//! **但本库从不注入它们。** 它只把内容给你（[`DEFAULT_CSS`] / [`DEFAULT_JS`]），
//! 往哪放、要不要放，是**界面**那一层的事。`mdpp --full` 就是那份界面：它把两样
//! 都内联进一个文档，于是拷走那个文件也还是那样。
//!
//! ## 想换一份
//!
//! 复制 [`DEFAULT_CSS`] 那一段文字改；或者不复制，用
//! `mdpp --css 你的.css` 在默认那份后面再附一份（后写的同优先级胜出）。

/// 默认样式表（[`assets/mdpp.css`](../../assets/mdpp.css) 的内容）。
///
/// 含打印规则（`@media print`）：屏幕上折叠的东西，打印时都展开。
pub const DEFAULT_CSS: &str = include_str!("../assets/mdpp.css");

/// 默认脚本（[`assets/mdpp.js`](../../assets/mdpp.js) 的内容）。
///
/// 只做一件事：让 `::tabs` 点得动。**刻意只有这一点** —— 方向键、动画、记住上次
/// 选中的那一栏这些都属于各自的界面，官方工具替所有人定下来反而碍事；要加就自己
/// 加，或者 `mdpp --js 你的.js` 附带进来。
pub const DEFAULT_JS: &str = include_str!("../assets/mdpp.js");

#[cfg(test)]
mod tests {
    use super::{DEFAULT_CSS, DEFAULT_JS};

    #[test]
    fn the_stylesheet_is_embedded_and_not_empty() {
        assert!(
            DEFAULT_CSS.len() > 500,
            "样式表该是内进来的：{}",
            DEFAULT_CSS.len()
        );
        assert!(DEFAULT_CSS.contains(".mdpp-content"), "正文那一层得有样式");
    }

    #[test]
    fn the_script_is_embedded_and_not_empty() {
        assert!(DEFAULT_JS.contains("tabs__tab--on"), "{}", DEFAULT_JS);
        assert!(DEFAULT_JS.contains("DOMContentLoaded"), "{}", DEFAULT_JS);
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

    /// 默认脚本只做**最小可用**的那点。它多带一样（方向键、动画、记忆…）就有人得
    /// 自己去拆，而拆一段别人的行为比不写要费劲。
    #[test]
    fn the_default_script_stays_minimal() {
        assert!(
            DEFAULT_JS.contains("addEventListener(\"click\""),
            "点击切换不能少"
        );
        assert!(
            !DEFAULT_JS.contains("ArrowRight"),
            "方向键不该由官方工具来定"
        );
        assert!(
            !DEFAULT_JS.contains("localStorage"),
            "记住上次选中那栏不该由官方工具来定"
        );
        assert!(
            DEFAULT_JS.contains("data-mdpp-bound"),
            "防重复绑定的标记不能少"
        );
    }
}
