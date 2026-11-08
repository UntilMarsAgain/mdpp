//! 控制流原语：`::if`。
//!
//! 它不是模板页 —— 「选择哪一枝」是模板引擎自己的活，用一页 `.mdpp` 写 `if`
//! 只会绕回同一个问题。这里只提供最小的命名测试：
//!
//! ```text
//! ::if condition-exist={{@body}}
//!   [then]
//!   有正文时才出现
//!   [else]
//!   没有正文
//! ```
//!
//! `{{@名字}}` 是晚绑定引用：块头已经分完词，值里有空格、引号、换行也不会破坏
//! 参数结构。条件在渲染时从当前模板作用域取值，所以裸 flag（`active`）与
//! 「变量不存在」能分开：
//!
//! - `condition-exist`：存在且去掉空白后非空；
//! - `condition-defined`：存在即可（空值/裸 flag 也算）；
//! - `condition-empty`：不存在或空白；
//! - `condition-equals`：与 `value=` 去空白后完全相等，大小写敏感；
//! - `condition-truthy`：存在且不是空串、`0`、`false`、`no`、`off`；
//! - `condition-yesno`：值（去空白、忽略大小写）是 `yes` / `on` / `true` / `1` 时为真。
//!
//! `condition-not` 反转最后结果。多个测试会报用法问题；需要「且」时嵌套 `::if`，
//! 需要「或」时等将来的 `::choose` 或表达式。
//!
//! `[then]` 必填，`[else]` 可选；正文里出现未知标签、重复标签或标签前正文都算
//! 用法问题。两枝都会被解析，只渲染选中那一枝。

use markdown_it::{Node, Renderer};

use super::super::dispatch::{no_leading_content, render_problem, sections};
use crate::block::fill;
use crate::context;
use crate::registry::Block;

/// `::if condition-*={{@名字}} [condition-not]`。
pub(super) fn render_if(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let sections = sections(node);
    if let Err(why) = check_sections(&sections) {
        render_problem(block, fmt, &why);
        return;
    }

    let test = match condition(block) {
        Ok(test) => test,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    let wanted = if test { "then" } else { "else" };
    fmt.cr();
    for (label, section) in sections {
        if label == wanted {
            fmt.contents(&section.children);
        }
    }
    fmt.cr();
}

/// 一个条件测试：`(键, 原始值)`。
///
/// 值还保留着 `{{@名字}}` —— 这里不做早绑定替换，渲染时才查作用域。
struct Test<'a> {
    key: &'a str,
    value: &'a str,
}

/// 解析并求值条件；`Err` 是摆给作者看的那一句。
fn condition(block: &Block) -> Result<bool, String> {
    let mut test: Option<Test<'_>> = None;
    let mut not = false;
    let mut value_param: Option<&str> = None;

    for (key, value) in &block.params {
        match key.as_str() {
            "condition-not" => not = fill::flag(block, "condition-not"),
            "value" => value_param = Some(value),
            key if key.starts_with("condition-") => {
                if test.is_some() {
                    return Err(
                        "`::if` 一次只能写一个 `condition-*`；要同时满足多个条件，请嵌套 `::if`。"
                            .to_string(),
                    );
                }
                test = Some(Test {
                    key,
                    value: value.as_str(),
                });
            }
            _ => {
                return Err(format!(
                    "`::if` 不认参数 `{key}`；条件写成 `condition-exist={{@body}}` 这样。"
                ));
            }
        }
    }

    let Some(test) = test else {
        return Err(
            "`::if` 缺少条件：写 `condition-exist={{@body}}` 之类的 `condition-*` 参数。"
                .to_string(),
        );
    };

    let equals = test.key == "condition-equals";
    if equals {
        if value_param.is_none() {
            return Err("`condition-equals` 还要一个 `value=` 作为比较对象。".to_string());
        }
    } else if value_param.is_some() {
        return Err(format!(
            "`value=` 只跟 `condition-equals` 一起用；`{}` 不需要它。",
            test.key
        ));
    }

    let result = match test.key {
        "condition-exist" => resolve(test.value)?.is_some_and(|value| !value.trim().is_empty()),
        "condition-defined" => resolve(test.value)?.is_some(),
        "condition-empty" => resolve(test.value)?.is_none_or(|value| value.trim().is_empty()),
        "condition-equals" => {
            let left = resolve(test.value)?;
            let right = resolve(value_param.unwrap_or_default())?;
            left.zip(right)
                .is_some_and(|(left, right)| left.trim() == right.trim())
        }
        "condition-truthy" => resolve(test.value)?.is_some_and(|value| truthy(&value)),
        "condition-yesno" => resolve(test.value)?.is_some_and(|value| yesno(&value)),
        other => {
            return Err(format!(
                "`::if` 不认条件 `{other}`；可写 condition-exist / condition-defined / \
                 condition-empty / condition-equals / condition-truthy / condition-yesno。"
            ));
        }
    };

    Ok(if not { !result } else { result })
}

