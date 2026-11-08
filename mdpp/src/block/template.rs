//! 模板页：**不在注册表里的块名，去哪里找正文**。
//!
//! 「在哪里找那一页」是文档之外的事（目录？数据库？搜索索引？），本库不猜，也不提供
//! 任何一种猜法 —— 这里只有问法（[`TemplateSource::page`]）、答案（[`TemplatePage`]）
//! 与容器类型（[`TemplatePages`]）。
//!
//! ## 名字怎么归一
//!
//! [`page_key`]：`甲`、`template:甲`（大小写与首尾空白都宽松）指同一页。传进
//! [`TemplateSource::page`] 的 `name` 已归一过（**小写**、剥掉了前缀），所以最简单的
//! 实现是「拿它当键查一下」。
//!
//! 注意：这里的 `template:` 只是**页面名**的写法（`src=`、诊断链接用），不是块语法；
//! `::template:甲` 在扫描器里仍然是普通段落。块名不能含冒号，命名空间归调用方识别。

use std::collections::HashSet;
use std::sync::Arc;

/// 模板页名的前缀。带不带它都行（见 [`page_key`]）。
pub const TEMPLATE_PREFIX: &str = "template";

/// 一页模板页在**这一刻**的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePage {
    /// 取到了：正文
    Ready(String),
    /// 有这一页，但这一刻读不出来 —— 带着**够说给人听**的原因。
    ///
    /// 为什么只有「原因」而没有别的：本库不为上锁的页出解锁框（那是调用方的界面
    /// 职责）。而缺了原因的话「读不出来」会退化成「未知块」—— 块名是对的，作者会
    /// 查错方向。
    Unreadable {
        /// 为什么读不出来（原文，给人看）
        reason: String,
    },
}

/// 模板页的来源。
///
/// 要求 `Debug`：实现方那几样（目录、数据库连接）多半已经 `Debug`，而没有它的话调用方
/// 连「这一份上下文里带了什么」都打印不出来。
///
/// [`crate::link::LinkResolver`] 上也有同一条约束。
///
/// 实现就是十行，不含任何关于「模板页存在哪里」的假设：
///
/// ```
/// use std::collections::HashMap;
/// use mdpp::block::template::{TemplatePage, TemplateSource};
///
/// #[derive(Debug, Default)]
/// struct Pages(HashMap<String, TemplatePage>);
///
/// impl TemplateSource for Pages {
///     fn page(&self, name: &str) -> Option<TemplatePage> {
///         // name 已经归一过了（小写、剥了前缀），所以直接查
///         self.0.get(name).cloned()
///     }
/// }
/// ```
pub trait TemplateSource: Send + Sync + std::fmt::Debug {
    /// 取一页模板页的正文。没有这一页就返回 `None`。
    ///
    /// `name` 是**已经归一过的**（小写、剥掉了前缀，见 [`page_key`]），实现方不必
    /// 再处理前缀与大小写。
    fn page(&self, name: &str) -> Option<TemplatePage>;
}

/// 递给 [`RenderContext`](crate::RenderContext) 的那份模板页来源。
pub type TemplatePages = Arc<dyn TemplateSource>;

/// 页名的归一写法：`甲` 与 `template:甲` 是同一个键（统一小写、剥前缀）。
///
/// 空串**不算**合法页名 —— 它返回 `None`，那意味着「这一页没有名字」。
pub fn page_key(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let page = match trimmed.split_once(':') {
        Some((prefix, rest)) if prefix.trim().eq_ignore_ascii_case(TEMPLATE_PREFIX) => rest.trim(),
        _ => trimmed,
    };
    if page.is_empty() {
        return None;
    }
    Some(page.to_lowercase())
}

/// `::卡片` / `::template:卡片` 这类写法在展开时显示用的名字。
pub fn display_name(name: &str) -> String {
    format!("{TEMPLATE_PREFIX}:{}", name.trim())
}

