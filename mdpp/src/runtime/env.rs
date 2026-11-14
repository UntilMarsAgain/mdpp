//! 一次渲染的运行时环境：上下文、注册表、资源收集器、变体作用域都装在这里。
//!
//! 之前这些字段散在几个线程局部里，读一个块渲染器时很难看出「现在到底有哪些
//! 东西可用」。现在只有一份 [`RenderEnv`]：进入渲染时装上，退出时还原；嵌套渲染
//! 退出时把内层资源合并回外层。
//!
//! 线程局部仍然是 markdown-it 回调的入口：`NodeValue::render(&self, node, fmt)`
//! 没有「用户数据」参数，规则只能从这里取当前环境。但**只有本模块直接碰线程局部**，
//! 其它模块通过 [`with`] 读一个明确的 [`RenderEnv`]，不再各自维护字段。
//!
//! ## 依赖方向
//!
//! `env` 只依赖**纯类型**模块：`render_context`、`registry::table`、`asset`。
//! `context` 的取当前值函数、`registry::scope`、`assets` 的登记函数反过来依赖
//! `env`，但 `env` 不再回头依赖它们，第二个模块环已经消掉。
//!
//! 剩下的 `engine ↔ block` 环是 `::` 语法本身需要的：解析器要装块规则，块里的
//! 内容又要回到渲染器再解析。它是行为/递归环，属于有意设计。

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use super::render_context::RenderContext;
use crate::block::registry::table::BlockRegistry;
use crate::block::template::TemplateVars;
use crate::engine::{Features, Problems};
use crate::runtime::asset::Sink;

/// 一次问题框的简要说明。`::guard` 失败时用它重画一个不丢掉原因的框。
#[derive(Debug, Clone)]
pub(crate) struct ProblemDetail {
    pub(crate) badge: String,
    pub(crate) class: String,
    pub(crate) name: String,
    pub(crate) why: String,
}

/// guard 需要的环境快照：问题计数、首个问题的说明、资源。
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    problems: Problems,
    diagnostics: usize,
    detail: Option<ProblemDetail>,
    assets: Sink,
    styles: Vec<String>,
    scripts: Vec<String>,
    features: Features,
}

/// 「在渲染途中再渲染一段 markdown」的能力。
pub(crate) type Nested = Box<dyn Fn(&str) -> String>;

/// 当前变体组作用域（内部）。公开的组/选项在 [`crate::variants`]。
#[derive(Debug, Clone)]
pub(crate) struct VariantScope {
    pub(crate) group: String,
    pub(crate) value: String,
}

/// 新建一份变体组 ID 计数器。
pub(crate) fn sequence() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

/// 一趟渲染的全部跨切面状态。
pub(crate) struct RenderEnv {
    /// 链接解析器与模板页来源
    pub(crate) context: RenderContext,
    /// 当前注册表
    pub(crate) registry: Arc<BlockRegistry>,
    /// 资源收集器（单页/站点两种模式）
    pub(crate) assets: Sink,
    /// 这一页全局收集的 CSS，按首次登记顺序去重
    pub(crate) styles: Vec<String>,
    /// 这一页全局收集的脚本标签，按首次登记顺序
    pub(crate) scripts: Vec<String>,
    /// 嵌套渲染入口；只有真正的渲染装得上
    pub(crate) nested: Option<Nested>,
    /// 当前变体组作用域
    pub(crate) variant: Option<VariantScope>,
    /// 页内变体组 ID 计数器；嵌套渲染共享同一份
    pub(crate) variant_seq: Arc<AtomicUsize>,
    /// 模板展开层数
    pub(crate) depth: usize,
    /// 当前模板作用域（`{{@名字}}` 的来源）；嵌套模板通过 `parent` 串起来
    pub(crate) template_vars: Option<Arc<TemplateVars>>,
    /// 在渲染途中再调 `render_nested` 的层数。它与 `depth` 分开：
    /// `depth` 数模板展开，`render_depth` 数 `::demo` / `<markdown>` 这类渲染期嵌套。
    pub(crate) render_depth: usize,
    /// 这一趟实际用到的前端资源（代码高亮 / 数学 / mermaid）。
    pub(crate) features: Features,
    /// 这一趟渲染出来的问题框计数。
    pub(crate) problems: Problems,
    /// 这一趟出现过多少个问题框。`problems` 只分「未知/误用」两类，
    /// `diagnostics` 还包含模板页读不出来等诊断；guard 只看后者。
    pub(crate) diagnostics: usize,
    /// 第一个问题框的说明。guard 失败时用它保留具体原因。
    pub(crate) problem_detail: Option<ProblemDetail>,
}

impl Default for RenderEnv {
    fn default() -> Self {
        RenderEnv {
            context: RenderContext::default(),
            registry: Arc::new(BlockRegistry::default()),
            assets: Sink::default(),
            styles: Vec::new(),
            scripts: Vec::new(),
            nested: None,
            variant: None,
            variant_seq: sequence(),
            depth: 0,
            template_vars: None,
            render_depth: 0,
            features: Features::default(),
            problems: Problems::default(),
            diagnostics: 0,
            problem_detail: None,
        }
    }
}

/// 记一个「没注册的块」问题框。
pub(crate) fn unknown_problem() {
    let _ = with(|env| env.problems.unknown += 1);
}

/// 记一个「块用法不对」问题框。
pub(crate) fn misused_problem() {
    let _ = with(|env| env.problems.misused += 1);
}