/// 取一个条件值。
///
/// - 整个值就是一个 `{{@名字}}`：返回 `Some`/`None`，于是「不存在」与「存在但为空」
///   分得开；
/// - 普通字面量：原样返回；
/// - 混合文本：把引用换成值，缺变量时返回 `None`（按条件失败处理）。
fn resolve(raw: &str) -> Result<Option<String>, String> {
    let trimmed = raw.trim();
    if let Some(name) = single_ref(trimmed) {
        if context::template_vars().is_none() {
            return Err(format!(
                "`{{{{{name}}}}}` 只能在模板页里用：这里没有模板作用域。"
            ));
        }
        // `{{@名字}}`：存在但为空也要返回 Some("")
        return Ok(context::template_var(&name));
    }
    if trimmed.contains("{{") {
        return Err("`::if` 的条件里请写 `{{@名字}}`（晚绑定），不要写 `{{名字}}`。".to_string());
    }
    Ok(Some(raw.to_string()))
}

/// `{{@name}}` 恰好是整个字符串时，返回 `name`。
fn single_ref(value: &str) -> Option<String> {
    let inner = value.strip_prefix("{{@")?.strip_suffix("}}")?;
    let name = inner.trim();
    if name.is_empty() || name.contains(['{', '}']) {
        return None;
    }
    Some(name.to_string())
}

/// 与 `::js` / `::once` 那套布尔写法保持一致的宽松解释：
/// 空、0、false、no、off 为假，其余为真。
fn truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

/// `condition-yesno`：只把 `yes` / `on` / `true` / `1` 当作真，其余都是假。
fn yesno(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "yes" | "on" | "true" | "1"
    )
}

