//! 行表格原语：`::rows`。
//!
//! 它把「每行若干格」的数据套在一段模板上重复渲染。块本身同样是严格的
//! [`ContentMode::Sections`](crate::registry::ContentMode)，只认两节：
//!
//! ```mdpp
//! ::rows
//!   [__template__]
//!     ::element tag=td
//!       ::text
//!         {{@row.0}}
//!   [__data__]
//!     甲|乙|丙
//! ```
//!
//! - `[__template__]` 的正文是模板；
//! - `[__data__]` 的正文是数据行；`rows` 用 [`Block::rows_limited`] 把每行拆成单元格。
//!
//! `max=` 限制每行最多拆几格，第 N 格保留剩下的 `|`（`fields` 用 `max=2`）。
//! 空行跳过，单元格两边去空白。
//!
//! 每行可用变量：
//!
//! - `{{@row.0}}` / `{{@row.1}}` / …：单元格；
//! - `{{@row.count}}`：这一行有几格；
//! - `{{@row.index}}`：第几行（0 开始）；
//! - `{{@row.first}}` / `{{@row.last}}`：是不是第一 / 最后一行。
//!
//! `{{@body}}` 仍是 caller 的完整正文；`as=` 可以改名，例如 `as=cell`。

use std::collections::HashSet;

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use super::super::fill;
use super::super::section;
use crate::block::template::TemplateVars;
use crate::context;
use crate::registry::Block;

const PARAMS: &[&str] = &["as", "max"];

struct Params<'a> {
    as_name: &'a str,
    max: Option<usize>,
}

fn present(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || matches!(ch, '.' | '@' | '{' | '}'))
}

fn parse(block: &Block) -> Result<Params<'_>, String> {
    let mut seen = HashSet::new();
    let mut as_name = "row";
    let mut max = None;

    for (key, value) in &block.params {
        if !seen.insert(key.as_str()) {
            return Err(format!("`::rows` 的参数 `{key}` 写了两次"));
        }
        match key.as_str() {
            "as" => {
                let Some(value) = present(value) else {
                    return Err("`::rows` 的 as= 不能为空".to_string());
                };
                if !valid_name(value) {
                    return Err(format!(
                        "`::rows` 的 as= 只能是普通名字，不能含空白、`.`、`@`、`{{`、`}}`：`{value}`"
                    ));
                }
                as_name = value;
            }
            "max" => {
                let Some(value) = present(value) else {
                    return Err("`::rows` 的 max= 不能为空".to_string());
                };
                let Some(number) = value.parse::<usize>().ok().filter(|number| *number > 0) else {
                    return Err("`::rows` 的 max= 只认大于 0 的整数".to_string());
                };
                max = Some(number);
            }
            other => {
                return Err(format!(
                    "`::rows` 不认参数 `{other}`：只允许 {}",
                    PARAMS.join(" / ")
                ));
            }
        }
    }

    Ok(Params { as_name, max })
}