/// 块名的下一站：模板页。扫描器在注册表里没查到时来这里。
///
/// 顺着写下来是：`::卡片` → 块名就是 `卡片` → [`page_key`] 归一 → 问来源。
pub fn page_for(name: &str) -> Option<TemplatePage> {
    let key = page_key(name)?;
    crate::context::templates()?.page(&key)
}

/// 展开一层模板时保留的变量：正文、参数与模板名。
///
/// 这是 `{{@名字}}` 的来源。`{{名字}}` 是**早绑定**文本插值，`{{@名字}}` 是
/// **晚绑定**参数引用：前者在模板文本替换时就没了，后者留到参数分词之后再取值，
/// 所以值里有空格、引号、换行也不会破坏块头。
#[derive(Debug, Clone)]
pub(crate) struct TemplateVars {
    pub(crate) name: String,
    pub(crate) body: String,
    pub(crate) params: Vec<(String, String)>,
    pub(crate) parent: Option<Arc<TemplateVars>>,
}

impl TemplateVars {
    pub(crate) fn new(
        name: &str,
        params: Vec<(String, String)>,
        body: String,
        parent: Option<Arc<TemplateVars>>,
    ) -> Self {
        TemplateVars {
            name: name.to_string(),
            body,
            params,
            parent,
        }
    }

    /// 取一个变量；`body` / `template` 是保留名，参数同名时也不行。
    pub(crate) fn lookup(&self, key: &str) -> Option<&str> {
        match key {
            "body" => Some(&self.body),
            "template" => Some(&self.name),
            _ => self
                .params
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
                .or_else(|| self.parent.as_ref().and_then(|parent| parent.lookup(key))),
        }
    }
}

/// 把参数值里的 `{{@名字}}` / `{{?@名字}}` 换掉。**在分词之后做**，所以值里有
/// 空白也不会再被切开。
///
/// 必填引用缺失时返回错误；`?` 开头的可选引用缺失时填空串。插入的值**不再扫一遍**，
/// 与 `{{}}` 的单趟替换一致 —— 否则参数里写 `{{@other}}` 会被误当成引用执行。
pub(crate) fn resolve_refs(params: Vec<(String, String)>) -> Result<Vec<(String, String)>, String> {
    params
        .into_iter()
        .map(|(key, value)| resolve_value(&value).map(|value| (key, value)))
        .collect()
}

