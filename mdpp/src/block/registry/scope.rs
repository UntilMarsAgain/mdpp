//! 当前注册表的线程局部作用域。
//!
//! 纯数据在 [`super::table::BlockRegistry`]；这里只负责把某一份注册表装进
//! [`crate::env`]，供扫描器与分发器读取。

use std::sync::Arc;

use super::spec::BlockSpec;
use super::table::BlockRegistry;

// ---- 当前生效的那一份 --------------------------------------------------
//
// 扫描器与分发器是零大小的 markdown-it 规则，拿不到「自己属于哪个解析器」—— 而一个
// 进程里可以有好几个 `Mdpp`，各带各的注册表。走线程局部，理由与 `crate::context` 相同：
// 渲染是同步的，规则必然跑在同一个线程上。

/// 取当次渲染的注册表
pub fn current() -> Option<Arc<BlockRegistry>> {
    crate::env::with(|env| Arc::clone(&env.registry))
}

/// 查一个块，**只**在当次渲染的注册表里查。扫描器与分发器用它。
pub fn lookup(name: &str) -> Option<BlockSpec> {
    current()?.get(name).cloned()
}

/// 装上注册表，返回用完还原的守卫。存的是**进入之前**的那一份（不是空），
/// 理由见 [`crate::context`] 模块头。
pub struct Guard {
    previous: Option<Arc<BlockRegistry>>,
    /// 进入之前根本没有渲染状态：这个守卫要负责把整份状态撤掉
    had_state: bool,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.had_state {
            let previous = self.previous.take().unwrap_or_default();
            crate::env::with(|env| env.registry = previous);
        } else {
            crate::env::set(None);
        }
    }
}

/// 装上 [`BlockRegistry`]，返回用完还原的守卫。
pub fn enter(registry: Arc<BlockRegistry>) -> Guard {
    let mut pending = Some(registry);
    let mut previous = None;
    let had_state = crate::env::with(|env| {
        previous = Some(std::mem::replace(
            &mut env.registry,
            pending.take().expect("只取一次"),
        ));
    })
    .is_some();
    if !had_state {
        crate::env::set(Some(crate::env::RenderEnv {
            registry: pending.expect("只取一次"),
            ..Default::default()
        }));
    }
    Guard {
        previous,
        had_state,
    }
}
