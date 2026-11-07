//! 变体组：一列选项 + 每个选项自己的延迟资源。
//!
//! 一个 `::tabs`、一个 `::theme`，以及将来任何「同一位置按某个开关显示不同版本」
//! 的块，都可以用同一套结构：一个 [`Group`] 列出一组 [`Variant`]，渲染某个选项的
//! 正文时用 [`Group::enter`] 进它的作用域。作用域里产生的 CSS / JS 会变成
//! [`crate::assets::DeferredAsset`]，由页面脚本在选项激活时再挂载。
//!
//! 组本身**不规定 DOM**：块作者自己决定用按钮、标签页还是别的控件，只把
//! [`Group::container_attrs`] / [`Group::option_attrs`] / [`Group::panel_attrs`]
//! 生成的属性挂到自己的元素上。前端 `mdpp.js` 只认这些 `data-mdpp-*` 属性。
//!
//! ```
//! use mdpp::variants::{Control, Group};
//!
//! let group = Group::new("tabs", Control::Choice)
//!     .option("0", "北岸")
//!     .option("1", "南岸");
//!
//! assert_eq!(group.variants().len(), 2);
//! assert_eq!(group.default_value(), "0");
//! assert_eq!(group.container_attrs()[0].1, group.id());
//! ```
//!
//! ## 选项值 `*`
//!
//! [`ALWAYS`] 是给「无论当前选哪一项都生效」的延迟资源用的：`::theme` 里写在第一
//! 个 `[标签]` 之前的内容就属于这一类。
//!
//! ## DOM 协议
//!
//! 组容器挂 [`Group::container_attrs`]；控件挂 [`Group::option_attrs`]；面板挂
//! [`Group::panel_attrs`]。前端 `mdpp.js` 认这几样：
//!
//! - `data-mdpp-group`：组 ID
//! - `data-mdpp-control`：`choice` / `theme` / `manual`
//! - `data-mdpp-option` / `data-mdpp-panel`：选项值
//! - `data-mdpp-asset` 的 `<template>`：延迟资源；`data-mdpp-value="*"` 表示
//!   祖先面板都激活时一直挂着
//!
//! 前端目标只是**静态页面里的 DOM 挂载**：单文件模式内容在 `<template>` 里，
//! 站点模式指到 `assets/` 下的静态文件，不请求服务端接口。
//!
//! 前端运行时挂在 `window.mdppVariants` 上：`get` / `set` / `refresh` / `rescan` /
//! `onChange` / `hasRun` / `runCount` / `markRun`。每次挂载的脚本都带
//! `data-mdpp-group` / `value` / `key` / `run` / `version`，脚本里可以用
//! `document.currentScript.dataset` 读到自己的环境。

use std::sync::atomic::Ordering;

use crate::env::VariantScope;

/// 无条件生效的选项值。
pub const ALWAYS: &str = "*";

/// 组里的一个选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// 选项值，进 `data-mdpp-option` / `data-mdpp-panel` / 资源的 `data-mdpp-value`
    pub value: String,
    /// 给人看的文字（标签、按钮名）
    pub label: String,
}

/// 选项由谁切换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// 用户点击 `data-mdpp-option` 切换，例如 `::tabs`
    Choice,
    /// 跟随页面主题（`data-mdpp-theme` / `prefers-color-scheme`），例如 `::theme`
    Theme,
    /// 只由页面脚本通过 `window.mdppVariants.set()` 或自定义事件切换
    Manual,
}

impl Control {
    /// 写进 `data-mdpp-control` 的值。
    pub fn as_str(self) -> &'static str {
        match self {
            Control::Choice => "choice",
            Control::Theme => "theme",
            Control::Manual => "manual",
        }
    }
}

/// 渲染期的选项作用域。`Group::enter` 返回它，丢弃时恢复外层作用域。
///
/// 字段不公开：它只负责「现在正在渲染哪一个组的哪一项」这一件事，块作者不需要
/// 自己构造它。
pub struct Scope {
    previous: Option<VariantScope>,
    had_state: bool,
}

/// 一组选项。
#[derive(Debug, Clone)]
pub struct Group {
    id: String,
    control: Control,
    variants: Vec<Variant>,
    default: Option<String>,
}

impl Group {
    /// 新建一个组，自动分配页内唯一 ID（`tabs-1`、`tabs-2`……）。
    ///
    /// `prefix` 只用于让 HTML 里一眼看出这是哪一类组；真正的唯一性由页内计数器给。
    pub fn new(prefix: &str, control: Control) -> Group {
        let prefix = if prefix.trim().is_empty() {
            "group"
        } else {
            prefix
        };
        Group {
            id: next_id(prefix),
            control,
            variants: Vec::new(),
            default: None,
        }
    }

    /// 用指定的 ID 建组。模板作者要跨块引用、或自己接管前端状态时用。
    ///
    /// ID 会原样进 `data-mdpp-group`；同一页里重复由调用方负责。
    pub fn with_id(id: impl Into<String>, control: Control) -> Group {
        Group {
            id: id.into(),
            control,
            variants: Vec::new(),
            default: None,
        }
    }

