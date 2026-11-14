//! 把一段节点的 HTML 收进字符串，而不是立刻交给外层渲染器。
//!
//! `::guard` 要先确认整段内容没有写出问题框，才能决定是否把 HTML 交给外层；
//! 因此它需要一份与 `markdown-it` 默认 HTML 渲染器等价、但结果可丢弃的渲染器。
//! 这里的属性合并规则与 `markdown-it` 的 `HTMLRenderer` 保持一致：`class` 用空格、
//! `style` 用分号合并，其余属性逐个输出。

use std::collections::HashMap;

use html_escape::{encode_double_quoted_attribute, encode_text};
use markdown_it::parser::extset::RenderExtSet;
use markdown_it::{Node, Renderer};

/// 只收集 HTML 的渲染器。
#[derive(Debug, Default)]
pub(crate) struct Capture {
    result: String,
    ext: RenderExtSet,
}

impl Capture {
    /// 取出收集到的 HTML。
    pub(crate) fn into_string(self) -> String {
        self.result
    }

    fn attr(&mut self, name: &str, value: &str) {
        self.result.push(' ');
        self.result.push_str(&encode_text(name));
        self.result.push_str("=\"");
        self.result.push_str(&encode_double_quoted_attribute(value));
        self.result.push('"');
    }

    fn attrs(&mut self, attrs: &[(&str, String)]) {
        let mut values: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut order = Vec::with_capacity(attrs.len());
        for (name, value) in attrs {
            values.entry(name).or_default().push(value);
            order.push(*name);
        }
        for name in order {
            let Some(values) = values.remove(name) else {
                continue;
            };
            match name {
                "class" => self.attr(name, &values.join(" ")),
                "style" => self.attr(name, &values.join(";")),
                _ => {
                    for value in values {
                        self.attr(name, value);
                    }
                }
            }
        }
    }
}

impl Renderer for Capture {
    fn open(&mut self, tag: &str, attrs: &[(&str, String)]) {
        self.result.push('<');
        self.result.push_str(tag);
        self.attrs(attrs);
        self.result.push('>');
    }

    fn close(&mut self, tag: &str) {
        self.result.push_str("</");
        self.result.push_str(tag);
        self.result.push('>');
    }

    fn self_close(&mut self, tag: &str, attrs: &[(&str, String)]) {
        self.result.push('<');
        self.result.push_str(tag);
        self.attrs(attrs);
        self.result.push('>');
    }

    fn contents(&mut self, nodes: &[Node]) {
        for node in nodes {
            node.node_value.render(node, self);
        }
    }

    fn cr(&mut self) {
        if !matches!(self.result.as_bytes().last(), Some(b'\n') | None) {
            self.result.push('\n');
        }
    }

    fn text(&mut self, text: &str) {
        self.result.push_str(&encode_text(text));
    }

    fn text_raw(&mut self, text: &str) {
        self.result.push_str(text);
    }

    fn ext(&mut self) -> &mut RenderExtSet {
        &mut self.ext
    }
}
