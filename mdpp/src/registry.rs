//! `::` 块的注册表：外部在这里挂块，内部在这里派发。
//!
//! [`BlockRegistry`] 是普通数据 —— 不含解析器，也不含全局状态。想要一份自定义的，
//! 构造一份交给 [`MdppBuilder::registry`](crate::engine::MdppBuilder::registry)，
//! 两个 [`Mdpp`](crate::engine::Mdpp) 各带各的，互不相干。
//!
//! 注册一块要交代两件事：渲染函数，以及**块内容怎么处理**（[`ContentMode`]）。
//! 后者不是可选项 —— 扫描器要靠它在内容被解析**之前**决定走哪条路，那时渲染函数
//! 还没被调用过。
//!
//! [`BlockRegistry::register`] 对同名块**覆盖**，这是改掉一个内置块最短的路。

use std::cell::RefCell;
use std::sync::Arc;

use markdown_it::{Node, Renderer};

/// 把头里的 token 收成 `key=value` 参数。
///
/// 两种写法都认：
///
/// - `key=value`（等号贴着）—— 按**第一个**切：`key="a=b"` 的值就是 `a=b`；
/// - `key = value`（等号两侧留白）—— 两边的空白本来就被 [`tokenize`] 吃掉了，
///   所以这里只需要把落单的 `=` 与它前后两个 token 拼回去。
///
/// 第二种是**必须**认的，不是顺手加的：`key = value` 会被切成 `key` / `=` / `value`
/// 三个 token，而不带 `=` 的 token 算「开关式参数」，于是 `key` 静悄悄变成一个空值、
/// 值整个丢掉 —— 写的人看到的是「参数没生效」，查不到是哪个字被吃掉了。
fn pairs(tokens: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        // `key = value`：等号是**下一个** token
        if tokens.get(index + 1).is_some_and(|next| next == "=") {
            // 再往后没有 token 时给空串：`key =` 是空值，不是「参数没给」
            let value = tokens.get(index + 2).cloned().unwrap_or_default();
            out.push((token.clone(), value));
            index += 3;
            continue;
        }
        // 落单的 `=`：没有键可配，它自己就是个开关式参数
        if token == "=" {
            out.push((token.clone(), String::new()));
            index += 1;
            continue;
        }
        match token.split_once('=') {
            Some((key, value)) => out.push((key.to_string(), value.to_string())),
            None => out.push((token.clone(), String::new())),
        }
        index += 1;
    }
    out
}