/// `::rows` —— 行表格的遍历原语。
pub(super) fn render_rows(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let params = match parse(block) {
        Ok(params) => params,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let (template, data) = match section::template_and_data(node, "rows") {
        Ok((template, data)) => (template.body.as_str(), data.body.as_str()),
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    if template.trim().is_empty() {
        render_problem(
            block,
            fmt,
            "`::rows` 的 [__template__] 是空的：模板正文要缩进在它下面",
        );
        return;
    }

    let data_block = Block {
        name: "rows".to_string(),
        params: Vec::new(),
        body: data.to_string(),
    };
    let rows = data_block.rows_limited(params.max);
    if rows.is_empty() {
        return;
    }

    let parent = context::template_vars();
    let inherited_body = parent
        .as_ref()
        .map(|vars| vars.body.clone())
        .unwrap_or_default();
    let template_name = parent
        .as_ref()
        .map(|vars| vars.name.clone())
        .unwrap_or_else(|| "rows".to_string());
    let count = rows.len();

    let mut expanded = String::new();
    for (index, cells) in rows.iter().enumerate() {
        let mut vars = Vec::with_capacity(cells.len() + 5);
        for (cell, value) in cells.iter().enumerate() {
            vars.push((format!("{}.{cell}", params.as_name), value.clone()));
        }
        vars.push((format!("{}.count", params.as_name), cells.len().to_string()));
        vars.push((format!("{}.index", params.as_name), index.to_string()));
        vars.push((
            format!("{}.first", params.as_name),
            if index == 0 { "yes" } else { "" }.to_string(),
        ));
        vars.push((
            format!("{}.last", params.as_name),
            if index + 1 == count { "yes" } else { "" }.to_string(),
        ));

        let vars = TemplateVars::new(&template_name, vars, inherited_body.clone(), parent.clone());
        let _scope = context::enter_template_vars(std::sync::Arc::new(vars));
        match fill::resolve_late_refs(template, Some(params.as_name)) {
            Ok(fragment) => {
                expanded.push_str(&fragment);
                expanded.push('\n');
            }
            Err(why) => {
                render_problem(block, fmt, &why);
                return;
            }
        }
    }

    let html = crate::engine::render_nested(&expanded);
    fmt.text_raw(&html);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::block::template::{TemplatePage, TemplateSource};
    use crate::{Mdpp, RenderContext};

    #[derive(Debug, Default)]
    struct Pages(HashMap<String, TemplatePage>);

    impl TemplateSource for Pages {
        fn page(&self, name: &str) -> Option<TemplatePage> {
            self.0.get(name).cloned()
        }
    }

    fn render_page(page: &str, template: &str, source: &str) -> String {
        let mut pages = Pages::default();
        pages
            .0
            .insert(page.to_string(), TemplatePage::Ready(template.to_string()));
        let context = RenderContext::new().with_templates(Arc::new(pages));
        Mdpp::new().render_with(source, &context)
    }

    /// 一行几格都由 `row.N` 取；不同行可以列数不同
    #[test]
    fn rows_expose_cells_by_index() {
        let template = "::rows\n  [__template__]\n    ::text\n      {{@row.0}}/{{@row.1}}/{{@row.2}};\n  [__data__]\n    {{body}}\n";
        let html = render_page("表", template, "::表\n  甲|乙|丙\n  丁|戊|己\n");
        assert!(html.contains("甲/乙/丙;"), "{html}");
        assert!(html.contains("丁/戊/己;"), "{html}");
    }

    /// `max=` 只拆前几刀，剩下的 `|` 留在最后一格
    #[test]
    fn max_keeps_the_rest_of_the_line_in_the_last_cell() {
        let template = "::rows max=2\n  [__template__]\n    ::text\n      {{@row.0}} => {{@row.1}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("表", template, "::表\n  甲|乙|丙\n");
        assert!(html.contains("甲 =&gt; 乙|丙"), "{html}");
    }

    /// 缺列用可选引用；没有 `|` 的行是只有一格的行
    #[test]
    fn short_rows_leave_missing_cells_empty_when_optional() {
        let template = "::rows\n  [__template__]\n    ::text\n      {{@row.0}}|{{?@row.1}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("表", template, "::表\n  只有一格\n");
        assert!(html.contains("只有一格|"), "{html}");
    }

    /// 数据行里的 `[x]` 只是数据，不是分节
    #[test]
    fn data_lines_that_look_like_labels_stay_data() {
        let template = "::rows\n  [__template__]\n    ::text\n      {{@row.0}}={{@row.1}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("表", template, "::表\n  [注]|说明\n");
        assert!(html.contains("[注]=说明"), "{html}");
    }

    /// `as=` 改名，`index` / `count` / `first` / `last` 都在
    #[test]
    fn as_renames_and_row_metadata_is_available() {
        let template = "::rows as=cell\n  [__template__]\n    ::text\n      {{@cell.index}}/{{@cell.count}} {{@cell.0}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("表", template, "::表\n  甲\n  乙\n");
        assert!(html.contains("0/1 甲"), "{html}");
        assert!(html.contains("1/1 乙"), "{html}");
    }

    /// 空数据不输出；模板缺标记、模板为空、max 非法都报用法问题
    #[test]
    fn bad_headers_and_empty_data_are_handled() {
        let template =
            "::rows\n  [__template__]\n    ::text\n      {{@row.0}}\n  [__data__]\n    {{body}}\n";
        let empty = render_page("表", template, "::表\n");
        assert!(!empty.contains("template--problem"), "{empty}");

        for (page, source) in [
            ("坏1", "::rows\n  [__data__]\n    {{body}}\n"),
            (
                "坏2",
                "::rows\n  [__template__]\n  [__data__]\n    {{body}}\n",
            ),
            (
                "坏3",
                "::rows max=0\n  [__template__]\n    {{@row.0}}\n  [__data__]\n    {{body}}\n",
            ),
            (
                "坏4",
                "::rows as=@row\n  [__template__]\n    {{@row.0}}\n  [__data__]\n    {{body}}\n",
            ),
        ] {
            let html = render_page(page, source, &format!("::{page}\n  甲|乙\n"));
            assert!(html.contains("template--problem"), "{page}: {html}");
        }
    }
}
