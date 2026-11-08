//! 块级语法：`::名字 key=value …` 开一个块，块的边界由**缩进**划出。
//!
//! ```text
//! ::name key=value key=value
//!   content
//! ```
//!
//! 块内容的边界规则只有一条：**缩进**。头行之后缩进更深的行属于块内，遇到不更深的行
//! 就结束，不需要结束标记。
//!
//! ## 分工
//!
//! - [`scanner`]：块级规则本身 —— 认出 `::` 那一行、往下量缩进、按 [`ContentMode`](crate::registry::ContentMode)
//!   决定内容怎么处理；
//! - [`dispatch`]：按名字分发，查不到就渲染「未知块」框；
//! - [`expand`]：**用户自定义块**的展开（见 [`template`]）—— `::卡片` 拿不到同名内置块
//!   时，去模板页里找 `卡片` 那一页，填好参数再解析；
//! - `section`：严格缩进分节 —— `[标签]` 在基础缩进，正文必须再缩进一层；
//! - [`fill`]：`{{}}` 填空、晚绑定引用与 CSS / HTML 过滤；
//! - [`html`]：`::html` 的净化器 —— 按 HTML5 规范解析后按白名单重建；
//! - [`stdlib`]：随库自带的那些块。加一块 = 写一个渲染器 + 在表里加一行。
//!
//! 块的**类型**与注册表在 [`registry`]（`Block` / `BlockSpec` / `ContentMode`）——
//! 渲染外部注册块时要用的那几件东西都在那儿，本模块只管把它们接起来。

pub mod dispatch;
pub mod expand;
pub mod fill;
pub mod html;
pub mod registry;
pub mod scanner;
mod section;
pub mod stdlib;
pub mod template;

use markdown_it::MarkdownIt;

pub use section::Section;

/// 挂在解析器上的那一行。
///
/// 挂载位置是刻意的：**必须排在段落规则之前**，否则 `::名字` 会先被当成一行普通
/// 文字收走。
pub fn add(md: &mut MarkdownIt) {
    md.block
        .add_rule::<scanner::BlockScanner>()
        .before::<markdown_it::plugins::cmark::block::paragraph::ParagraphScanner>()
        .after_all();
}
