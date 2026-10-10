//! `::` 块的注册表：**外部**在这里挂自己的块，**内部**在这里派发。
//!
//! ## 注册表是普通数据
//!
//! [`BlockRegistry`] 不含解析器，也不含全局状态 —— 想要一份自定义的注册表，构造一份
//! 灌进去，交给 [`MdppBuilder::registry`](crate::engine::MdppBuilder::registry) 即可。
//! 两个 [`Mdpp`](crate::engine::Mdpp) 各带各的注册表，互不相干。
//!
//! ## 一块要向注册表交代两件事
//!
//! 除了渲染函数，还必须说清**块内容怎么处理**（[`ContentMode`]）。这一条不是可选项：
//! 扫描器要靠它在**内容被解析之前**决定走哪条路（要不要分节、要不要干脆不解析），
//! 那时渲染函数还没有被调用过。
//!
//! ## 重名的处理
//!
//! [`BlockRegistry::register`] 覆盖 —— 这是「改掉一个内置块」最短的路。
//! [`BlockRegistry::builtin`] 内部走的是同一套，所以不存在「内置的表」与
//! 「用户的表」两张表对不上的可能（那种半截状态最难查）。

use std::cell::RefCell;
use std::sync::Arc;

use markdown_it::{Node, Renderer};

/// 写出一个 `::` 块。
///
/// 参数：`块的头`（名字、参数、正文原文）、**已解析好的子节点**、渲染器。
///
/// 「已解析好的子节点」是 `ContentMode` 决定的：见 [`ContentMode`]。
///
/// 三个生命周期**显式写成高阶的**（`for<'a,'b,'c>`），不能省成
/// `Fn(&Block, &Node, &mut dyn Renderer)`：省略写法在函数指针类型的位置上会被
/// 定死成某一个具体生命周期，于是「把一个函数指针接成渲染器」那种写法编译不过
/// （报 `implementation of FnOnce is not general enough`）。
pub type BlockRenderer =
    dyn for<'a, 'b, 'c> Fn(&'a Block, &'b Node, &'c mut (dyn Renderer + 'c)) + Send + Sync;

/// 块内容怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentMode {
    /// 交给整个解析器再解析一遍，子节点挂在这个块下。模板里因此能写 markdown、
    /// 能写内部链接，也能再嵌 `::` 块。**绝大多数块都是这个。**
    #[default]
    Markdown,
    /// 按 `[标签]` 行切成几节，**各节各解析一遍**，每节是一个子节点。
    ///
    /// 用于「一节是一篇独立的小文档」的那类块（选项卡、主题面板）。
    /// 为什么值得单列一档：一节里能写列表、能写链接、能再嵌块，
    /// 而大块里的一段普通文字做不到这些。
    Sections,
    /// **不解析**，原文交给渲染器。
    ///
    /// 存在的理由有两条，都不是「省时间」：
    ///
    /// - 正确性：TeX、图表定义、样式里的记号各有各的语法，按 markdown 走一遍会被
    ///   改写（`$x$`、`*`、`_`、行首的 `#` 全都是 markdown 的记号）。不解析就是原文。
    /// - 性能：块规则在这一行重新进整个解析器，于是嵌套 N 层时代价随 N 超线性增长
    ///   （实测每层约 ×1.95）。不解析的块直接把那一层摘掉。
    Raw,
}

/// 一个 `::` 块的头。
///
/// 一次解析产出一份：名字、参数、块正文。内容本身不在这里 —— 它要么解析成子节点
/// （[`ContentMode::Markdown`] / [`ContentMode::Sections`]），要么就只是这段原文
/// （[`ContentMode::Raw`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// `::` 后面的名字（派发就用它）
    pub name: String,
    /// `key=value` 参数；只写了 `key` 的开关式参数，值是空串
    pub params: Vec<(String, String)>,
    /// 块内容原文，已去掉公共缩进
    pub body: String,
}

impl Block {
    /// 解析头一行：`::名字 key=value …`。不是头就返回 `None`。
    ///
    /// 名字必须紧跟在 `::` 之后，且不能含 `=` / `:` —— 那两个字符属于参数与
    /// 「目标前缀」，出现在名字里几乎一定是写错了。宁可退回普通段落，也不要认出一个坏块。
    pub fn parse_header(line: &str) -> Option<(String, Vec<(String, String)>)> {
        let rest = line.trim_start().strip_prefix("::")?;
        let mut tokens = tokenize(rest)?.into_iter();
        let name = tokens.next()?;
        if name.is_empty() || name.contains('=') || name.contains(':') {
            return None;
        }
        // `=` 按**第一个**切：`key="a=b"` 的值就是 `a=b`。
        let params = tokens
            .map(|token| match token.split_once('=') {
                Some((key, value)) => (key.to_string(), value.to_string()),
                None => (token.to_string(), String::new()),
            })
            .collect();
        Some((name, params))
    }

