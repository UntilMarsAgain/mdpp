//! 当前注册表的线程局部作用域。
//!
//! 纯数据在 [`super::table::BlockRegistry`]；这里只负责从 [`crate::env`]
//! 取当前那一份，供扫描器与分发器读取。

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
