//! 渲染时产生的 CSS / JS 资源：单页去重内联，站点外链共享。
//!
//! `::css` / `::js` / `::html js` 不再直接往页面里写 `<style>` / `<script>`，
//! 而是先算内容哈希、记入当次渲染的收集器，再按 [`AssetMode`] 决定输出什么：
//!
//! - [`AssetMode::Inline`]（单页）：第一次出现时内联，重复的跳过；
//! - [`AssetMode::Linked`]（站点）：第一次出现时输出链接，内容交给调用方写盘。
//!
//! 收集器是线程局部的，与 `context` / `registry` 同一个模式：渲染是同步的，
//! 一次渲染一趟，嵌套渲染（`::html` 里的 `<markdown>`）会把内层资源合并回外层。

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
///     AssetUse::Duplicate => { /* 同样的内容已经有人登记过 */ }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetUse {
    /// 第一次出现：把这段内容内联进页面（单页模式）
    Inline(String),
    /// 第一次出现：用这个地址（站点模式）
    Linked(String),
    /// 已经出现过，跳过
    Duplicate,
}

/// 从自定义块的渲染器里登记一份 CSS / JS。
///
/// 不在渲染中时退化为 [`AssetUse::Inline`] —— 与单独调用渲染器的行为一致。
pub fn use_asset(kind: AssetKind, content: impl Into<String>) -> AssetUse {
    match emit(kind, content.into()) {
        Emission::Inline(content) => AssetUse::Inline(content),
        Emission::Linked(url) => AssetUse::Linked(url),
        Emission::Duplicate => AssetUse::Duplicate,
    }
}

/// 一份资源在页面里该怎么出现。
pub(crate) enum Emission {
    /// 第一次出现，内联这段内容。
    Inline(String),
    /// 第一次出现，用这个链接。
    Linked(String),
    /// 已经出现过，跳过。
    Duplicate,
}

/// 记录一份资源，返回它在页面里的样子。
pub(crate) fn emit(kind: AssetKind, content: String) -> Emission {
    let hash = hash_of(&content);
    // 不在渲染中（例如单独调用渲染器）：退化为内联，行为与旧版一致
    if !crate::state::with(|state| state.assets.is_some()).unwrap_or(false) {
        return Emission::Inline(content);
    }
    crate::state::with(|state| {
        let sink = state.assets.as_mut().expect("上面刚查过");
        if !sink.hashes.insert(hash.clone()) {
            return Emission::Duplicate;
        }
        let link = match &sink.mode {
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
        sink.assets.push(Asset {
            kind,
            hash,
            content: content.clone(),
        });
        match link {
            Some(url) => Emission::Linked(url),
            None => Emission::Inline(content),
        }
    })
    .expect("上面刚查过")
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
}
