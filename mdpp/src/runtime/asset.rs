//! 资源类型：CSS / JS 的内容、模式、延迟策略与收集器。
//!
//! 这里只放**纯类型**，不读当前渲染环境；真正把资源登记进当次渲染的是
//! [`crate::assets`]。这样 `RenderEnv` 可以只依赖本模块，而 `assets` 服务单向依赖
//! `RenderEnv`，不再形成「环境 ↔ 资源」的模块环。

use std::collections::HashSet;

/// 资源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// 样式表
    Css,
    /// 脚本
    Js,
}

impl AssetKind {
    /// 文件扩展名（`css` / `js`）。
    pub fn extension(self) -> &'static str {
        match self {
            AssetKind::Css => "css",
            AssetKind::Js => "js",
        }
    }
}

/// 一次渲染收集到的一份资源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// 资源类型
    pub kind: AssetKind,
    /// 内容哈希（用于文件名与去重）
    pub hash: String,
    /// 资源内容
    pub content: String,
}

impl Asset {
    /// 建议的文件名：`mdpp-<hash>.<扩展名>`。
    pub fn file_name(&self) -> String {
        format!("mdpp-{}.{}", self.hash, self.kind.extension())
    }
}

/// 资源怎么进页面。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AssetMode {
    /// 单页：去重后内联（默认）。
    #[default]
    Inline,
    /// 站点：输出链接，资源由调用方写盘。
    Linked {
        /// 链接前缀（如 `assets` 或 `../../assets`）。
        prefix: String,
    },
}

/// 延迟脚本再次激活时怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetPolicy {
    /// 每次选项激活都重新挂载并执行一次（默认）。
    #[default]
    EveryActivation,
    /// 只在第一次激活时执行；之后这个资源不再挂载。
    ///
    /// 只对 JS 有意义；CSS 始终随选项激活/失活挂载与移除。
    Once,
}

/// 当次渲染的资源收集器。
pub(crate) struct Sink {
    pub(crate) mode: AssetMode,
    pub(crate) assets: Vec<Asset>,
    /// 已经在 `assets` 里的内容哈希 —— 去重从线性扫描改成一次哈希查找
    pub(crate) hashes: HashSet<String>,
}

impl Default for Sink {
    fn default() -> Self {
        Sink {
            mode: AssetMode::Inline,
            assets: Vec::new(),
            hashes: HashSet::new(),
        }
    }
}

/// 自定义块登记资源时拿到的结果。
///
/// 用法是在渲染器里：
///
/// ```
/// use mdpp::{AssetKind, AssetUse, use_asset};
///
/// let use_ = use_asset(AssetKind::Css, ".mine { color: red; }");
/// match use_ {
///     AssetUse::Inline(css) => { /* 单页模式：把 css 写进 <style> */ }
///     AssetUse::Linked(url) => { /* 站点模式：写 <link href=url> */ }
///     AssetUse::Deferred(asset) => { /* 变体作用域：写 asset.html() */ }
///     AssetUse::Duplicate => { /* 同样的内容已经有人登记过 */ }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetUse {
    /// 第一次出现：把这段内容内联进页面（单页模式）
    Inline(String),
    /// 第一次出现：用这个地址（站点模式）
    Linked(String),
    /// 在变体作用域里：写成惰性占位符，选项激活时再挂载
    Deferred(DeferredAsset),
    /// 已经出现过，跳过
    Duplicate,
}

/// 一份与变体选项绑定的延迟资源。
///
/// 由 [`crate::assets::use_asset`] 在 [`Group::enter`](crate::variants::Group::enter) 的作用域里
/// 自动产生。块作者把它写进正文即可，页面脚本会在选项激活时创建真正的
/// `<style>` / `<script>`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredAsset {
    group: String,
    value: String,
    kind: AssetKind,
    key: String,
    href: Option<String>,
    content: String,
    policy: AssetPolicy,
}

impl DeferredAsset {
    pub(crate) fn new(
        group: String,
        value: String,
        kind: AssetKind,
        key: String,
        href: Option<String>,
        content: String,
        policy: AssetPolicy,
    ) -> Self {
        DeferredAsset {
            group,
            value,
            kind,
            key,
            href,
            content,
            policy,
        }
    }

    /// 惰性占位符的 HTML（一个 `<template data-mdpp-asset=…>`）。
    pub fn html(&self) -> String {
        let kind = self.kind.extension();
        let mut out = String::with_capacity(self.content.len() + 160);
        out.push_str("<template data-mdpp-asset=\"");
        out.push_str(kind);
        out.push_str("\" data-mdpp-group=\"");
        out.push_str(&escape_attribute(&self.group));
        out.push_str("\" data-mdpp-value=\"");
        out.push_str(&escape_attribute(&self.value));
        out.push_str("\" data-mdpp-key=\"");
        out.push_str(&escape_attribute(&self.key));
        out.push_str("\" data-mdpp-version=\"");
        out.push_str(&escape_attribute(crate::VERSION));
        out.push_str("\" data-mdpp-runtime=\"asset");
        out.push('"');
        if let Some(href) = &self.href {
            out.push_str(" data-mdpp-href=\"");
            out.push_str(&escape_attribute(href));
            out.push('"');
        }
        if self.policy == AssetPolicy::Once {
            out.push_str(" data-mdpp-once=\"once\"");
        }
        out.push('>');
        // 内容走文本节点：`<` 与 `&` 必须转义，否则 `</template>` / 实体序列
        // 会被 HTML 解析器提前解释。浏览器读 `.textContent` 时再还原。
        if self.href.is_none() {
            out.push_str(&escape_text(&self.content));
        }
        out.push_str("</template>");
        out
    }

    /// 组 ID。
    pub fn group(&self) -> &str {
        &self.group
    }

    /// 选项值。
    pub fn value(&self) -> &str {
        &self.value
    }

    /// 资源类型。
    pub fn kind(&self) -> AssetKind {
        self.kind
    }

    /// 稳定标识：`组:选项:类型:内容哈希`。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 再次激活时的策略。
    pub fn policy(&self) -> AssetPolicy {
        self.policy
    }
}

/// 文本节点转义：交给 `html-escape`。
fn escape_text(text: &str) -> String {
    html_escape::encode_text(text).into_owned()
}

/// 属性值转义：双引号包值，交给 `html-escape`。
fn escape_attribute(value: &str) -> String {
    html_escape::encode_double_quoted_attribute(value).into_owned()
}
