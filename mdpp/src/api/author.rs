//! 写自定义块、模板页时用的**稳定工具面**。
//!
//! 这一层只放「不知道具体块名」的东西：
//!
//! - 填空：`{{param}}` / `{{body}}`（缺失即报错），`{{?param}}` 可选；
//! - 分节：把 `[标签]` 切出来的 `Section` 收成 `(标签, 节点)`；
//! - 净化：HTML 白名单、CSS 收窄、`</style>` / `</script>` 防护；
//! - 资源：把自定义 CSS / JS 交给页面（单页内联、站点外链）；在变体作用域里
//!   自动变成 `DeferredAsset`，由页面脚本在选项激活时挂载；
//! - 变体组：[`Group`](crate::variants::Group) 列出一组选项，内置的 `::variants`
//!   与将来的模板都可以复用同一套 `data-mdpp-*` 协议；
//! - 诊断：名字不认识/参数用错时那两个框。
//!
//! 具体块的 DOM（`.tabs`、`.callout`、`<figure>` …）**不在这里** —— 那是
//! [`crate::block::stdlib`] 的呈现选择，不是扩展契约。要复用分节数据、自己摆
//! DOM，就拿 [`sections`]；要让现成的交互脚本认领，就把
//! [`Group::container_attrs`](crate::variants::Group::container_attrs) /
//! `option_attrs` / `panel_attrs` 挂到自己的元素上。
//!
//! ```
//! use mdpp::author::{render_problem, sections};
//! use mdpp::markdown_it::{Node, Renderer};
//! use mdpp::{BlockRegistry, ContentMode, Mdpp};
//!
//! let mut registry = BlockRegistry::with_builtins();
//! registry.register_fn("我的面板", ContentMode::Sections, |block, node, fmt| {
//!     let sections = sections(node);
//!     if sections.is_empty() {
//!         render_problem(block, fmt, "至少要有一节 [标签]");
//!         return;
//!     }
//!     fmt.open("div", &[("class", "mine".to_string())]);
//!     for (label, content) in sections {
//!         fmt.open("section", &[]);
//!         fmt.text(label);
//!         fmt.contents(&content.children);
//!         fmt.close("section");
//!     }
//!     fmt.close("div");
//! });
//!
//! let mdpp = Mdpp::builder().registry(registry).build();
//! let html = mdpp.render("::我的面板\n  [甲]\n    内容\n");
//! assert!(html.contains(r#"class="mine""#), "{html}");
//! ```

pub use crate::assets::{
    AssetKind, AssetPolicy, AssetUse, DeferredAsset, use_asset, use_asset_once, use_asset_with,
};
pub use crate::block::Section;
pub use crate::block::dispatch::{render_problem, render_unknown, sections};
pub use crate::block::fill::{
    CONTENT_SCOPE, allows_js, escape_script_end, flag, sanitize_css, sanitize_html, scope_css,
    source_of, substitute, substitute_checked,
};
pub use crate::block::html::sanitize;
pub use crate::engine::render_nested;
pub use crate::registry::{Block, BlockRegistry, BlockSpec, ContentMode};
pub use crate::substitute_placeholders;
pub use crate::variants;
pub use crate::variants::{ALWAYS, Control, Group, Variant};