/// 记一个实际写出的问题框，并留下第一份说明。
///
/// `diagnostics` 与 `problems` 分开：模板页读不出来也写问题框，但它不归入
/// 「未知/误用」两项；guard 需要把这类诊断也算失败。
pub(crate) fn note_problem(badge: &str, class: &str, name: &str, why: &str) {
    let _ = with(|env| {
        env.diagnostics += 1;
        if env.problem_detail.is_none() {
            env.problem_detail = Some(ProblemDetail {
                badge: badge.to_string(),
                class: class.to_string(),
                name: name.to_string(),
                why: why.to_string(),
            });
        }
    });
}

/// 登记一份整页 CSS。内容相同只留第一份，顺序就是首次出现的顺序。
pub(crate) fn register_style(content: String) {
    if content.trim().is_empty() {
        return;
    }
    let _ = with(|env| {
        if !env.styles.contains(&content) {
            env.styles.push(content);
        }
    });
}

/// 取走这一趟收集到的整页 CSS；只有最外层渲染调用。
pub(crate) fn take_styles() -> Vec<String> {
    with(|env| std::mem::take(&mut env.styles)).unwrap_or_default()
}

/// 登记一段要放到页面末尾的脚本标签。内容相同只留第一份。
pub(crate) fn register_script(content: String) -> bool {
    if content.trim().is_empty() {
        return false;
    }
    with(|env| {
        if !env.scripts.contains(&content) {
            env.scripts.push(content);
        }
        true
    })
    .unwrap_or(false)
}

/// 取走这一趟收集到的脚本标签；只有最外层渲染调用。
pub(crate) fn take_scripts() -> Vec<String> {
    with(|env| std::mem::take(&mut env.scripts)).unwrap_or_default()
}

/// 当前渲染环境的状态快照；不在渲染中时返回 `None`。
pub(crate) fn snapshot() -> Option<Snapshot> {
    with(|env| Snapshot {
        problems: env.problems,
        diagnostics: env.diagnostics,
        detail: env.problem_detail.clone(),
        assets: env.assets.clone(),
        styles: env.styles.clone(),
        scripts: env.scripts.clone(),
        features: env.features,
    })
}

/// 恢复快照。guard 用它丢弃失败分支写过的问题计数与资源。
pub(crate) fn restore(snapshot: Snapshot) {
    let _ = with(|env| {
        env.problems = snapshot.problems;
        env.diagnostics = snapshot.diagnostics;
        env.problem_detail = snapshot.detail;
        env.assets = snapshot.assets;
        env.styles = snapshot.styles;
        env.scripts = snapshot.scripts;
        env.features = snapshot.features;
    });
}

/// 从快照之后新增了多少个问题框。
pub(crate) fn problems_since(snapshot: &Snapshot) -> usize {
    with(|env| env.diagnostics.saturating_sub(snapshot.diagnostics)).unwrap_or(0)
}

/// 当前第一份问题说明。
pub(crate) fn problem_detail() -> Option<ProblemDetail> {
    with(|env| env.problem_detail.clone()).flatten()
}

/// 清空当前问题说明，供 guard 捕获自己那段内容的第一份诊断。
pub(crate) fn clear_problem_detail() {
    let _ = with(|env| env.problem_detail = None);
}

/// 恢复之前的问题说明。
pub(crate) fn set_problem_detail(detail: Option<ProblemDetail>) {
    let _ = with(|env| env.problem_detail = detail);
}

thread_local! {
    static CURRENT: RefCell<Option<RenderEnv>> = const { RefCell::new(None) };
}

/// 在**当前**环境上做一件事。不在渲染中就返回 `None`。
pub(crate) fn with<R>(f: impl FnOnce(&mut RenderEnv) -> R) -> Option<R> {
    CURRENT.with(|cell| cell.borrow_mut().as_mut().map(f))
}

/// 取走当前环境（嵌套渲染要先把闭包拿出来，避免在 `with` 的借用里再进渲染）
fn take() -> Option<RenderEnv> {
    CURRENT.with(|cell| cell.borrow_mut().take())
}

/// 直接放回一份环境（[`take`] 取走之后用）
pub(crate) fn set(env: Option<RenderEnv>) {
    CURRENT.with(|cell| *cell.borrow_mut() = env);
}

/// 进入一趟渲染。返回的守卫出栈时：
///
/// 1. 把内层收集到的资源**合并回外层**（嵌套渲染不让资源消失）；
/// 2. 还原进入之前的那一份（可能没有）。
pub(crate) struct Guard {
    previous: Option<RenderEnv>,
}

impl Guard {
    /// 这一趟是不是嵌套渲染
    pub(crate) fn nested(&self) -> bool {
        self.previous.is_some()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let inner = take();
        if let Some(mut previous) = self.previous.take() {
            if let Some(inner) = inner {
                for asset in inner.assets.assets {
                    if previous.assets.hashes.insert(asset.hash.clone()) {
                        previous.assets.assets.push(asset);
                    }
                }
                for style in inner.styles {
                    if !previous.styles.contains(&style) {
                        previous.styles.push(style);
                    }
                }
                for script in inner.scripts {
                    if !previous.scripts.contains(&script) {
                        previous.scripts.push(script);
                    }
                }
                previous.features.merge(inner.features);
                previous.problems.merge(inner.problems);
                previous.diagnostics += inner.diagnostics;
                if previous.problem_detail.is_none() {
                    previous.problem_detail = inner.problem_detail;
                }
            }
            set(Some(previous));
        }
    }
}

/// 装上这一趟的环境，返回守卫。`nested` 由守卫回答。
pub(crate) fn enter(env: RenderEnv) -> Guard {
    let previous = take();
    set(Some(env));
    Guard { previous }
}
