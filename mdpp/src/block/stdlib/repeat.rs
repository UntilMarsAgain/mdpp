//! `::each` 与 `::rows` 共用的重复渲染循环。
//!
//! 两种块的**文档端语法不同**：`each` 按分节扫描数据，`rows` 按 `|` 拆单元格；
//! 这里刻意不碰那一步。两边切好、选好模板之后都归一成 [`Piece`]，循环体只负责
//! 建变量作用域、做晚绑定替换，最后把整段拼接结果交给嵌套渲染。
//!
//! 拼接后一次渲染（而不是逐段渲染）是语法的一部分：相邻片段之间可以续写列表、
//! 表格与引用，模板本身不需要知道「这是第几节」。

use std::sync::Arc;

use markdown_it::Renderer;

use crate::block::fill;
use crate::block::template::TemplateVars;
use crate::block::typecheck::TemplateType;
use crate::context;

/// 一节已经切好、选好模板的数据。
pub(super) struct Piece<'a> {
    /// 这一节要套的模板正文
    pub(super) template: &'a str,
    /// 这一节暴露给模板的变量（不含 `body` / `template` / `type` 保留名）
    pub(super) vars: Vec<(String, String)>,
}

/// 循环的公共上下文：变量名、模板页名、外层正文与类型。
pub(super) struct Repeat<'a> {
    as_name: &'a str,
    template_name: String,
    inherited_body: String,
    kind: TemplateType,
    parent: Option<Arc<TemplateVars>>,
}

impl<'a> Repeat<'a> {
    /// 从当前模板作用域取外层信息；没有外层时用 `fallback` 当模板名。
    pub(super) fn new(as_name: &'a str, fallback: &str) -> Self {
        let parent = context::template_vars();
        let inherited_body = parent
            .as_ref()
            .map(|vars| vars.body.clone())
            .unwrap_or_default();
        // `@template` 指向模板页，不是 `as=` 的名字
        let template_name = parent
            .as_ref()
            .map(|vars| vars.name.clone())
            .unwrap_or_else(|| fallback.to_string());
        let kind = parent
            .as_ref()
            .map_or(TemplateType::Block, |vars| vars.kind);
        Self {
            as_name,
            template_name,
            inherited_body,
            kind,
            parent,
        }
    }

    /// 逐节填空并把结果拼成一篇，交给嵌套渲染后直接写进当前格式器。
    pub(super) fn render<'b>(
        &self,
        pieces: impl IntoIterator<Item = Piece<'b>>,
        fmt: &mut dyn Renderer,
    ) -> Result<(), String> {
        let mut expanded = String::new();
        for piece in pieces {
            let vars = TemplateVars::new(
                &self.template_name,
                piece.vars,
                self.inherited_body.clone(),
                self.kind,
                self.parent.clone(),
            );
            let _scope = context::enter_template_vars(Arc::new(vars));
            let fragment = fill::resolve_late_refs(piece.template, Some(self.as_name))?;
            expanded.push_str(&fragment);
            expanded.push('\n');
        }

        let html = crate::engine::render_nested(&expanded);
        fmt.text_raw(&html);
        Ok(())
    }
}