    /// 取一个参数
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// 这个开关有没有写
    ///
    /// 与 [`Block::param`] 的区别：只写 `flag` 的参数值是空串，于是
    /// `param("flag")` 给出 `Some("")`，与「写了 `flag=`」分不开，而这两件事
    /// 在实践中并不总是同一个意思。
    pub fn has(&self, key: &str) -> bool {
        self.params.iter().any(|(name, _)| name == key)
    }
}

/// 注册表里的一条：一个名字 + 一个渲染器 + 内容怎么处理。
#[derive(Clone)]
pub struct BlockSpec {
    name: String,
    renderer: RendererKind,
    mode: ContentMode,
}

/// 渲染器的两种存法。
///
/// 分成两种是为了让**函数指针**那条路不必绕 HRTB：把 `fn(&Block, &Node, &mut dyn
/// Renderer)` 包进一个闭包时，闭包的参数生命周期会被按实体推断出来（而不是按
/// `BlockRenderer` 那个高阶签名），于是报 `implementation of FnOnce is not general
/// enough`。直接存函数指针就没这个问题 —— 顺带还少一次间接调用。
#[derive(Clone)]
enum RendererKind {
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
        }
    }

    /// 造一条注册项（渲染器是任意**闭包**）
    ///
    /// 写 `BlockSpec::plain` 就够的那些不必用这条 —— 函数指针那条少一次间接调用。
    pub fn new(
        name: impl Into<String>,
        mode: ContentMode,
        renderer: impl BlockRendererFn,
    ) -> Self {
        Self {
            name: name.into(),
            renderer: RendererKind::Boxed(Arc::new(renderer.into_boxed())),
            mode,
        }
    }

    /// 块名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 内容怎么处理
    pub fn mode(&self) -> ContentMode {
        self.mode
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
            .finish_non_exhaustive()
    }
}

/// [`BlockSpec::new`] 的渲染函数那一参。
///
/// 存在的理由只有一个：让 `new` 收**任何** `Fn(&Block, &Node, &mut dyn Renderer)`，
/// 而不必让调用方自己 `Box::new` 出一个 `Arc<BlockRenderer>` 再塞进来。
/// 实现是 blanket 的 —— 调用方永远不用直接碰它。
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

/// `::` 块的注册表。
///
/// 有序：诊断与命令行帮助按注册次序列出，加了什么一眼看得见。
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

    /// 灌进去一份内置的，再改（续）
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
        f.debug_struct("BlockRegistry").field("blocks", &self.names()).finish()
    }
}

// ---- 当前生效的那一份 --------------------------------------------------
//
// 扫描器与分发器是零大小的 markdown-it 规则，拿不到「自己属于哪个解析器」——
// 而一个进程里可以有好几个 [`Mdpp`](crate::engine::Mdpp)，各带各的注册表。
//
// markdown-it 给的 `ext` 集合放不下：它的键值是 `&'static dyn Any`，而这里的
// 生命周期由 `Mdpp` 的拥有者决定。于是走线程局部，理由与
// [`crate::context`] 那份一模一样（渲染是同步的，规则必然跑在同一个线程上）。

thread_local! {
    /// 现在这一趟渲染在用的注册表
    static CURRENT: RefCell<Option<Arc<BlockRegistry>>> = const { RefCell::new(None) };
}

/// 取当次渲染的注册表
pub fn current() -> Option<Arc<BlockRegistry>> {
    CURRENT.with(|cell| cell.borrow().clone())
}

/// 查一个块，**只**在当次渲染的注册表里查。扫描器与分发器用它。
pub fn lookup(name: &str) -> Option<BlockSpec> {
    current()?.get(name).cloned()
}

/// 装上注册表，返回用完还原的守卫。存的是**进入之前**的那一份（不是空），
/// 理由见 [`crate::context`] 模块头。
pub struct Guard {
    previous: Option<Arc<BlockRegistry>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|cell| *cell.borrow_mut() = previous);
    }
}

/// 装上 [`BlockRegistry`]，返回用完还原的守卫。
pub fn enter(registry: Arc<BlockRegistry>) -> Guard {
    Guard {
        previous: CURRENT.with(|cell| cell.borrow_mut().replace(registry)),
    }
}