/// 校验 `[then]` / `[else]`。
fn check_sections(sections: &[(&str, &Node)]) -> Result<(), String> {
    if sections.is_empty() {
        return Err("`::if` 是空的：至少要写 `[then]` 一节。".to_string());
    }
    no_leading_content(sections, "条件块")?;
    let mut then = false;
    let mut otherwise = false;
    for (label, _) in sections {
        match *label {
            "then" if !then => then = true,
            "then" => return Err("`::if` 里的 `[then]` 写了两次。".to_string()),
            "else" if !otherwise => otherwise = true,
            "else" => return Err("`::if` 里的 `[else]` 写了两次。".to_string()),
            other => {
                return Err(format!(
                    "`::if` 不认分节 `[{other}]`；只写 `[then]` 或 `[else]`。"
                ));
            }
        }
    }
    if !then {
        return Err("`::if` 缺少 `[then]` 一节。".to_string());
    }
    Ok(())
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

    fn render(source: &str) -> String {
        Mdpp::new().render(source)
    }

    /// 字面量条件：`[then]` / `[else]` 各走一次
    #[test]
    fn a_literal_condition_picks_a_branch() {
        let yes = render("::if condition-exist=是\n  [then]\n    甲\n  [else]\n    乙\n");
        assert!(yes.contains("甲"), "{yes}");
        assert!(!yes.contains("乙"), "{yes}");

        let no = render("::if condition-exist=\n  [then]\n    甲\n  [else]\n    乙\n");
        assert!(!no.contains("甲"), "{no}");
        assert!(no.contains("乙"), "{no}");
    }

    /// 模板里的晚绑定条件：`{{@body}}` 在渲染时才取，空格/换行都不怕
    #[test]
    fn a_late_bound_condition_sees_the_template_body() {
        let mut pages = Pages::default();
        pages.0.insert(
            "卡片".into(),
            TemplatePage::Ready(
                "::if condition-exist= {{@body}}\n  [then]\n    有\n  [else]\n    无\n".into(),
            ),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));

        let yes = Mdpp::new().render_with("::卡片\n  你好 世界\n", &context);
        assert!(yes.contains("有"), "{yes}");
        assert!(!yes.contains("无"), "{yes}");

        let no = Mdpp::new().render_with("::卡片\n", &context);
        assert!(no.contains("无"), "{no}");
    }

    /// 存在但为空、根本不存在、裸 flag：`exist` / `defined` / `empty` 分得开
    #[test]
    fn presence_and_emptiness_are_different() {
        let mut pages = Pages::default();
        pages.0.insert(
            "检查".into(),
            TemplatePage::Ready(
                "::if condition-defined={{@flag}}\n  [then]\n    定义\n  [else]\n    缺失\n".into(),
            ),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));

        let bare = Mdpp::new().render_with("::检查 flag\n", &context);
        assert!(bare.contains("定义"), "{bare}");

        let absent = Mdpp::new().render_with("::检查\n", &context);
        assert!(absent.contains("缺失"), "{absent}");
    }

    /// `condition-yesno` 只认 yes / on / true / 1
    #[test]
    fn yesno_accepts_only_the_yes_words() {
        for value in ["yes", "on", "true", "1", "YES", "On", "TRUE"] {
            let html = render(&format!(
                "::if condition-yesno={value}\n  [then]\n    真\n  [else]\n    假\n"
            ));
            assert!(html.contains("真"), "{value:?}：{html}");
        }
        for value in ["no", "off", "false", "0", "", "随便"] {
            let html = render(&format!(
                "::if condition-yesno={value}\n  [then]\n    真\n  [else]\n    假\n"
            ));
            assert!(html.contains("假"), "{value:?}：{html}");
        }
    }

    /// 晚绑定的 equals / truthy：值从模板参数来，而不是早绑定文本插值
    #[test]
    fn late_bound_values_can_be_compared_and_tested() {
        let mut pages = Pages::default();
        pages.0.insert(
            "检查".into(),
            TemplatePage::Ready(
                "::if condition-equals={{@tone}} value=warning\n  [then]\n警告\n  [else]\n其他\n"
                    .into(),
            ),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));

        let warning = Mdpp::new().render_with("::检查 tone=warning\n", &context);
        assert!(warning.contains("警告"), "{warning}");

        let info = Mdpp::new().render_with("::检查 tone=info\n", &context);
        assert!(info.contains("其他"), "{info}");
    }

    /// `condition-equals` 与 `condition-not`
    #[test]
    fn equality_and_negation() {
        let yes = render("::if condition-equals=warning value=warning\n  [then]\n    命中\n");
        assert!(yes.contains("命中"), "{yes}");

        let no = render(
            "::if condition-equals=warning value=info condition-not\n  [then]\n    没命中\n",
        );
        assert!(no.contains("没命中"), "{no}");
    }

    /// 缺 `[then]`、未知标签、重复标签都报用法问题
    #[test]
    fn bad_sections_are_reported() {
        for source in [
            "::if condition-exist=是\n  [else]\n    乙\n",
            "::if condition-exist=是\n  [then]\n    甲\n  [other]\n    乙\n",
            "::if condition-exist=是\n  [then]\n    甲\n  [then]\n    乙\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{html}");
        }
    }

    /// 嵌套 `::if`：外层走 `[then]`，内层再选自己的分支
    #[test]
    fn if_blocks_nest() {
        let html = render(
            "::if condition-exist=yes\n  [then]\n    ::if condition-exist=\n      [then]\n        内甲\n      [else]\n        内乙\n",
        );
        assert!(html.contains("内乙"), "{html}");
        assert!(!html.contains("内甲"), "{html}");
    }

    /// 用户正文里的一行 `[else]` 不该把外层 `::if` 劈开。
    ///
    /// `{{body}}` 要缩进在分支内容里；那样替换进来的每一行都带着缩进，
    /// `[else]` 不再是行首标签。
    #[test]
    fn a_body_that_looks_like_a_label_does_not_split_the_if() {
        let mut pages = Pages::default();
        pages.0.insert(
            "卡片".into(),
            TemplatePage::Ready(
                "::if condition-exist={{@body}}\n  [then]\n    {{body}}\n  [else]\n    空\n".into(),
            ),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));
        let html = Mdpp::new().render_with("::卡片\n  [else]\n  用户内容\n", &context);
        assert!(html.contains("用户内容"), "{html}");
        assert!(!html.contains("空"), "外层 else 不该被选中：{html}");
    }

    /// `::if` 也走模板调用的严格头检查：等号一边贴一边空、引号没闭合都报
    #[test]
    fn if_headers_are_checked_strictly() {
        for source in [
            "::if condition-exist=\"没关上\n  [then]\n    甲\n",
            "::if condition-exist=是 condition-empty=否\n  [then]\n    甲\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{source:?}：{html}");
        }
    }

    /// 条件参数自己也要报错：缺条件、多个条件、equals 缺 value、未知参数
    #[test]
    fn bad_conditions_are_reported() {
        for source in [
            "::if\n  [then]\n    甲\n",
            "::if condition-exist=是 condition-empty=否\n  [then]\n    甲\n",
            "::if condition-equals=是\n  [then]\n    甲\n",
            "::if foo=bar\n  [then]\n    甲\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{html}");
        }
    }
}
