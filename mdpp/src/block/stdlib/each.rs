//! 模板遍历原语：`::each`。
//!
//! 它把一段**模板**套在另一组分节上重复渲染。块本身是严格的
//! [`ContentMode::Sections`](crate::registry::ContentMode)，只认两节：
//!
//! ```mdpp
//! ::each
//!   [__template__]
//!     ... 每一节的模板，必须缩进 ...
//!   [__data__]
//!     [甲]
//!       第一节
//!     [乙]
//!       第二节
//! ```
//!
//! - `[__template__]` 的正文是模板；
//! - `[__data__]` 的正文才是数据；它自己再按普通分节规则切成 `[甲]` / `[乙]`。
//!
//! 模板页里通常把 caller 的正文放在 `[__data__]` 下面：
//!
//! ```mdpp
//! ::each
//!   [__template__]
//!     ...
//!   [__data__]
//!     {{body}}
//! ```
//!
//! 数据区的正文由 Rust 的分节扫描器切，`each` 只负责遍历；每一节的内容仍然
//! 缩进在自己的标签下面。
//!
//! ## 每一节的变量
//!
//! 模板里用 `{{@item.…}}`（`as=` 可以改名）：
//!
//! - `{{@item.label}}`：标签；
//! - `{{@item.body}}`：这一节的正文；
//! - `{{@item.index}}`：下标（0 开始）；
//! - `{{@item.count}}`：总节数；
//! - `{{@item.first}}` / `{{@item.last}}`：是不是第一 / 最后一节（`yes` 或空）。
//!
//! `{{@body}}` 仍然是 caller 的完整正文，不会和 `item.body` 冲突。
//!
//! ## 过滤
//!
//! - `only=甲,乙`：只遍历这些标签；
//! - `skip=above,below`：跳过这些标签；
//! - `required`：过滤后一节不剩就报用法问题；
//! - `name=` / `empty-why=`：诊断框里显示的名字与空列表时的那句话。
//!
//! `each` 自己不产生外层 DOM，类名和结构全部由模板片段决定。

use std::collections::HashSet;

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use super::super::fill;
use super::super::section;
use crate::block::template::TemplateVars;
use crate::context;
use crate::registry::Block;

/// `::each` 认的全部参数。
const PARAMS: &[&str] = &["as", "only", "skip", "required", "name", "empty-why"];

struct Params<'a> {
    as_name: &'a str,
    only: Option<&'a str>,
    skip: Option<&'a str>,
    required: bool,
    name: &'a str,
    empty_why: &'a str,
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
    let mut as_name = "item";
    let mut only = None;
    let mut skip = None;
    let mut required = false;
    let mut name = "each";
    let mut empty_why = "没有可遍历的分节";

    for (key, value) in &block.params {
        if !seen.insert(key.as_str()) {
            return Err(format!("`::each` 的参数 `{key}` 写了两次"));
        }
        match key.as_str() {
            "as" => {
                let Some(value) = present(value) else {
                    return Err("`::each` 的 as= 不能为空".to_string());
                };
                if !valid_name(value) {
                    return Err(format!(
                        "`::each` 的 as= 只能是普通名字，不能含空白、`.`、`@`、`{{`、`}}`：`{value}`"
                    ));
                }
                as_name = value;
            }
            "only" => only = present(value),
            "skip" => skip = present(value),
            "required" => required = fill::flag(block, "required"),
            "name" => name = present(value).unwrap_or("each"),
            "empty-why" => empty_why = present(value).unwrap_or("没有可遍历的分节"),
            other => {
                return Err(format!(
                    "`::each` 不认参数 `{other}`：只允许 {}",
                    PARAMS.join(" / ")
                ));
            }
        }
    }

    Ok(Params {
        as_name,
        only,
        skip,
        required,
        name,
        empty_why,
    })
}

fn has_label(list: &str, label: &str) -> bool {
    list.split(',').any(|one| one.trim() == label)
}