fn resolve_value(value: &str) -> Result<String, String> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(format!("参数里的占位符没有闭合：{}", &rest[start..]));
        };
        let raw = after[..end].trim();
        let (raw, optional) = match raw.strip_prefix('?') {
            Some(raw) => (raw.trim(), true),
            None => (raw, false),
        };
        if let Some(name) = raw.strip_prefix('@') {
            let name = name.trim();
            if name.is_empty() {
                return Err("参数引用的变量名不能为空".to_string());
            }
            match crate::context::template_var(name) {
                Some(value) => out.push_str(&value),
                None if optional => {}
                None => return Err(format!("模板变量 `{name}` 不存在")),
            }
        } else {
            // 不是晚绑定引用：保留原样，交给上层/作者自己看
            out.push_str(&rest[start..start + 2 + end + 2]);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// 模板调用头的严格语法检查。**只查写法，不查参数含义**。
///
/// 普通内置块仍走宽松解析；模板调用（以及 `::if`）在解析成功之后再过这里：
///
/// - 等号两侧的空格一律忽略（`key=value` / `key = value` / `key= value` 同义）；
/// - 参数名不能为空、不能含空白/控制字符/`:`/`=`，不能以 `@` 开头；
/// - 同名参数只能出现一次。
///
/// 返回的那句话是给作者看的，调用方放进 `template--problem` 框。
pub(crate) fn check_header(
    name: &str,
    line: &str,
    params: &[(String, String)],
) -> Result<(), String> {
    let Some(tokens) = crate::registry::Block::header_tokens(line) else {
        return Err("模板调用的头里有没闭合的引号。".to_string());
    };
    if tokens.is_empty() {
        return Err("模板调用的名字是空的。".to_string());
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for (key, _) in params {
        if key.is_empty() {
            return Err("模板调用的参数名不能为空。".to_string());
        }
        if key == "=" {
            return Err("模板调用的参数里出现了单独的 `=`；等号两边都要有东西。".to_string());
        }
        if key.starts_with('@') {
            return Err(format!(
                "模板调用的参数名 `{key}` 不能以 `@` 开头：`@` 保留给 `{{@名字}}`。"
            ));
        }
        if key
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || matches!(ch, ':' | '='))
        {
            return Err(format!(
                "模板调用的参数名 `{key}` 不能含空白、控制字符、`:` 或 `=`。"
            ));
        }
        if !seen.insert(key) {
            return Err(format!("模板调用的参数 `{key}` 写了两次。"));
        }
    }

    // `name` 只用于把话说全；名字的合法性由 `parse_header` 保证。
    if name.is_empty() {
        return Err("模板调用的名字是空的。".to_string());
    }
    Ok(())
}

/// 一个**看起来像模板调用、但头解析失败**的行：`(名字, 原因)`。
///
/// 只在名字能捞出来时返回。调用方再查这个名字是不是模板；不是就仍按普通段落走，
/// 不改变 `::` 行在普通正文里的宽松行为。
pub(crate) fn malformed_header(line: &str) -> Option<(String, String)> {
    let name = loose_name(line)?;
    let why = if crate::registry::Block::header_tokens(line).is_none() {
        "模板调用的头里有没闭合的引号（或引号里的转义不完整）。".to_string()
    } else {
        "模板调用的名字无效：名字不能含 `=` 或 `:`。".to_string()
    };
    Some((name, why))
}

/// 从头行里尽量捞出名字：引号没闭合时也尽量取到第一个名字。
fn loose_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("::")?.trim_start();
    let first = rest.chars().next()?;
    if matches!(first, '"' | '\'') {
        let mut name = String::new();
        let mut escaped = false;
        for ch in rest[1..].chars() {
            if escaped {
                name.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == first {
                let name = name.trim();
                return (!name.is_empty()).then(|| name.to_string());
            }
            name.push(ch);
        }
        // 引号一直没闭合：取到第一个空白为止，至少别把整行都当名字
        let end = rest[1..]
            .find(char::is_whitespace)
            .map_or(rest.len(), |at| at + 1);
        let name = rest[1..end].trim();
        return (!name.is_empty()).then(|| name.to_string());
    }
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = rest[..end].trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `甲` 与 `template:甲` 是同一个键；大小写与首尾空白都宽松
    #[test]
    fn the_prefix_and_the_plain_name_are_the_same_key() {
        assert_eq!(page_key("甲").as_deref(), Some("甲"));
        assert_eq!(page_key("template:甲").as_deref(), Some("甲"));
        assert_eq!(page_key("Template:甲").as_deref(), Some("甲"));
        // 大小写与首尾空白都宽松
        assert_eq!(page_key("  甲  ").as_deref(), Some("甲"));
        assert_eq!(page_key("甲甲甲").as_deref(), Some("甲甲甲"));
    }

    /// 前缀后面没东西 = 这一页没有名字，不是「一页叫 `template:` 的页」
    #[test]
    fn a_name_with_nothing_left_after_the_prefix_is_not_a_page() {
        assert_eq!(page_key(""), None);
        assert_eq!(page_key("   "), None);
        assert_eq!(page_key("template:"), None);
        assert_eq!(page_key("template:   "), None);
    }

    /// 从零写一个实现够不够用 —— 够，所以本模块不必给实现。
    #[test]
    fn a_scratch_implementation_is_all_anyone_needs() {
        use std::collections::HashMap;

        #[derive(Debug, Default)]
        struct Pages(HashMap<String, TemplatePage>);

        impl TemplateSource for Pages {
            fn page(&self, name: &str) -> Option<TemplatePage> {
                self.0.get(name).cloned()
            }
        }

        let mut pages = Pages::default();
        pages.0.insert(
            "卡片".into(),
            TemplatePage::Ready("::banner\n  卡片来了\n".into()),
        );

        let context = crate::RenderContext::new().with_templates(Arc::new(pages));
        let html = crate::Mdpp::new().render_with("::卡片\n", &context);
        assert!(html.contains("卡片来了"), "{html}");
    }

    /// 只剥 `template:` 那一个前缀：`帮助:入门` 的前缀是页名的一部分
    #[test]
    fn a_prefix_that_is_not_the_template_one_is_kept() {
        // `帮助:入门` 是一页的名字，不是「入门页在 template 里」
        assert_eq!(page_key("帮助:入门").as_deref(), Some("帮助:入门"));
    }

    /// 展示用的名字一律补上前缀，免得写的人看不出这是一页模板页
    #[test]
    fn the_display_name_is_always_prefixed() {
        assert_eq!(display_name("卡片"), "template:卡片");
    }

    fn checked(line: &str) -> Result<(), String> {
        let (name, params) = crate::registry::Block::parse_header(line).expect("头该解析得动");
        check_header(&name, line, &params)
    }

    /// 等号两侧的空格全部忽略；规范写法与宽松写法都放行
    #[test]
    fn strict_headers_accept_spaces_around_equals() {
        for line in [
            r#"::卡片 title=甲 flag e="" f="" 标题="含 空格" a=1 b=2"#,
            r#"::卡片 title = "含 空格" color = #0af"#,
            r#"::卡片 title= "含 空格" color= #0af"#,
            r#"::卡片 title ="含 空格" color ="红色""#,
            r#"::卡片 condition-exist={{@body}} condition-not"#,
        ] {
            assert!(checked(line).is_ok(), "{line:?}：{:?}", checked(line));
        }
    }

    /// 重复键、空键、`@` 键仍然在这里停下
    #[test]
    fn strict_headers_reject_the_real_misuses() {
        for line in [
            "::卡片 =x",
            "::卡片 title=1 title=2",
            r#"::卡片 "带 空格"=值"#,
            "::卡片 @ref=值",
        ] {
            let error = checked(line).expect_err(line);
            assert!(!error.is_empty(), "{line}");
        }
    }

    /// 头里有没闭合的引号时也要捞得出名字，才能知道该不该报模板错误
    #[test]
    fn a_broken_header_still_yields_a_name() {
        let (name, why) = malformed_header(r#"::卡片 title="没关上"#).expect("捞得出名字");
        assert_eq!(name, "卡片");
        assert!(why.contains("引号"), "{why}");

        let (name, _) = malformed_header(r#"::"两 个词" title=甲"#).expect("引号名字也要认");
        assert_eq!(name, "两 个词");
    }

    /// 晚绑定引用只替换 `{{@...}}`，且插入的值不再扫第二遍
    #[test]
    fn late_refs_are_replaced_once() {
        let vars = Arc::new(TemplateVars::new(
            "卡片",
            vec![("标题".into(), "{{@标题}}".into())],
            "正文".into(),
            None,
        ));
        let _guard = crate::context::enter_template_vars(vars);
        let params = resolve_refs(vec![
            ("a".into(), "{{@标题}}".into()),
            ("b".into(), "前 {{@body}} 后".into()),
        ])
        .expect("都找得到");
        assert_eq!(params[0].1, "{{@标题}}", "插入的值不该再展开一次");
        assert_eq!(params[1].1, "前 正文 后");

        assert!(
            resolve_refs(vec![("a".into(), "{{@没有}}".into())])
                .expect_err("必填变量缺失该报错")
                .contains("没有")
        );
        let optional =
            resolve_refs(vec![("a".into(), "前 {{?@没有}} 后".into())]).expect("可选缺失");
        assert_eq!(optional[0].1, "前  后");
    }
}
