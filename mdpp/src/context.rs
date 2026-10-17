//! 渲染上下文：**每次渲染各自一份**的那几样东西。
//!
//! `[[目标]]` 指到哪了、`::卡片` 的模板页正文是什么 —— 没有一样是解析器自己能知道的，
//! 它们都在解析器外面，所以必须**递进来**（见 [`RenderContext`]）。
//!
//! ## 为什么是线程局部而不是解析器的 `ext`
//!
//! markdown-it 的 `ext` 集合是**解析器全局唯一**的那一份，而渲染状态必须每次调用各
//! 自一份：两个线程同时渲染时，一个的「用完还原」会把另一个打断。而 `parse(&self, src)`
//! 与 `NodeValue::render` 都没有「顺带传个环境参数」的入口。
//!
//! 线程局部是那个入口的等价物：解析本身是同步的，规则必然跑在同一个线程上。
//!
//! ## 存旧值、用完还原
//!
//! [`enter`] 返回一个守卫，Drop 时还原**进入之前**的那一份，而不是「清空」。
//! 区别在嵌套渲染：`::html` 里的 `<markdown>` 会在渲染途中回调进
//! [`Mdpp::render_with`](crate::engine::Mdpp::render_with)，两次调用叠在一起 ——
//! 还原成空的话，内层一结束，外层剩下的正文就再也问不到链接解析器了。

use std::cell::{Cell, RefCell};
use std::fmt;
use std::sync::Arc;

use crate::block::template::TemplatePages;
use crate::link::LinkResolver;

/// 用户模板最多套这么多层（再多就当是转圈了）。超限时给的是一句说明而不是硬展开
/// —— 见 [`crate::block::expand`]。
pub const MAX_TEMPLATE_DEPTH: usize = 12;

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

thread_local! {
    /// 现在这一趟渲染的上下文。没有就是"不在渲染中"。
    static CURRENT: RefCell<Option<RenderContext>> = const { RefCell::new(None) };

    /// 用户模板展开到第几层（转圈的保护）
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// 取当次渲染的上下文副本。渲染规则在回调里用这个问「现在用的是哪个解析器」。
pub fn current() -> Option<RenderContext> {
    CURRENT.with(|cell| cell.borrow().clone())
}

/// 当次渲染的链接解析器（渲染规则里的高频入口，少一层包装）
pub fn links() -> Option<Arc<dyn LinkResolver>> {
    CURRENT.with(|cell| cell.borrow().as_ref().and_then(|ctx| ctx.links.clone()))
}

/// 当次渲染的模板页来源
pub fn templates() -> Option<TemplatePages> {
    CURRENT.with(|cell| cell.borrow().as_ref().and_then(|ctx| ctx.templates.clone()))
}

/// 现在展开到第几层
pub fn depth() -> usize {
    DEPTH.with(|cell| cell.get())
}

/// 深一层地做一件事（用完还原）。用户模板可以嵌用户模板 —— 一个转圈的模板（甲嵌乙、
/// 乙嵌甲）会一直展开下去，所以每展开一层记一笔，超过 [`MAX_TEMPLATE_DEPTH`] 就不展开。
pub fn deeper<T>(run: impl FnOnce() -> T) -> T {
    let previous = DEPTH.with(|cell| cell.replace(depth() + 1));
    let out = run();
    DEPTH.with(|cell| cell.set(previous));
    out
}

/// 把上下文装上，返回一个用完自动还原的守卫。存的是**进入之前**的那一份，不是空。
pub struct Guard {
    previous: Option<RenderContext>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|cell| *cell.borrow_mut() = previous);
    }
}

/// 装上 [`RenderContext`]，返回用完还原的守卫。见 [`Guard`]。
pub fn enter(context: RenderContext) -> Guard {
    Guard {
        previous: CURRENT.with(|cell| cell.borrow_mut().replace(context)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 守卫装上就有、掉下来就回到没有 —— 渲染之外不许残留上一趟的上下文
    #[test]
    fn a_guard_puts_the_context_in_place_and_takes_it_away_again() {
        assert!(current().is_none(), "渲染之外没有上下文");

        let resolver: Arc<dyn LinkResolver> = Arc::new(crate::link::tests::AnyTarget);
        {
            let _guard = enter(RenderContext::new().with_links(resolver.clone()));
            assert!(links().is_some());
        }
        assert!(current().is_none(), "守卫掉下来之后要回到没有");
    }

    /// 嵌套渲染：里层结束后，**外层还在**，而且还是外层那一份。
    ///
    /// 这是守卫「存旧值」而非「清空」的全部理由。守卫若清空，内层一返回，
    /// 外层剩下的模板就再也问不到链接解析器 —— 而且只在**真的有嵌套**时犯。
    #[test]
    fn nesting_restores_the_outer_context_not_none() {
        let outer: Arc<dyn LinkResolver> = Arc::new(crate::link::tests::AnyTarget);
        {
            let _outer_guard = enter(RenderContext::new().with_links(outer.clone()));

            let inner: Arc<dyn LinkResolver> = Arc::new(crate::link::tests::NoneAtAll);
            {
                let _inner_guard = enter(RenderContext::new().with_links(inner));
                assert!(links().is_some());
            }

            // 里层的解析器是 None，外层那个还在
            let back = links().expect("外层的上下文该还在");
            assert!(std::ptr::eq(
                back.as_ref() as *const dyn LinkResolver,
                Arc::as_ptr(&outer)
            ));
        }
        assert!(current().is_none());
    }

    /// 层数进得去也退得回来 —— 少了还原那一步，后面每一次展开都白扣一层
    #[test]
    fn deeper_counts_and_comes_back() {
        assert_eq!(depth(), 0);
        deeper(|| {
            assert_eq!(depth(), 1);
            deeper(|| assert_eq!(depth(), 2));
            assert_eq!(depth(), 1);
        });
        assert_eq!(depth(), 0);
    }

    /// 复制出来的是同一份链接解析器，不是另起一份 —— 里面只有 `Arc`，克隆很便宜
    #[test]
    fn the_context_is_cheap_to_clone() {
        let resolver: Arc<dyn LinkResolver> = Arc::new(crate::link::tests::AnyTarget);
        let context = RenderContext::new().with_links(resolver);
        let copy = context.clone();
        assert!(std::ptr::eq(
            copy.links().unwrap().as_ref() as *const dyn LinkResolver,
            context.links().unwrap().as_ref() as *const dyn LinkResolver
        ));
    }
}
