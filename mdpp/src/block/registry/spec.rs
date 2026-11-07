use std::sync::Arc;

use markdown_it::{Node, Renderer};

use super::block::{Block, ContentMode};

/// 写出一个 `::` 块：块的头、已解析好的子节点（形状由 [`ContentMode`] 决定）、渲染器。
///
/// 三个生命周期显式写成高阶的，不能省成 `Fn(&Block, &Node, &mut dyn Renderer)`：
/// 省略写法在函数指针的位置上会被定死成某一个具体生命周期，于是接函数指针的那种写法
/// 编译不过。
pub type BlockRenderer =
    dyn for<'a, 'b, 'c> Fn(&'a Block, &'b Node, &'c mut (dyn Renderer + 'c)) + Send + Sync;

/// 注册表里的一条：一个名字 + 一个渲染器 + 内容怎么处理。
#[derive(Clone)]
pub struct BlockSpec {
    pub(super) name: String,
    pub(super) renderer: RendererKind,
    pub(super) mode: ContentMode,
    /// 按**这一块的实际参数/正文**再决定一次内容模式；`None` = 用固定的 `mode`
    pub(super) dynamic: Option<Arc<ContentModeFn>>,
}

/// 动态内容模式：看到块头之后决定这一块走 Markdown / Sections / Raw。
///
/// 扫到 `::名字 …` 时渲染函数还没被调用，所以模式必须在**解析内容之前**定下来；
/// 固定的 [`ContentMode`] 不够用时（同一个块有时要 raw、有时要 markdown），
/// 用 [`BlockSpec::with_mode_fn`] 挂一个这样的函数。
pub type ContentModeFn = dyn Fn(&Block) -> ContentMode + Send + Sync;

/// 渲染器的两种存法。函数指针那条路不必绕 HRTB：把 `fn(&Block, &Node, &mut dyn
/// Renderer)` 包进闭包时，闭包的参数生命周期会按实体推断，于是报
/// `implementation of FnOnce is not general enough`。
#[derive(Clone)]
pub(super) enum RendererKind {
    /// 一个普通的函数指针。本库自带的块全是这一种。
    Plain(fn(&Block, &Node, &mut dyn Renderer)),
    /// 装箱的闭包。外部注册时用的多半是这一种。
    Boxed(Arc<BlockRenderer>),
}

impl RendererKind {
    fn call(&self, block: &Block, node: &Node, fmt: &mut dyn Renderer) {
        match self {
            RendererKind::Plain(run) => run(block, node, fmt),
            RendererKind::Boxed(run) => (run)(block, node, fmt),
        }
    }
}

impl std::fmt::Debug for RendererKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererKind::Plain(_) => f.write_str("fn"),
            RendererKind::Boxed(_) => f.write_str("closure"),
        }
    }
}

impl BlockSpec {
    /// 造一条注册项（渲染器是一个**函数指针**）
    pub fn plain(
        name: impl Into<String>,
        mode: ContentMode,
        renderer: fn(&Block, &Node, &mut dyn Renderer),
    ) -> Self {
        Self {
            name: name.into(),
            renderer: RendererKind::Plain(renderer),
            mode,
            dynamic: None,
        }
    }

    /// 造一条注册项（渲染器是任意**闭包**）
    pub fn new(name: impl Into<String>, mode: ContentMode, renderer: impl BlockRendererFn) -> Self {
        Self {
            name: name.into(),
            renderer: RendererKind::Boxed(Arc::new(renderer.into_boxed())),
            mode,
            dynamic: None,
        }
    }

    /// 块名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 内容怎么处理（固定档；动态档见 [`Self::mode_for`]）
    pub fn mode(&self) -> ContentMode {
        self.mode
    }

    /// 给这一条挂一个「看块头再决定」的模式函数。
    ///
    /// 例如同一个 `::代码` 块：写了 `raw=1` 就按原文、否则按 markdown。
    /// 函数在扫描阶段调用，拿到的 [`Block`] 已经有名字、参数与正文。
    pub fn with_mode_fn(
        mut self,
        choose: impl Fn(&Block) -> ContentMode + Send + Sync + 'static,
    ) -> Self {
        self.dynamic = Some(Arc::new(choose));
        self
    }

    /// 这一块该走哪种内容模式：有动态函数就问它，否则用固定的那一档
    pub fn mode_for(&self, block: &Block) -> ContentMode {
        match &self.dynamic {
            Some(choose) => choose(block),
            None => self.mode,
        }
    }

    /// 调渲染器
    pub fn render(&self, block: &Block, node: &Node, fmt: &mut dyn Renderer) {
        self.renderer.call(block, node, fmt);
    }

    /// 换个名字（注册进注册表之前用；注册之后名字由表决定）
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl std::fmt::Debug for BlockSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockSpec")
            .field("name", &self.name)
            .field("mode", &self.mode)
            .field("dynamic", &self.dynamic.is_some())
            .finish_non_exhaustive()
    }
}

/// [`BlockSpec::new`] 的渲染函数那一参：让 `new` 收**任何**
/// `Fn(&Block, &Node, &mut dyn Renderer)`，调用方不必自己装箱。
pub trait BlockRendererFn {
    /// 装箱成 `Arc<BlockRenderer>`
    fn into_boxed(self) -> Box<BlockRenderer>;
}

impl<F> BlockRendererFn for F
where
    F: Fn(&Block, &Node, &mut dyn Renderer) + Send + Sync + 'static,
{
    fn into_boxed(self) -> Box<BlockRenderer> {
        Box::new(self)
    }
}
