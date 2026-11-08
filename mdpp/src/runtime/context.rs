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

use std::sync::Arc;

use crate::block::template::{TemplatePages, TemplateVars};
use crate::link::LinkResolver;

pub use super::render_context::RenderContext;

/// 用户模板的**默认**嵌套上限。超限时给的是一句说明而不是硬展开 —— 见
/// [`crate::block::expand`]。调用方可以通过
/// [`RenderContext::with_max_template_depth`](crate::RenderContext::with_max_template_depth)
/// 改；命令行与站点配置也各有一个入口。
///
/// 64 而不是更小：嵌套 navbox 这类「同名块里再放同名块」是正当用法，不能把递归
/// 本身当错误。
pub const MAX_TEMPLATE_DEPTH: usize = super::render_context::DEFAULT_MAX_TEMPLATE_DEPTH;

/// 取当次渲染的上下文副本。渲染规则在回调里用这个问「现在用的是哪个解析器」。
pub fn current() -> Option<RenderContext> {
    crate::env::with(|env| env.context.clone())
}

/// 当次渲染的链接解析器（渲染规则里的高频入口，少一层包装）
pub fn links() -> Option<Arc<dyn LinkResolver>> {
    crate::env::with(|env| env.context.links().cloned()).flatten()
}

/// 当次渲染的模板页来源
pub fn templates() -> Option<TemplatePages> {
    crate::env::with(|env| env.context.templates().cloned()).flatten()
}

/// 当前模板作用域（最内层）。
pub(crate) fn template_vars() -> Option<Arc<TemplateVars>> {
    crate::env::with(|env| env.template_vars.clone()).flatten()
}

/// 取一个模板变量。存在但为空也返回 `Some("")` —— `condition-defined` 靠这个
/// 与「变量根本不存在」区分开。
pub(crate) fn template_var(name: &str) -> Option<String> {
    crate::env::with(|env| {
        env.template_vars
            .as_ref()
            .and_then(|vars| vars.lookup(name))
            .map(str::to_string)
    })
    .flatten()
}

/// 进入一层模板作用域；出栈时还原外层作用域。
pub(crate) struct TemplateVarsGuard {
    previous: Option<Arc<TemplateVars>>,
    had_state: bool,
}

impl Drop for TemplateVarsGuard {
    fn drop(&mut self) {
        if self.had_state {
            let previous = self.previous.take();
            crate::env::with(|env| env.template_vars = previous);
        } else {
            crate::env::set(None);
        }
    }
}

/// 装上模板作用域，返回用完自动还原的守卫。
pub(crate) fn enter_template_vars(vars: Arc<TemplateVars>) -> TemplateVarsGuard {
    let mut pending = Some(vars);
    let mut previous = None;
    let had_state = crate::env::with(|env| {
        previous = env.template_vars.replace(pending.take().expect("只取一次"));
    })
    .is_some();
    if !had_state {
        crate::env::set(Some(crate::env::RenderEnv {
            template_vars: pending,
            ..Default::default()
        }));
    }
    TemplateVarsGuard {
        previous,
        had_state,
    }
}

/// 当次渲染的模板嵌套上限（没有上下文时用默认值）。
pub(crate) fn max_template_depth() -> usize {
    crate::env::with(|env| env.context.max_template_depth()).unwrap_or(MAX_TEMPLATE_DEPTH)
}

/// 现在展开到第几层
pub fn depth() -> usize {
    crate::env::with(|env| env.depth).unwrap_or(0)
}

/// 深一层地做一件事（用完还原）。用户模板可以嵌用户模板 —— 一个转圈的模板（甲嵌乙、
/// 乙嵌甲）会一直展开下去，所以每展开一层记一笔；超过当次
/// [`RenderContext`](crate::RenderContext) 给的上限就不展开。
pub fn deeper<T>(run: impl FnOnce() -> T) -> T {
    let existed = crate::env::with(|env| env.depth += 1).is_some();
    if !existed {
        // 渲染之外单独调用（低层用法/测试）：建一份临时环境，出去时撤掉
        crate::env::set(Some(crate::env::RenderEnv {
            depth: 1,
            ..Default::default()
        }));
    }
    let out = run();
    if existed {
        crate::env::with(|env| env.depth = env.depth.saturating_sub(1));
    } else {
        crate::env::set(None);
    }
    out
}

/// 把上下文装上，返回一个用完自动还原的守卫。存的是**进入之前**的那一份，不是空。
pub struct Guard {
    previous: Option<RenderContext>,
    /// 进入之前根本没有渲染状态：这个守卫要负责把整份状态撤掉
    had_state: bool,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.had_state {
            let previous = self.previous.take().unwrap_or_default();
            crate::env::with(|env| env.context = previous);
        } else {
            crate::env::set(None);
        }
    }
}

/// 装上 [`RenderContext`]，返回用完还原的守卫。见 [`Guard`]。
pub fn enter(context: RenderContext) -> Guard {
    let mut pending = Some(context);
    let mut previous = None;
    let had_state = crate::env::with(|env| {
        previous = Some(std::mem::replace(
            &mut env.context,
            pending.take().expect("只取一次"),
        ));
    })
    .is_some();
    if !had_state {
        // 渲染之外单独装上下文（低层用法/测试）：建一份只有 context 的环境
        crate::env::set(Some(crate::env::RenderEnv {
            context: pending.expect("只取一次"),
            ..Default::default()
        }));
    }
    Guard {
        previous,
        had_state,
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