/// 把一行切成 token：空白分隔，但**引号里的空白不算分隔**。
///
/// 单引号与双引号都可以；引号内用反斜杠保护引号本身与反斜杠，引号外反斜杠没有
/// 特殊含义（路径里到处都是反斜杠，不该在这里被吃掉）。
///
/// 引号没有配对就整行走不通：返回 `None`，这一行退回普通段落。
pub fn tokenize(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();

    while let Some(ch) = chars.next() {
        match quote {
            Some(closing) => {
                if ch == '\\' {
                    match chars.next() {
                        Some('\\') => current.push('\\'),
                        Some(next) if next == closing => current.push(closing),
                        // 只认这两个转义，其余原样保留（连反斜杠一起）
                        Some(next) => {
                            current.push('\\');
                            current.push(next);
                        }
                        None => return None,
                    }
                } else if ch == closing {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    // `""` 是一个空值的 token，不是没有 token
                    started = true;
                }
                ch if ch.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                ch => {
                    current.push(ch);
                    started = true;
                }
            },
        }
    }

    if quote.is_some() {
        return None;
    }
    if started {
        tokens.push(current);
    }
    Some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_it::Node;

    fn noop(_: &Block, _: &Node, _: &mut dyn Renderer) {}

    #[test]
    fn parsing_a_header_yields_name_and_params() {
        let (name, params) = Block::parse_header("::note key=value flag 另一个=值").unwrap();
        assert_eq!(name, "note");
        assert_eq!(
            params,
            vec![
                ("key".to_string(), "value".to_string()),
                ("flag".to_string(), String::new()),
                ("另一个".to_string(), "值".to_string()),
            ]
        );
    }

    #[test]
    fn quotes_protect_spaces_and_escapes() {
        let (name, params) =
            Block::parse_header(r#"::"两 个词" 标题="含 空格" 引号="他说\"好\"" flag"#).unwrap();
        assert_eq!(name, "两 个词");
        assert_eq!(
            params,
            vec![
                ("标题".to_string(), "含 空格".to_string()),
                ("引号".to_string(), "他说\"好\"".to_string()),
                ("flag".to_string(), String::new()),
            ]
        );
    }

    /// 认不出的头一律退回普通段落 —— 宁可当普通文字，也不要猜作者想要什么
    #[test]
    fn lines_that_are_not_headers_are_rejected() {
        for line in [
            "::",
            "::=x",
            "::a:b",
            ":name",
            "普通一行",
            "::note key=\"没关上",
        ] {
            assert!(Block::parse_header(line).is_none(), "{line:?} 不该被认成头");
        }
    }

    /// 引号外的反斜杠是**字面**的：路径里到处都是它，不该在这里被吃掉
    #[test]
    fn backslash_outside_quotes_is_literal() {
        let (_, params) = Block::parse_header(r"::note path=C:\笔记").unwrap();
        assert_eq!(params, vec![("path".to_string(), r"C:\笔记".to_string())]);
    }

    #[test]
    fn a_bare_flag_is_present_but_empty() {
        let block = Block {
            name: "x".into(),
            params: vec![("flag".into(), String::new())],
            body: String::new(),
        };
        assert!(block.has("flag"));
        assert_eq!(block.param("flag"), Some(""));
        assert!(!block.has("other"));
        assert_eq!(block.param("other"), None);
    }

    #[test]
    fn registering_the_same_name_twice_replaces_rather_than_duplicates() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("a", ContentMode::Markdown, noop);
        registry.register_fn("b", ContentMode::Raw, noop);
        registry.register_fn("a", ContentMode::Sections, noop);

        assert_eq!(registry.len(), 2, "{:?}", registry.names());
        assert_eq!(registry.get("a").map(BlockSpec::mode), Some(ContentMode::Sections));
    }

    #[test]
    fn a_block_can_be_taken_out_and_put_back_under_another_name() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("a", ContentMode::Markdown, noop);
        let taken = registry.remove("a").unwrap();
        assert!(!registry.contains("a"));

        registry.register(taken.with_name("c"));
        assert!(registry.contains("c"));
        assert!(!registry.contains("a"));
    }

    #[test]
    fn registration_order_is_kept_for_listing() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("z", ContentMode::Raw, noop);
        registry.register_fn("a", ContentMode::Raw, noop);
        registry.register_fn("m", ContentMode::Raw, noop);
        assert_eq!(registry.names(), vec!["z", "a", "m"]);
    }
}