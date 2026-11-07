//! 渲染上下文的纯类型：链接解析器与模板页来源。
//!
//! 这里不读线程局部；当前上下文的取用函数在 [`crate::context`]。

use std::fmt;
use std::sync::Arc;

use crate::block::template::TemplatePages;
use crate::link::LinkResolver;

/// 一次渲染里、解析器外面那几样东西。
///
/// 克隆是廉价的：里面只有两个 [`Arc`]。
#[derive(Clone, Default)]
pub struct RenderContext {
    links: Option<Arc<dyn LinkResolver>>,
    templates: Option<TemplatePages>,
}

impl fmt::Debug for RenderContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderContext")
            .field("links", &self.links.is_some())
            .field("templates", &self.templates.is_some())
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

    /// 当次渲染的链接解析器（没有就是没有 —— 不猜）
    pub fn links(&self) -> Option<&Arc<dyn LinkResolver>> {
        self.links.as_ref()
    }

    /// 当次渲染的模板页来源
    pub fn templates(&self) -> Option<&TemplatePages> {
        self.templates.as_ref()
    }
}
