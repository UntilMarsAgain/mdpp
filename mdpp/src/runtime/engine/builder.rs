use std::sync::Arc;

use crate::parse::dialect::{self, Mode, SyntaxOptions};
use crate::registry::BlockRegistry;
use crate::telemetry::debug;

use super::Mdpp;

/// [`Mdpp`] 的构建器。
///
/// 默认值见模块头；每一项都能单独改。
#[derive(Debug)]
pub struct MdppBuilder {
    registry: Option<BlockRegistry>,
    syntax: SyntaxOptions,
}

impl Default for MdppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MdppBuilder {
    /// 默认设置
    pub fn new() -> Self {
        Self {
            registry: None,
            syntax: SyntaxOptions::default(),
        }
    }

    /// 换掉块注册表（默认是 [`BlockRegistry::builtin`]）。
    ///
    /// 想「内置的 + 我自己的」就从 [`BlockRegistry::with_builtins`] 开始灌。
    pub fn registry(mut self, registry: BlockRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    /// 行内数学公式 `$…$`（默认开）
    pub fn math(mut self, on: bool) -> Self {
        self.syntax.math = on;
        self
    }

    /// 内部链接 `[[…]]`（默认开）
    pub fn wikilink(mut self, on: bool) -> Self {
        self.syntax.wikilink = on;
        self
    }

    /// 表格（默认开）
    pub fn tables(mut self, on: bool) -> Self {
        self.syntax.tables = on;
        self
    }

    /// raw HTML：正文里的 HTML 原样透传。默认**关**，理由在模块头。要在自己那套
    /// 已经做过净化的管线里用它，才打开。
    pub fn raw_html(mut self, on: bool) -> Self {
        self.syntax.raw_html = on;
        self
    }

    /// 给标题加 `id`（默认开）。关掉之后 `[跳转](#标题)` 就少了个落点。
    pub fn heading_anchors(mut self, on: bool) -> Self {
        self.syntax.heading_anchors = on;
        self
    }

    /// Setext 标题（`标题` 下一行写 `===` 或 `---`）。默认**关**。
    ///
    /// 理由是它和 `---`（thematic break）撞：摘掉之后 `标题\n---` 老老实实退化成
    /// 「段落 + 分隔线」，开着的时候它是标题 —— 一段写错了的正文会变成另一种意思。
    pub fn setext(mut self, on: bool) -> Self {
        self.syntax.setext = on;
        self
    }

    /// 缩进代码块（四空格开头）。默认**关**。理由见模块头。
    pub fn indented_code(mut self, on: bool) -> Self {
        self.syntax.indented_code = on;
        self
    }

    /// 排版替换（`--` → `–`、直引号 → 弯引号）。默认**关**。理由见模块头。
    pub fn typographer(mut self, on: bool) -> Self {
        self.syntax.typographer = on;
        self
    }

    /// 建出来
    pub fn build(self) -> Mdpp {
        let options = self.syntax;
        let registry = Arc::new(self.registry.unwrap_or_else(BlockRegistry::builtin));

        // 方言的唯一装配点在 `parse::dialect`：渲染器与原始区扫描共用同一套
        // `::` 规则，两边的块边界因此不会各写一份。
        let md = dialect::build(Mode::Render, &options);

        // 建好之后报一句「这一份解析器带了什么」—— 排查「为什么这个块没生效」时，
        debug!(
            blocks = registry.len(),
            names = ?registry.names(),
            "解析器已建好"
        );

        Mdpp {
            parser: Arc::new(md),
            registry,
        }
    }
}