fn keep(label: &str, params: &Params<'_>) -> bool {
    if let Some(only) = params.only
        && !has_label(only, label)
    {
        return false;
    }
    if let Some(skip) = params.skip
        && has_label(skip, label)
    {
        return false;
    }
    true
}

fn named(block: &Block, name: &str) -> Block {
    let mut block = block.clone();
    block.name = name.to_string();
    block
}

/// `::each` —— 把模板套在分节上重复渲染。
pub(super) fn render_each(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let params = match parse(block) {
        Ok(params) => params,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    let (template, data) = match section::template_and_data(node, "each") {
        Ok((template, data)) => (template.body.as_str(), data.body.as_str()),
        Err(why) => {
            render_problem(&named(block, params.name), fmt, &why);
            return;
        }
    };
    if template.trim().is_empty() {
        render_problem(
            &named(block, params.name),
            fmt,
            "`::each` 的 [__template__] 是空的：模板正文要缩进在它下面",
        );
        return;
    }

    let items = if data.trim().is_empty() {
        Vec::new()
    } else {
        match section::parse(data) {
            Ok(items) => items,
            Err(why) => {
                render_problem(&named(block, params.name), fmt, &why);
                return;
            }
        }
    };
    let items: Vec<_> = items
        .iter()
        .filter(|item| keep(&item.label, &params))
        .collect();
    if items.is_empty() {
        if params.required {
            render_problem(&named(block, params.name), fmt, params.empty_why);
        }
        return;
    }

    let parent = context::template_vars();
    let inherited_body = parent
        .as_ref()
        .map(|vars| vars.body.clone())
        .unwrap_or_default();
    // `@template` 仍然指向模板页，不是 `as=` 的名字
    let template_name = parent
        .as_ref()
        .map(|vars| vars.name.clone())
        .unwrap_or_else(|| "each".to_string());
    let count = items.len();

    let mut expanded = String::new();
    for (index, item) in items.iter().enumerate() {
        let vars = TemplateVars::new(
            &template_name,
            vec![
                (format!("{}.label", params.as_name), item.label.clone()),
                (format!("{}.body", params.as_name), item.body.clone()),
                (format!("{}.index", params.as_name), index.to_string()),
                (format!("{}.count", params.as_name), count.to_string()),
                (
                    format!("{}.first", params.as_name),
                    if index == 0 { "yes" } else { "" }.to_string(),
                ),
                (
                    format!("{}.last", params.as_name),
                    if index + 1 == count { "yes" } else { "" }.to_string(),
                ),
            ],
            inherited_body.clone(),
            parent.clone(),
        );
        let _scope = context::enter_template_vars(std::sync::Arc::new(vars));
        match fill::resolve_late_refs(template, Some(params.as_name)) {
            Ok(fragment) => {
                expanded.push_str(&fragment);
                expanded.push('\n');
            }
            Err(why) => {
                render_problem(&named(block, params.name), fmt, &why);
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

    /// 每一节都拿得到 label / body / index；`@body` 仍是 caller 的完整正文
    #[test]
    fn each_renders_every_section_with_item_variables() {
        let template = "::each\n  [__template__]\n    ::element tag=p class=col\n      ::text\n        {{@item.label}}\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("列", template, "::列\n  [甲]\n    甲文\n  [乙]\n    乙文\n");
        assert_eq!(html.matches("<p class=\"col\">").count(), 2, "{html}");
        assert!(html.contains("甲") && html.contains("甲文"), "{html}");
        assert!(html.contains("乙") && html.contains("乙文"), "{html}");
    }

    /// `as=` 改名；父模板参数在循环里仍然取得到
    #[test]
    fn as_renames_item_variables_and_parent_params_survive() {
        let template = "::each as=col\n  [__template__]\n    ::text\n      {{@col.index}}={{@col.label}}/{{@title}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "列",
            template,
            "::列 title=标题\n  [甲]\n    甲文\n  [乙]\n    乙文\n",
        );
        assert!(html.contains("0=甲/标题"), "{html}");
        assert!(html.contains("1=乙/标题"), "{html}");
    }

    /// `only=` / `skip=` 过滤；没有匹配时默认什么都不输出
    #[test]
    fn only_and_skip_filter_the_data() {
        let only = "::each only=above\n  [__template__]\n    ::text\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "只上",
            only,
            "::只上\n  [above]\n    上面\n  [甲]\n    中间\n",
        );
        assert!(html.contains("上面"), "{html}");
        assert!(!html.contains("中间"), "{html}");

        let skip = "::each skip=above\n  [__template__]\n    ::text\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "跳上",
            skip,
            "::跳上\n  [above]\n    上面\n  [甲]\n    中间\n",
        );
        assert!(!html.contains("上面"), "{html}");
        assert!(html.contains("中间"), "{html}");
    }

    /// 过滤后没有内容：不写 required 就静默，不产生空壳
    #[test]
    fn required_reports_an_empty_list_but_optional_stays_silent() {
        let silent = "::each only=没有\n  [__template__]\n    ::text\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("空", silent, "::空\n  [甲]\n    甲文\n");
        assert!(!html.contains("template--problem"), "{html}");

        let required = "::each only=没有 required empty-why=一节也没有\n  [__template__]\n    ::text\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("空", required, "::空\n  [甲]\n    甲文\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("一节也没有"), "{html}");
    }

    /// 数据区里的裸行不是匿名节；它要么缩进成 [标签] 的正文，要么报错
    #[test]
    fn a_leading_paragraph_in_the_data_section_is_refused() {
        let template = "::each\n  [__template__]\n    {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "列",
            template,
            "::列\n  没有标签的开场白\n  [甲]\n    甲文\n",
        );
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("第一行必须是 [标签]"), "{html}");
    }

    /// 模板节必须存在、排在数据节之前，且必须有内容
    #[test]
    fn the_template_section_must_be_present_first_and_non_empty() {
        let wrong = "::each\n  [__data__]\n    {{body}}\n";
        let html = render_page("错", wrong, "::错\n  [甲]\n    甲文\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("[__template__]"), "{html}");

        let empty = "::each\n  [__template__]\n  [__data__]\n    {{body}}\n";
        let html = render_page("空", empty, "::空\n  [甲]\n    甲文\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("模板正文要缩进"), "{html}");
    }

    /// `each` 解析后的标签能直接进 `::if` 的条件
    #[test]
    fn item_variables_can_drive_if_conditions() {
        let template = "::each as=row\n  [__template__]\n    ::if condition-equals={{@row.label}} value=show\n      [then]\n        ::text\n          {{@row.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "筛选",
            template,
            "::筛选\n  [show]\n    显示\n  [hide]\n    隐藏\n",
        );
        assert!(html.contains("显示"), "{html}");
        assert!(!html.contains("隐藏"), "{html}");
    }

    /// 内层 each 的变量由内层解析，外层只解析自己的前缀
    #[test]
    fn nested_each_keeps_the_outer_prefix_for_the_inner_scope() {
        let template = "::each as=outer\n  [__template__]\n    ::each as=inner\n      [__template__]\n        ::text\n          {{@inner.label}}/{{@outer.label}}\n      [__data__]\n        {{@outer.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("嵌套", template, "::嵌套\n  [外]\n    [子]\n      子文\n");
        assert!(html.contains("子/外"), "{html}");
    }

    /// 空数据与过滤后为空是两条路：前者默认静默，`required` 才报
    #[test]
    fn empty_data_is_silent_unless_required() {
        let template = "::each required empty-why=没有数据\n  [__template__]\n    ::text\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page("空", template, "::空\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("没有数据"), "{html}");
    }
}
