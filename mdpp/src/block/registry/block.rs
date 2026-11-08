use super::tokenize::{pairs, tokenize, tokenize_meta};

/// 块内容怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentMode {
    /// 交给整个解析器再解析一遍，子节点挂在这个块下。
    #[default]
    Markdown,
    /// 按 `[标签]` 分节，**各节各解析一遍**，每节一个子节点。标签在基础缩进上，
    /// 正文必须缩进在标签之下；标签层上出现没有标签头的正文是用法错误。
    ///
    /// 每节的原文仍保留在 [`Section::body`](crate::block::Section::body) 上，
    /// 所以 `each` / `rows` 这类既要分节、又要原文的块也用这一档。
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
    /// `key=value` 参数；只写了 `key` 的开关式参数，值是 `on`（裸 flag 是
    /// `key=on` 的语法糖）；显式 `key=` 才是空串
    pub params: Vec<(String, String)>,
    /// 块内容原文，已去掉公共缩进
    pub body: String,
}

impl Block {
    /// 解析头一行：`::名字 key=value …`。不是头就返回 `None`。
    ///
    /// 名字必须紧跟 `::`，且不能含 `=` / `:`。`::template:卡片` 不在这里特殊处理：
    /// 它是普通段落；模板命名空间归调用方识别。
    ///
    /// **宽松**：认不出的头一律返回 `None`，由调用方决定是普通段落还是报错。
    /// 模板调用要走严格检查时用内部的 `header_tokens` 与 `template::check_header`，
    /// 不要在公共解析器里改变普通块的行为。
    pub fn parse_header(line: &str) -> Option<(String, Vec<(String, String)>)> {
        let rest = line.trim_start().strip_prefix("::")?;
        let tokens = tokenize_meta(rest)?;
        let name = &tokens.first()?.text;
        if !valid_name(name) {
            return None;
        }
        let params = pairs(&tokens[1..]);
        Some((name.clone(), params))
    }

    /// 头行切成 token（名字也在里面）。**不校验名字**，交给调用方判断这是不是一块。
    ///
    /// 模板调用的严格检查要拿它看等号两侧的空格、重复参数名这些 [`Block`]
    /// 已经无从得知的写法。
    pub(crate) fn header_tokens(line: &str) -> Option<Vec<String>> {
        let rest = line.trim_start().strip_prefix("::")?;
        tokenize(rest)
    }

    /// 取一个参数
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// 这个开关有没有写。裸 `flag` 的值是 `on`，显式 `flag=` 是空串，
    /// 两者用 [`Block::param`] 就分得开。
    pub fn has(&self, key: &str) -> bool {
        self.params.iter().any(|(name, _)| name == key)
    }


}

/// 块名：不能为空，不能含 `=` 或 `:`。`::template:卡片` 是**普通段落**，
/// 模板命名空间是否识别由调用方自己决定。
fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('=') && !name.contains(':')
}
