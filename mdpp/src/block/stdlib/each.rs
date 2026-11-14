//! 模板遍历原语：`::each`。
//!
//! 它把一段**模板**套在另一组分节上重复渲染。块本身是严格的
//! [`ContentMode::Sections`](crate::registry::ContentMode)：
//!
//! ```mdpp
//! ::each
//!   [__template__]
//!     ... 默认模板，必须缩进 ...
//!   [__template__ link]
//!     ... 类型为 link 的模板 ...
//!   [__data__]
//!     [link 甲]
//!       第一节
//!     [乙]
//!       第二节
//! ```
//!
//! - `[__template__]` 是默认模板，必须有且只能有一个；
//! - `[__template__ 类型]` 是可选模板，按数据标签的第一个词选择；
//! - `[__data__]` 的正文才是数据；它自己再按普通分节规则切成 `[类型 数据]`
//!   或 `[标签]`。类型没有对应模板时，整条标签走默认模板。
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
//! 数据的标签如果是 `[类型 数据]`，并且有对应的 `[__template__ 类型]`，就使用
//! 那个模板，`{{@item.label}}` 拿到的是 `数据` 部分，`{{@item.type}}` 拿到的是
//! `类型`。没有对应模板时，仍用默认模板，`{{@item.label}}` 是整条标签。
//!
//! ## 每一节的变量
//!
//! 模板里用 `{{@item.…}}`（`as=` 可以改名）：
//!
//! - `{{@item.label}}`：标签；使用类型模板时是类型后面的数据；
//! - `{{@item.type}}`：类型；使用默认模板时为空；
//! - `{{@item.body}}`：这一节的正文；
//! - `{{@item.index}}`：下标（0 开始）；
//! - `{{@item.count}}`：总节数；
//! - `{{@item.first}}` / `{{@item.last}}`：是不是第一 / 最后一节（`yes` 或空）；
//! - `{{@item.empty}}` / `{{@item.body_empty}}`：标签的数据部分 / 正文是不是空的
//!   （`yes` 或空），供模板做布尔判断，省得把可能带空格的文本塞进条件参数。
//!
//! `{{@body}}` 仍然是 caller 的完整正文，不会和 `item.body` 冲突。
//!
//! ## 过滤
//!
//! - `only=甲,乙`：只遍历这些标签；匹配的是数据标签的原文；
//! - `skip=above,below`：跳过这些标签；
//! - `required`：过滤后一节不剩就报用法问题；
//! - `min=` / `max=`：过滤后的节数范围；
//! - `nonempty`：每一节的正文都不能为空；
//! - `check`：只做上面的校验，不渲染模板；
//! - `name=` / `empty-why=`：诊断框里显示的名字与空列表时的那句话。
//!
//! `each` 自己不产生外层 DOM，类名和结构全部由模板片段决定。

use std::collections::HashSet;

use markdown_it::{Node, Renderer};

use super::super::dispatch::render_problem;
use super::super::fill;
use super::super::section;
use super::{present, valid_name};
use crate::block::template::TemplateVars;
use crate::context;
use crate::registry::Block;

/// `::each` 认的全部参数。
const PARAMS: &[&str] = &[
    "as",
    "only",
    "skip",
    "required",
    "min",
    "max",
    "nonempty",
    "check",
    "name",
    "empty-why",
];

struct Params<'a> {
    as_name: &'a str,
    only: Option<&'a str>,
    skip: Option<&'a str>,
    min: usize,
    max: Option<usize>,
    nonempty: bool,
    check: bool,
    name: &'a str,
    empty_why: &'a str,
}

