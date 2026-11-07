use super::tokenize::{pairs, tokenize};

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
