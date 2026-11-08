//! 渲染上下文的纯类型：链接解析器、模板页来源与模板嵌套上限。
//!
//! 这里不读线程局部；当前上下文的取用函数在 [`crate::context`]。

use std::fmt;
use std::sync::Arc;

use crate::block::template::{TemplatePage, TemplatePages, TemplateSource};
use crate::link::LinkResolver;

/// 用户模板默认最多嵌套多少层。
///
/// 调用方可以用 [`RenderContext::with_max_template_depth`] 改；命令行对应
/// `render --max-template-depth`，站点模式对应 `mdpp.toml` 的
/// `max_template_depth`。
pub const DEFAULT_MAX_TEMPLATE_DEPTH: usize = 64;

/// 调用方模板优先、标准库兜底。
#[derive(Debug)]
struct LayeredTemplates {
    primary: TemplatePages,
    fallback: TemplatePages,
}

impl TemplateSource for LayeredTemplates {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        self.primary.page(name).or_else(|| self.fallback.page(name))
    }
}

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
            templates: Some(crate::stdlib::pages()),
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
    /// 默认上下文：内部链接落成红链，标准库模板可用；标准库之外的名字按未知块处理。
    pub fn new() -> Self {
        Self::default()
    }

    /// 递进一个内部链接解析器。
    pub fn with_links(mut self, links: Arc<dyn LinkResolver>) -> Self {
        self.links = Some(links);
        self
    }

    /// 在现有来源之上叠加一份模板页来源；**调用方优先**，已有的标准库是兜底。
    ///
    /// 多次调用会一层层叠上去，后叠的优先。要完全关掉标准库用
    /// [`Self::without_templates`]。
    pub fn with_templates(mut self, templates: TemplatePages) -> Self {
        self.templates = Some(match self.templates.take() {
            Some(fallback) => Arc::new(LayeredTemplates {
                primary: templates,
                fallback,
            }),
            None => templates,
        });
        self
    }

    /// 不要任何模板来源：非内置的 `::名字` 全部按未知块处理。
    pub fn without_templates(mut self) -> Self {
        self.templates = None;
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
