use std::sync::Arc;

use markdown_it::{Node, Renderer};

use super::block::{Block, ContentMode};
use super::spec::BlockSpec;

/// `::` 块的注册表。有序：诊断与帮助按注册次序列出，加了什么一眼看得见。
#[derive(Clone, Default)]
pub struct BlockRegistry {
    specs: Vec<BlockSpec>,
}

impl BlockRegistry {
    /// 空表（一个块也没有）
    pub fn new() -> Self {
        Self::default()
    }

    /// 装上全部内置块
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        crate::block::stdlib::register(&mut registry);
        registry
    }

    /// [`Self::builtin`] 的**别名**，两份逐字等价，行为没有任何区别。
    ///
    /// 留两个名字是因为读起来各是一句话：`builtin()` 是「就是内置的那张表」，
    /// `with_builtins()` 是「**先**灌一份内置的，**接着**改我自己的」—— 后者是给自己
    /// 那张表加块时的常规起点。选哪个都行
    pub fn with_builtins() -> Self {
        Self::builtin()
    }

    /// 注册一个块。同名**覆盖**。
    pub fn register(&mut self, spec: BlockSpec) -> &mut Self {
        match self.specs.iter_mut().find(|one| one.name == spec.name) {
            Some(slot) => *slot = spec,
            None => self.specs.push(spec),
        }
        self
    }

    /// [`BlockRegistry::register`] 的简写
    pub fn register_fn(
        &mut self,
        name: impl Into<String>,
        mode: ContentMode,
        renderer: impl Fn(&Block, &Node, &mut dyn Renderer) + Send + Sync + 'static,
    ) -> &mut Self {
        self.register(BlockSpec::new(name, mode, renderer))
    }

    /// 摘掉一个块（拿到手可以改名字再注册回去）
    pub fn remove(&mut self, name: &str) -> Option<BlockSpec> {
        let at = self.specs.iter().position(|one| one.name == name)?;
        Some(self.specs.remove(at))
    }

    /// 查一个块
    pub fn get(&self, name: &str) -> Option<&BlockSpec> {
        self.specs.iter().find(|one| one.name == name)
    }

    /// 有没有这个块
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// 所有块名，按注册次序
    pub fn names(&self) -> Vec<&str> {
        self.specs.iter().map(BlockSpec::name).collect()
    }

    /// 表里有几个块
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// 表是空的
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// 迭代注册项
    pub fn iter(&self) -> impl Iterator<Item = &BlockSpec> {
        self.specs.iter()
    }
}

impl std::fmt::Debug for BlockRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockRegistry")
            .field("blocks", &self.names())
            .finish()
    }
}

// ---- 当前生效的那一份 --------------------------------------------------
//
// 扫描器与分发器是零大小的 markdown-it 规则，拿不到「自己属于哪个解析器」—— 而一个
// 进程里可以有好几个 `Mdpp`，各带各的注册表。走线程局部，理由与 `context` 相同：
// 渲染是同步的，规则必然跑在同一个线程上。

/// 取当次渲染的注册表
pub fn current() -> Option<Arc<BlockRegistry>> {
    crate::state::with(|state| state.registry.clone()).flatten()
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
            let previous = self.previous.take();
            crate::state::with(|state| state.registry = previous);
        } else {
            crate::state::set(None);
        }
    }
}

/// 装上 [`BlockRegistry`]，返回用完还原的守卫。
pub fn enter(registry: Arc<BlockRegistry>) -> Guard {
    let mut pending = Some(registry);
    let mut previous = None;
    let had_state = crate::state::with(|state| {
        previous = state.registry.replace(pending.take().expect("只取一次"));
    })
    .is_some();
    if !had_state {
        let mut state = crate::state::default_state();
        state.registry = pending;
        crate::state::set(Some(state));
    }
    Guard {
        previous,
        had_state,
    }
}