/// 写出一个 `::` 块：块的头、已解析好的子节点（形状由 [`ContentMode`] 决定）、渲染器。
///
/// 三个生命周期显式写成高阶的，不能省成 `Fn(&Block, &Node, &mut dyn Renderer)`：
/// 省略写法在函数指针的位置上会被定死成某一个具体生命周期，于是接函数指针的那种写法
/// 编译不过。
pub type BlockRenderer =
    dyn for<'a, 'b, 'c> Fn(&'a Block, &'b Node, &'c mut (dyn Renderer + 'c)) + Send + Sync;

/// 块内容怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentMode {
    /// 交给整个解析器再解析一遍，子节点挂在这个块下。
    #[default]
    Markdown,
    /// 按 `[标签]` 行切成几节，**各节各解析一遍**，每节一个子节点。用于「一节是一篇
    /// 独立的小文档」的块（选项卡、主题面板）。
    Sections,
    /// **不解析**，原文交给渲染器。
    ///
    /// 理由是正确性而非性能：TeX、图表定义、样式里的记号各有各的语法，按 markdown
    /// 走一遍会被改写（`$x$`、`*`、`_`、行首的 `#` 全是 markdown 的记号）。
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
    /// 名字必须紧跟 `::`，且不能含 `=` / `:`（那两个字符属于参数与「目标前缀」）。
    /// 认出一个坏块比退回普通段落更糟。
    pub fn parse_header(line: &str) -> Option<(String, Vec<(String, String)>)> {
        let rest = line.trim_start().strip_prefix("::")?;
        let tokens = tokenize(rest)?;
        let name = tokens.first()?;
        if name.is_empty() || name.contains('=') || name.contains(':') {
            return None;
        }
        let params = pairs(&tokens[1..]);
        Some((name.clone(), params))
    }

    /// 取一个参数
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// 这个开关有没有写。与 [`Block::param`] 的区别：只写 `flag` 的参数值是空串，
    /// `param("flag")` 给出 `Some("")`，与「写了 `flag=`」分不开。
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

/// 渲染器的两种存法。函数指针那条路不必绕 HRTB：把 `fn(&Block, &Node, &mut dyn
/// Renderer)` 包进闭包时，闭包的参数生命周期会按实体推断，于是报
/// `implementation of FnOnce is not general enough`。
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
    pub fn new(name: impl Into<String>, mode: ContentMode, renderer: impl BlockRendererFn) -> Self {
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

    /// 头一行切成名字加参数的那一步 —— 派发、渲染、诊断读的都是这一份，切错了没人看得出来
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

    /// 引号里的空白不算分隔、`\"` 出来是一个引号 —— 少了这条，带空格的值会被劈成几段参数
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

    /// 等号两侧留白也认 —— `key = value` 与 `key=value` 是同一个意思。
    ///
    /// 这条是**必须**的：`tokenize` 按空白切，而不带 `=` 的 token 算「开关式参数」，
    /// 所以修复之前 `title = 提醒` 会变成一个空的 `title` 加一个叫「提醒」的开关，
    /// 而值整个丢掉 —— 写的人只看到「参数没生效」，查不出是哪个字被吃掉了。
    #[test]
    fn spaces_around_the_equals_are_fine() {
        let (name, params) = Block::parse_header("::note title = 提醒 color = #0af").unwrap();
        assert_eq!(name, "note");
        assert_eq!(
            params,
            vec![
                ("title".to_string(), "提醒".to_string()),
                ("color".to_string(), "#0af".to_string())
            ]
        );

        // 两种写法混着写也一样
        let (_, mixed) = Block::parse_header("::note a = 1 b=2 c = 3").unwrap();
        assert_eq!(
            mixed,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
                ("c".to_string(), "3".to_string())
            ]
        );
    }

    /// 引号里的空格仍然由引号保护 —— 等号两侧留白这条不该把它带跑
    #[test]
    fn spaces_around_the_equals_keep_quoted_spaces_safe() {
        let (_, params) = Block::parse_header(r#"::note 标题 = "含 空格 的 标题""#).unwrap();
        assert_eq!(
            params,
            vec![("标题".to_string(), "含 空格 的 标题".to_string())]
        );
    }

    /// `key =` 后面空了是**空值**，不是「参数没给」—— 两种写法一致
    #[test]
    fn a_trailing_equals_is_an_empty_value() {
        let (_, with_space) = Block::parse_header("::note title =").unwrap();
        let (_, without) = Block::parse_header("::note title=").unwrap();
        assert_eq!(with_space, without);
        assert_eq!(with_space, vec![("title".to_string(), String::new())]);
    }

    /// 落单的 `=` 不该把后一个参数吃掉：它是开关，不是分隔符
    #[test]
    fn a_lone_equals_does_not_swallow_the_next_parameter() {
        let (_, params) = Block::parse_header("::note = flag title=提醒").unwrap();
        assert_eq!(
            params,
            vec![
                ("=".to_string(), String::new()),
                ("flag".to_string(), String::new()),
                ("title".to_string(), "提醒".to_string())
            ]
        );
    }

    /// 开关式参数照旧认得：写成 `key = flag` 时，`flag` 是**值**
    #[test]
    fn an_equals_with_a_space_still_takes_the_next_token_as_its_value() {
        let (_, params) = Block::parse_header("::note title = flag").unwrap();
        assert_eq!(params, vec![("title".to_string(), "flag".to_string())]);
    }

    /// 值里带 `=` 的按**第一个**切，与留白那条是同一个规则的两面
    #[test]
    fn only_the_first_equals_splits() {
        let (_, params) = Block::parse_header(r#"::note 标题 = "a=b=c""#).unwrap();
        assert_eq!(params, vec![("标题".to_string(), "a=b=c".to_string())]);
    }

    /// 开关认得出来：`has` 说有、`param` 给空串 —— 与「没给这个参数」的 `None` 分得开
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

    /// 同名再灌一次是覆盖，不是追加 —— 换了实现就该只剩新的那一份，表里不留两条同名的
    #[test]
    fn registering_the_same_name_twice_replaces_rather_than_duplicates() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("a", ContentMode::Markdown, noop);
        registry.register_fn("b", ContentMode::Raw, noop);
        registry.register_fn("a", ContentMode::Sections, noop);

        assert_eq!(registry.len(), 2, "{:?}", registry.names());
        assert_eq!(
            registry.get("a").map(BlockSpec::mode),
            Some(ContentMode::Sections)
        );
    }

    /// 摘掉之后旧名字就查不到了，换名装回去也只有新名字 —— 一条注册项只认一个名字
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

    /// 名单按注册次序给，不排序 —— 诊断与帮助里「先加的先出现」，加了什么一眼看得见
    #[test]
    fn registration_order_is_kept_for_listing() {
        let mut registry = BlockRegistry::new();
        registry.register_fn("z", ContentMode::Raw, noop);
        registry.register_fn("a", ContentMode::Raw, noop);
        registry.register_fn("m", ContentMode::Raw, noop);
        assert_eq!(registry.names(), vec!["z", "a", "m"]);
    }
}