    /// 加一个选项。顺序就是控件与面板的顺序。
    pub fn option(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.variants.push(Variant {
            value: value.into(),
            label: label.into(),
        });
        self
    }

    /// 一次加一组 `(值, 标签)`。
    pub fn options<I, V, L>(mut self, list: I) -> Self
    where
        I: IntoIterator<Item = (V, L)>,
        V: Into<String>,
        L: Into<String>,
    {
        for (value, label) in list {
            self.variants.push(Variant {
                value: value.into(),
                label: label.into(),
            });
        }
        self
    }

    /// 指定默认选项；不指定就是第一个。
    pub fn default_to(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// 组 ID（进 `data-mdpp-group`）。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 切换方式。
    pub fn control(&self) -> Control {
        self.control
    }

    /// 所有选项，按加入顺序。
    pub fn variants(&self) -> &[Variant] {
        &self.variants
    }

    /// 默认选项值。没选过就是第一个；一个选项都没有时是空串。
    pub fn default_value(&self) -> &str {
        self.default
            .as_deref()
            .or_else(|| self.variants.first().map(|variant| variant.value.as_str()))
            .unwrap_or("")
    }

    /// 组容器的属性。
    pub fn container_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("data-mdpp-group", self.id.clone()),
            ("data-mdpp-control", self.control.as_str().to_string()),
            ("data-mdpp-default", self.default_value().to_string()),
        ]
    }

    /// 一个选项控件的属性。
    pub fn option_attrs(&self, value: &str) -> Vec<(&'static str, String)> {
        vec![("data-mdpp-option", value.to_string())]
    }

    /// 一个选项面板的属性。非默认选项自动带 `hidden`。
    pub fn panel_attrs(&self, value: &str) -> Vec<(&'static str, String)> {
        let mut attrs = vec![("data-mdpp-panel", value.to_string())];
        if value != self.default_value() {
            attrs.push(("hidden", "hidden".to_string()));
        }
        attrs
    }

    /// 进入某个选项的作用域。作用域里产生的资源会自动绑定到这个组/选项。
    pub fn enter(&self, value: &str) -> Scope {
        let next = VariantScope {
            group: self.id.clone(),
            value: value.to_string(),
        };
        let mut previous = None;
        let had_state = crate::env::with(|env| {
            previous = env.variant.replace(next);
        })
        .is_some();
        Scope {
            previous,
            had_state,
        }
    }

    /// [`Group::enter`] 的闭包写法。
    pub fn with_option<T>(&self, value: &str, f: impl FnOnce() -> T) -> T {
        let _scope = self.enter(value);
        f()
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.had_state {
            let previous = self.previous.take();
            crate::env::with(|env| env.variant = previous);
        }
    }
}

/// 当前正在渲染哪个组、哪个选项。不在任何选项里时是 `None`。
pub(crate) fn current() -> Option<VariantScope> {
    crate::env::with(|env| env.variant.clone()).flatten()
}

/// 页内自增的组 ID 计数器。嵌套渲染共享同一个计数，避免两个 `tabs-1` 撞车。
fn next_id(prefix: &str) -> String {
    let number =
        crate::env::with(|env| env.variant_seq.fetch_add(1, Ordering::Relaxed) + 1).unwrap_or(1);
    format!("{prefix}-{number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 组 ID 自动递增，且带上前缀
    #[test]
    fn ids_are_unique_and_prefixed() {
        let _guard = crate::env::enter(crate::env::RenderEnv::default());
        let first = Group::new("tabs", Control::Choice);
        let second = Group::new("tabs", Control::Choice);
        assert!(first.id().starts_with("tabs-"), "{}", first.id());
        assert_ne!(first.id(), second.id());
    }

    /// 默认值：显式给就用它，否则第一个
    #[test]
    fn the_default_is_the_first_variant_unless_set() {
        let group = Group::new("x", Control::Choice)
            .option("a", "甲")
            .option("b", "乙");
        assert_eq!(group.default_value(), "a");

        let group = group.default_to("b");
        assert_eq!(group.default_value(), "b");
        assert!(
            group
                .panel_attrs("b")
                .iter()
                .all(|(name, _)| *name != "hidden")
        );
        assert!(
            group
                .panel_attrs("a")
                .iter()
                .any(|(name, _)| *name == "hidden")
        );
    }

    /// 进入作用域时资源收集看得到组与选项，退出后恢复外层
    #[test]
    fn entering_restores_the_outer_scope() {
        let _guard = crate::env::enter(crate::env::RenderEnv::default());
        let group = Group::new("x", Control::Choice).option("a", "甲");
        let outer = group.enter("a");
        assert_eq!(
            current().as_ref().map(|scope| scope.value.as_str()),
            Some("a")
        );

        let inner = Group::new("y", Control::Choice).option("b", "乙");
        {
            let _inner = inner.enter("b");
            assert_eq!(
                current().as_ref().map(|scope| scope.group.as_str()),
                Some(inner.id())
            );
        }
        assert_eq!(
            current().as_ref().map(|scope| scope.group.as_str()),
            Some(group.id())
        );
        drop(outer);
        assert!(current().is_none());
    }
}
