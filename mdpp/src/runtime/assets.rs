//! 渲染时产生的 CSS / JS 资源：单页去重内联，站点外链共享。
//!
//! `::css` / `::js` / `::html js` 不再直接往页面里写 `<style>` / `<script>`，
//! 而是先算内容哈希、记入当次渲染的收集器，再按 [`AssetMode`] 决定输出什么：
//!
//! - [`AssetMode::Inline`]（单页）：第一次出现时内联，重复的跳过；
//! - [`AssetMode::Linked`]（站点）：第一次出现时输出链接，内容交给调用方写盘。
//!
//! 在 [`crate::variants`] 的选项作用域里，资源变成 [`DeferredAsset`]：页面里
//! 先放一个 `<template data-mdpp-*>`，选项激活时由前端挂载。这样一份资源可以
//! 只属于某个 tab / 主题，而单文件与静态站点两种产物都不需要服务端。
//!
//! 收集器是线程局部的，与 `context` / `registry` 同一个模式：渲染是同步的，
//! 一次渲染一趟，嵌套渲染（`::html` 里的 `<markdown>`）会把内层资源合并回外层。

use std::collections::HashSet;

use crate::variants;

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

/// 取出当前收集到的资源（只在**最外层**渲染结束时调用）。
pub(crate) fn take_assets() -> Vec<Asset> {
    crate::state::with(|state| {
        state
            .assets
            .as_mut()
            .map(|sink| std::mem::take(&mut sink.assets))
            .unwrap_or_default()
    })
    .unwrap_or_default()
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
/// 由 [`use_asset`] 在 [`Group::enter`](crate::variants::Group::enter) 的作用域里
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

/// 从自定义块的渲染器里登记一份 CSS / JS。
///
/// 不在渲染中时退化为 [`AssetUse::Inline`] —— 与单独调用渲染器的行为一致。
pub fn use_asset(kind: AssetKind, content: impl Into<String>) -> AssetUse {
    use_asset_with(kind, content, AssetPolicy::EveryActivation)
}

/// 同 [`use_asset`]，但显式指定再次激活时的策略。
pub fn use_asset_with(
    kind: AssetKind,
    content: impl Into<String>,
    policy: AssetPolicy,
) -> AssetUse {
    match emit_with(kind, content.into(), policy) {
        Emission::Inline(content) => AssetUse::Inline(content),
        Emission::Linked(url) => AssetUse::Linked(url),
        Emission::Deferred(asset) => AssetUse::Deferred(asset),
        Emission::Duplicate => AssetUse::Duplicate,
    }
}

/// 只在第一次激活时执行的脚本。CSS 用 [`use_asset`] 即可（它总是随选项挂载）。
pub fn use_asset_once(kind: AssetKind, content: impl Into<String>) -> AssetUse {
    use_asset_with(kind, content, AssetPolicy::Once)
}

/// 一份资源在页面里该怎么出现。
pub(crate) enum Emission {
    /// 第一次出现，内联这段内容。
    Inline(String),
    /// 第一次出现，用这个链接。
    Linked(String),
    /// 变体作用域里的条件资源。
    Deferred(DeferredAsset),
    /// 已经出现过，跳过。
    Duplicate,
}

/// 记录一份资源，返回它在页面里的样子。测试里当默认策略的便捷入口。
#[cfg(test)]
pub(crate) fn emit(kind: AssetKind, content: String) -> Emission {
    emit_with(kind, content, AssetPolicy::EveryActivation)
}

/// 记录一份资源，并决定它在页面里的样子。
pub(crate) fn emit_with(kind: AssetKind, content: String, policy: AssetPolicy) -> Emission {
    // 在变体作用域里：内容不直接进页面，而是绑到「组 + 选项」上。
    // 每个占位符都要保留 —— 即使内容相同，不同选项也需要各自的挂载点。
    if let Some(scope) = variants::current() {
        let (hash, href, _) = register(kind, &content);
        let key = format!(
            "{}:{}:{}:{}",
            scope.group,
            scope.value,
            kind.extension(),
            hash
        );
        return Emission::Deferred(DeferredAsset {
            group: scope.group,
            value: scope.value,
            kind,
            key,
            href,
            content,
            policy,
        });
    }

    // 不在渲染中（例如单独调用渲染器）：退化为内联，行为与旧版一致
    if !crate::state::with(|state| state.assets.is_some()).unwrap_or(false) {
        return Emission::Inline(content);
    }
    let (_, link, inserted) = register(kind, &content);
    if !inserted {
        return Emission::Duplicate;
    }
    match link {
        Some(url) => Emission::Linked(url),
        None => Emission::Inline(content),
    }
}

/// 把内容记进当次收集器，返回 `(哈希, 链接, 是否第一次出现)`。
///
/// 延迟资源即使内容重复也要拿到链接：同一个文件可以被多个选项引用，但每个选项
/// 都需要自己的占位符。普通资源仍按「第一次出现」决定是否输出。
fn register(kind: AssetKind, content: &str) -> (String, Option<String>, bool) {
    let hash = hash_of(content);
    let registered = crate::state::with(|state| {
        let sink = state.assets.as_mut()?;
        let inserted = sink.hashes.insert(hash.clone());
        let url = match &sink.mode {
            AssetMode::Inline => None,
            AssetMode::Linked { prefix } => Some(format!(
                "{}/{}",
                prefix.trim_end_matches('/'),
                Asset {
                    kind,
                    hash: hash.clone(),
                    content: String::new(),
                }
                .file_name()
            )),
        };
        if inserted {
            sink.assets.push(Asset {
                kind,
                hash: hash.clone(),
                content: content.to_string(),
            });
        }
        Some((url, inserted))
    });
    match registered {
        Some(Some((url, inserted))) => (hash, url, inserted),
        _ => (hash, None, false),
    }
}

/// 文本节点转义：`&` 与 `<`。`>` 不转义也不会被当成标签开头。
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 属性值转义：文本那两样之外再管双引号。
fn escape_attribute(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}

/// FNV-1a 64 位：不引入额外依赖，内容相同哈希就相同。
fn hash_of(content: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试里装一份只有资源收集器的状态
    fn enter(mode: AssetMode) -> (crate::state::Guard, bool) {
        let guard = crate::state::enter(crate::state::RenderState {
            assets: Some(Sink {
                mode,
                assets: Vec::new(),
                hashes: HashSet::new(),
            }),
            ..Default::default()
        });
        let nested = guard.nested();
        (guard, nested)
    }

    /// 内容一样哈希就一样，长度固定 16 位
    #[test]
    fn same_content_same_hash() {
        assert_eq!(hash_of("a{}"), hash_of("a{}"));
        assert_ne!(hash_of("a{}"), hash_of("b{}"));
        assert_eq!(hash_of("a{}").len(), 16);
    }

    /// 单页模式：同一份资源第一次内联，第二次跳过
    #[test]
    fn inline_mode_dedupes() {
        let (guard, nested) = enter(AssetMode::Inline);
        assert!(!nested);
        assert!(matches!(
            emit(AssetKind::Css, "a{}".into()),
            Emission::Inline(_)
        ));
        assert!(matches!(
            emit(AssetKind::Css, "a{}".into()),
            Emission::Duplicate
        ));
        let assets = take_assets();
        assert_eq!(assets.len(), 1);
        drop(guard);
    }

    /// 站点模式：第一次返回链接，内容留在清单里
    #[test]
    fn linked_mode_returns_url() {
        let (guard, _) = enter(AssetMode::Linked {
            prefix: "assets".into(),
        });
        match emit(AssetKind::Js, "x()".into()) {
            Emission::Linked(url) => {
                assert!(url.starts_with("assets/mdpp-"), "{url}");
                assert!(url.ends_with(".js"), "{url}");
            }
            _ => panic!("应当是链接"),
        }
        let assets = take_assets();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].kind, AssetKind::Js);
        drop(guard);
    }

    /// 嵌套渲染的资源合并回外层，不会被内层守卫丢掉
    #[test]
    fn nested_rendering_merges_assets() {
        let (outer, _) = enter(AssetMode::Inline);
        let _ = emit(AssetKind::Css, "outer{}".into());
        {
            let (inner, nested) = enter(AssetMode::Inline);
            assert!(nested);
            let _ = emit(AssetKind::Css, "inner{}".into());
            // 内层不 take，guard drop 时合并到外层
            drop(inner);
        }
        let assets = take_assets();
        assert_eq!(assets.len(), 2, "{assets:?}");
        drop(outer);
    }

    /// 变体作用域里资源变成带组/选项的模板；内容里的 `<` 与 `&` 不会破坏模板
    #[test]
    fn scoped_assets_become_deferred_templates() {
        let (guard, _) = enter(AssetMode::Inline);
        let group =
            crate::variants::Group::new("tabs", crate::variants::Control::Choice).option("0", "甲");
        let _scope = group.enter("0");
        match emit(AssetKind::Js, "if (a < b && c > d) {}".into()) {
            Emission::Deferred(asset) => {
                assert_eq!(asset.group(), group.id());
                assert_eq!(asset.value(), "0");
                assert!(asset.key().starts_with("tabs-1:0:js:"));
                let html = asset.html();
                assert!(html.contains("data-mdpp-asset=\"js\""), "{html}");
                assert!(html.contains("data-mdpp-group=\"tabs-1\""), "{html}");
                assert!(html.contains("data-mdpp-value=\"0\""), "{html}");
                assert!(html.contains("data-mdpp-version=\"0.1.0\""), "{html}");
                assert!(html.contains("data-mdpp-runtime=\"asset\""), "{html}");
                assert!(html.contains("a &lt; b &amp;&amp; c > d"), "{html}");
                assert!(!html.contains("</template>}\""), "{html}");
            }
            _ => panic!("应当是延迟资源"),
        }
        drop(guard);
    }

    /// 站点模式的延迟资源仍然进清单，占位符只带静态链接
    #[test]
    fn scoped_linked_assets_use_href() {
        let (guard, _) = enter(AssetMode::Linked {
            prefix: "assets".into(),
        });
        let group =
            crate::variants::Group::new("tabs", crate::variants::Control::Choice).option("0", "甲");
        let _scope = group.enter("0");
        match emit(AssetKind::Css, ".a{}".into()) {
            Emission::Deferred(asset) => {
                let html = asset.html();
                assert!(html.contains("data-mdpp-href=\"assets/mdpp-"), "{html}");
                assert!(!html.contains(".a{}"), "外链模式不该把内容塞进页面：{html}");
            }
            _ => panic!("应当是延迟资源"),
        }
        let assets = take_assets();
        assert_eq!(assets.len(), 1, "{assets:?}");
        drop(guard);
    }

    /// 相同的延迟内容会各自保留占位符，但只登记一份文件
    #[test]
    fn duplicate_scoped_assets_keep_both_placeholders() {
        let (guard, _) = enter(AssetMode::Linked {
            prefix: "assets".into(),
        });
        let group = crate::variants::Group::new("tabs", crate::variants::Control::Choice)
            .option("0", "甲")
            .option("1", "乙");
        for value in ["0", "1"] {
            let _scope = group.enter(value);
            assert!(matches!(
                emit(AssetKind::Css, ".same{}".into()),
                Emission::Deferred(_)
            ));
        }
        let assets = take_assets();
        assert_eq!(assets.len(), 1, "{assets:?}");
        drop(guard);
    }

    /// `once` 策略写进模板属性
    #[test]
    fn once_policy_marks_the_template() {
        let (guard, _) = enter(AssetMode::Inline);
        let group =
            crate::variants::Group::new("tabs", crate::variants::Control::Choice).option("0", "甲");
        let _scope = group.enter("0");
        match emit_with(AssetKind::Js, "x()".into(), AssetPolicy::Once) {
            Emission::Deferred(asset) => {
                assert_eq!(asset.policy(), AssetPolicy::Once);
                assert!(asset.html().contains("data-mdpp-once=\"once\""));
            }
            _ => panic!("应当是延迟资源"),
        }
        drop(guard);
    }
}
