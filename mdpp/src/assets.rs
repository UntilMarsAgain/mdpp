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

use std::cell::RefCell;

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
struct Sink {
    mode: AssetMode,
    assets: Vec<Asset>,
}

thread_local! {
    static CURRENT: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

/// 进入一次渲染的守卫：出栈时恢复外层收集器，并把内层资源合并回去。
pub(crate) struct Guard {
    previous: Option<Sink>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        CURRENT.with(|cell| {
            let mut cell = cell.borrow_mut();
            let inner = cell.take();
            *cell = self.previous.take();
            // 嵌套渲染（`::html` 里的 `<markdown>`）的资源合并回外层
            if let (Some(inner), Some(outer)) = (inner, cell.as_mut()) {
                for asset in inner.assets {
                    if !outer.assets.iter().any(|one| one.hash == asset.hash) {
                        outer.assets.push(asset);
                    }
                }
            }
        });
    }
}

/// 进入一次渲染。返回守卫与「是不是嵌套渲染」。
pub(crate) fn enter(mode: AssetMode) -> (Guard, bool) {
    let previous = CURRENT.with(|cell| cell.borrow_mut().take());
    let nested = previous.is_some();
    CURRENT.with(|cell| {
        *cell.borrow_mut() = Some(Sink {
            mode,
            assets: Vec::new(),
        });
    });
    (Guard { previous }, nested)
}

/// 取出当前收集到的资源（只在**最外层**渲染结束时调用）。
pub(crate) fn take_assets() -> Vec<Asset> {
    CURRENT.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .map(|sink| std::mem::take(&mut sink.assets))
            .unwrap_or_default()
    })
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
    CURRENT.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(sink) = cell.as_mut() else {
            // 不在渲染中（例如单独调用渲染器）：退化为内联，行为与旧版一致
            return Emission::Inline(content);
        };
        if sink.assets.iter().any(|asset| asset.hash == hash) {
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