fn parse(block: &Block) -> Result<Params<'_>, String> {
    let mut seen = HashSet::new();
    let mut as_name = "item";
    let mut only = None;
    let mut skip = None;
    let mut required = false;
    let mut min = 0usize;
    let mut max = None;
    let mut nonempty = false;
    let mut check = false;
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
            "min" | "max" => {
                let Some(value) = present(value) else {
                    return Err(format!("`::each` 的 {key}= 不能为空"));
                };
                let Some(number) = value.parse::<usize>().ok() else {
                    return Err(format!("`::each` 的 {key}= 只认非负整数"));
                };
                if key == "min" {
                    min = number;
                } else {
                    max = Some(number);
                }
            }
            "nonempty" => nonempty = fill::flag(block, "nonempty"),
            "check" => check = fill::flag(block, "check"),
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

    if required && min == 0 {
        min = 1;
    }
    if let Some(max) = max
        && min > max
    {
        return Err(format!("`::each` 的 min= 不能大于 max=：{min} > {max}"));
    }

    Ok(Params {
        as_name,
        only,
        skip,
        min,
        max,
        nonempty,
        check,
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

/// 模板类块的三部分：默认模板、类型模板、数据节。
struct Templates<'a> {
    default: &'a str,
    typed: Vec<(&'a str, &'a str)>,
    data: &'a section::Section,
}

/// 解析 `[__template__]`、`[__template__ 类型]`、`[__data__]`。
///
/// 模板节可以写在数据节之前，顺序随意；数据节必须最后。默认模板必须有，
/// 类型模板可以没有。类型只认第一个词，且不能为空或再含空白。
fn templates_and_data<'a>(node: &'a Node, what: &str) -> Result<Templates<'a>, String> {
    let mut default = None;
    let mut typed: Vec<(&str, &str)> = Vec::new();
    let mut data = None;
    let mut seen_data = false;
    let mut labels: Vec<&str> = Vec::new();

    for child in &node.children {
        let Some(section) = child.cast::<section::Section>() else {
            continue;
        };
        labels.push(section.label.as_str());
        if seen_data {
            return Err(format!(
                "`::{what}` 的 [__data__] 必须是最后一节；后面还有 [{}]",
                section.label
            ));
        }
        if section.label == "__data__" {
            if data.is_some() {
                return Err(format!("`::{what}` 只能有一个 [__data__]"));
            }
            data = Some(section);
            seen_data = true;
            continue;
        }

        let kind = if section.label == "__template__" {
            None
        } else if let Some(rest) = section.label.strip_prefix("__template__ ") {
            let kind = rest.trim();
            if kind.is_empty() {
                return Err(format!(
                    "`::{what}` 的类型模板缺少类型：[{}]",
                    section.label
                ));
            }
            if kind.chars().any(char::is_whitespace) {
                return Err(format!(
                    "`::{what}` 的类型只能是一个词：[{}]",
                    section.label
                ));
            }
            Some(kind)
        } else {
            return Err(format!(
                "`::{what}` 只认 [__template__]、[__template__ 类型] 和 [__data__]；这里读到的是 [{}]",
                section.label
            ));
        };

        match kind {
            None => {
                if default.is_some() {
                    return Err(format!("`::{what}` 只能有一个默认 [__template__]"));
                }
                default = Some(section);
            }
            Some(kind) => {
                if typed.iter().any(|(seen, _)| *seen == kind) {
                    return Err(format!("`::{what}` 的类型模板 `{kind}` 写了两次"));
                }
                typed.push((kind, section.body.as_str()));
            }
        }
    }

    let default = default
        .ok_or_else(|| format!("`::{what}` 缺少默认 [__template__]；读到的是 {labels:?}"))?;
    let data = data.ok_or_else(|| format!("`::{what}` 缺少 [__data__]；读到的是 {labels:?}"))?;

    Ok(Templates {
        default: default.body.as_str(),
        typed,
        data,
    })
}

/// 把数据标签拆成 `(类型, 数据)`；没有空格时数据就是整条标签。
fn split_label(label: &str) -> (&str, &str) {
    let mut parts = label.splitn(2, char::is_whitespace);
    let head = parts.next().unwrap_or(label);
    let rest = parts.next().unwrap_or("").trim_start();
    (head, rest)
}

/// 一节选中的模板和它暴露给模板的标签。
struct Item<'a> {
    raw: &'a section::RawSection,
    template: &'a str,
    kind: &'a str,
    label: &'a str,
}

