//! 渲染上下文的纯类型：链接解析器、模板页来源与模板嵌套上限。
//!
//! 这里不读线程局部；当前上下文的取用函数在 [`crate::context`]。

use std::fmt;
use std::sync::Arc;

use crate::block::template::TemplatePages;
use crate::link::LinkResolver;

/// 用户模板默认最多嵌套多少层。
///
/// 调用方可以用 [`RenderContext::with_max_template_depth`] 改；命令行对应
/// `render --max-template-depth`，站点模式对应 `mdpp.toml` 的
/// `max_template_depth`。
pub const DEFAULT_MAX_TEMPLATE_DEPTH: usize = 64;

/// 一次渲染里、解析器外面那几样东西。
///
/// 克隆是廉价的：里面只有两个 [`Arc`]，以及一个层数上限。
#[derive(Clone)]
pub struct RenderContext {
    links: Option<Arc<dyn LinkResolver>>,
    templates: Option<TemplatePages>,
    max_template_depth: usize,
}

impl Default for RenderContext {
    fn default() -> Self {
        RenderContext {
            links: None,
            templates: None,
            max_template_depth: DEFAULT_MAX_TEMPLATE_DEPTH,
        }
    }
}

impl fmt::Debug for RenderContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderContext")
            .field("links", &self.links.is_some())
            .field("templates", &self.templates.is_some())
            .field("max_template_depth", &self.max_template_depth)
            .finish()
    }
}

impl RenderContext {
    /// 空上下文：所有内部链接都落成红链，所有非内置的 `::名字` 都按未知块处理。
    pub fn new() -> Self {
        Self::default()
    }

    /// 递进一个内部链接解析器。
    pub fn with_links(mut self, links: Arc<dyn LinkResolver>) -> Self {
        self.links = Some(links);
        self
    }

    /// 递进一份模板页来源（非内置的 `::名字` 去那里找同名页）。
    pub fn with_templates(mut self, templates: TemplatePages) -> Self {
        self.templates = Some(templates);
        self
    }

    /// 改用户模板的嵌套上限。默认是 [`MAX_TEMPLATE_DEPTH`](crate::context::MAX_TEMPLATE_DEPTH)
    /// （64）。
    ///
    /// 这是「最多套多少层」而不是「允不允许递归」：嵌套 navbox 这类正当递归
    /// 也可以用满这个数。写 `0` 会让任何模板展开都立刻停止。
    pub fn with_max_template_depth(mut self, depth: usize) -> Self {
        self.max_template_depth = depth;
        self
    }

    /// 当次渲染的链接解析器（没有就是没有 —— 不猜）
    pub fn links(&self) -> Option<&Arc<dyn LinkResolver>> {
        self.links.as_ref()
    }

    /// 当次渲染的模板页来源
    pub fn templates(&self) -> Option<&TemplatePages> {
        self.templates.as_ref()
    }

    /// 当次渲染的用户模板嵌套上限
    pub fn max_template_depth(&self) -> usize {
        self.max_template_depth
    }
}