fn select<'a>(items: &'a [section::RawSection], templates: &'a Templates<'a>) -> Vec<Item<'a>> {
    items
        .iter()
        .map(|item| {
            let (head, rest) = split_label(&item.label);
            if let Some((kind, template)) = templates.typed.iter().find(|(kind, _)| *kind == head) {
                Item {
                    raw: item,
                    template,
                    kind,
                    label: rest,
                }
            } else {
                Item {
                    raw: item,
                    template: templates.default,
                    kind: "",
                    label: &item.label,
                }
            }
        })
        .collect()
}

fn keep_item(item: &Item<'_>, params: &Params<'_>) -> bool {
    keep(&item.raw.label, params)
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

    let templates = match templates_and_data(node, "each") {
        Ok(templates) => templates,
        Err(why) => {
            render_problem(&named(block, params.name), fmt, &why);
            return;
        }
    };
    if templates.default.trim().is_empty() && !params.check {
        render_problem(
            &named(block, params.name),
            fmt,
            "`::each` 的 [__template__] 是空的：模板正文要缩进在它下面",
        );
        return;
    }

    let raw_items = if templates.data.body.trim().is_empty() {
        Vec::new()
    } else {
        match section::parse(&templates.data.body) {
            Ok(items) => items,
            Err(why) => {
                render_problem(&named(block, params.name), fmt, &why);
                return;
            }
        }
    };
    let items: Vec<_> = select(&raw_items, &templates)
        .into_iter()
        .filter(|item| keep_item(item, &params))
        .collect();
    if items.len() < params.min {
        render_problem(&named(block, params.name), fmt, params.empty_why);
        return;
    }
    if let Some(max) = params.max
        && items.len() > max
    {
        render_problem(
            &named(block, params.name),
            fmt,
            &format!("`::each` 最多允许 {max} 节，这里是 {} 节", items.len()),
        );
        return;
    }
    if params.nonempty
        && let Some(item) = items.iter().find(|item| item.raw.body.trim().is_empty())
    {
        render_problem(
            &named(block, params.name),
            fmt,
            &format!(
                "`::each` 的 [{}] 是空的：这一节不能没有正文",
                item.raw.label
            ),
        );
        return;
    }
    if params.check {
        return;
    }
    if let Some(item) = items.iter().find(|item| item.template.trim().is_empty()) {
        let why = if item.kind.is_empty() {
            "`::each` 的 [__template__] 是空的：模板正文要缩进在它下面".to_string()
        } else {
            format!(
                "`::each` 的 [__template__ {}] 是空的：模板正文要缩进在它下面",
                item.kind
            )
        };
        render_problem(&named(block, params.name), fmt, &why);
        return;
    }
    if items.is_empty() {
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
                (format!("{}.label", params.as_name), item.label.to_string()),
                (format!("{}.type", params.as_name), item.kind.to_string()),
                (format!("{}.body", params.as_name), item.raw.body.clone()),
                (
                    format!("{}.empty", params.as_name),
                    if item.label.trim().is_empty() {
                        "yes"
                    } else {
                        ""
                    }
                    .to_string(),
                ),
                (
                    format!("{}.body_empty", params.as_name),
                    if item.raw.body.trim().is_empty() {
                        "yes"
                    } else {
                        ""
                    }
                    .to_string(),
                ),
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
        match fill::resolve_late_refs(item.template, Some(params.as_name)) {
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
    /// 类型模板按数据标签的第一个词选择；`item.label` 只剩数据部分。
    #[test]
    fn typed_templates_select_by_the_first_word_of_the_label() {
        let template = "::each\n  [__template__]\n    ::text\n      默认:{{@item.type}}/{{@item.label}}\n  [__template__ link]\n    ::text\n      链接:{{@item.type}}/{{@item.label}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "类型",
            template,
            "::类型\n  [link 甲|甲页]\n    甲文\n  [乙 二]\n    乙文\n",
        );
        assert!(html.contains("链接:link/甲|甲页"), "{html}");
        assert!(html.contains("默认:/乙 二"), "{html}");
        assert!(!html.contains("默认:/link"), "{html}");
    }

    /// 类型模板可以没有数据部分；`[header]` 仍能选到 header 模板。
    #[test]
    fn a_typed_template_can_take_an_empty_data_part() {
        let template = "::each\n  [__template__]\n    ::text\n      组:{{@item.label}}\n  [__template__ header]\n    ::text\n      头:{{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "头",
            template,
            "::头\n  [header]\n    通栏\n  [分类]\n    内容\n",
        );
        assert!(html.contains("头:通栏"), "{html}");
        assert!(html.contains("组:分类"), "{html}");
        assert!(!html.contains("header"), "{html}");
    }

    /// 数据节里的标签仍按原文走 only/skip；类型模板不改变过滤口径。
    #[test]
    fn typed_templates_do_not_change_the_filter_labels() {
        let template = "::each skip=header\n  [__template__]\n    ::text\n      {{@item.label}}\n  [__template__ header]\n    ::text\n      {{@item.body}}\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "过滤",
            template,
            "::过滤\n  [header]\n    通栏\n  [分类]\n    内容\n",
        );
        assert!(!html.contains("通栏"), "{html}");
        assert!(html.contains("分类"), "{html}");
    }

    /// 类型模板重复、类型为空或出现不认识的模板节，都应在写 DOM 前报错。
    #[test]
    fn malformed_typed_templates_are_refused() {
        let duplicate = "::each\n  [__template__]\n    默认\n  [__template__ link]\n    一\n  [__template__ link]\n    二\n  [__data__]\n    {{body}}\n";
        let html = render_page("重复", duplicate, "::重复\n  [link 甲]\n    文\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("写了两次"), "{html}");

        let spaced = "::each\n  [__template__]\n    默认\n  [__template__ two words]\n    错\n  [__data__]\n    {{body}}\n";
        let html = render_page("空类型", spaced, "::空类型\n  [甲]\n    文\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("类型只能是一个词"), "{html}");

        let unknown = "::each\n  [__template__]\n    默认\n  [__template link]\n    错\n  [__data__]\n    {{body}}\n";
        let html = render_page("错节", unknown, "::错节\n  [甲]\n    文\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("只认"), "{html}");
    }

    /// 选中的类型模板为空时，不能把它当成空模板静默跳过。
    #[test]
    fn an_empty_selected_typed_template_is_reported() {
        let template = "::each\n  [__template__]\n    默认\n  [__template__ link]\n  [__data__]\n    {{body}}\n";
        let html = render_page(
            "空模板",
            template,
            "::空模板\n  [link 甲]\n    正文\n  [乙]\n    正文\n",
        );
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("[__template__ link]"), "{html}");
    }

    /// `check` 只校验不输出；`max` / `min` / `nonempty` 都在写 DOM 前拦住
    #[test]
    fn check_validates_counts_and_empty_bodies_without_rendering() {
        let check = "::each only=above max=1 check\n  [__template__]\n  [__data__]\n    {{body}}\n";
        let ok = render_page("查", check, "::查\n  [above]\n    一行\n");
        assert!(!ok.contains("template--problem"), "{ok}");
        assert!(!ok.contains("一行"), "{ok}");

        let too_many = render_page("查", check, "::查\n  [above]\n    一\n  [above]\n    二\n");
        assert!(too_many.contains("template--problem"), "{too_many}");

        let nonempty =
            "::each only=header nonempty check\n  [__template__]\n  [__data__]\n    {{body}}\n";
        let empty = render_page("查", nonempty, "::查\n  [header]\n\n  [header]\n    有字\n");
        assert!(empty.contains("template--problem"), "{empty}");

        let at_least = "::each skip=above,below,header min=1 check\n  [__template__]\n  [__data__]\n    {{body}}\n";
        let none = render_page("查", at_least, "::查\n  [header]\n  只有通栏\n");
        assert!(none.contains("template--problem"), "{none}");
    }
}
