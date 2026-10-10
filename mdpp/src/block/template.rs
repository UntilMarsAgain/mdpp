//! 模板页：**不在注册表里的块名，去哪里找正文**。
//!
//! ## 这个模块只给接口，不给实现
//!
//! 「在哪里找那一页」是**文档之外**的事 —— 是一堆目录里的 `.md` 文件？是数据库的
//! 一行？是搜索索引？是加密存的、渲染前才解出来的？本库不猜，也不提供任何一种猜法。
//!
//! 所以这里只有三个东西：问法（[`TemplateSource::page`]）、答案（[`TemplatePage`]）、
//! 以及递进上下文的那个容器类型（[`TemplatePages`]）。**查表的活儿在外面** ——
//! 使用方自己写那几行。命令行里那个「读一个目录成一张表」的就是最简的一版
//! （见 `mdpp-cli/src/context.rs`），而它也只有三十行。
//!
//! 库**曾经**在这里带过一个 `MapPages`（`HashMap` 版），拿掉的理由是它不只是省事，
//! 还会**规定形状**：它暗示「模板页就是一整张表、渲染前全在内存里」。真实使用方
//! 常常不是这样 —— 按需查一库、或从索引里捞 —— 而要绕开那个类型得重新实现一遍
//! trait。留一个「最常见的那种」进库，等于给这件事定了个默认答案。
//!
//! ## 名字怎么归一
//!
//! [`page_key`] 管这一件事：`甲`、`模板:甲`、`template:甲`（大小写与首尾空白都宽松）
//! 都指同一页。归一是**库**做的 —— 它属于「这个名字怎么写」，属于文档解析 ——
//! 所以实现方不必在 `page` 里自己再处理一遍，而漏掉这一步的症状很隐蔽
//! （`::甲` 能用、`::template:甲` 却报未知块）。
//!
//! [`page_key`] 传来的 `name` 已经归一过（**小写**、剥掉了前缀），所以最简单的
//! 实现就是「拿它当键查一下」。

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
    /// 职责），所以只需要知道**为什么**、好把话说清楚。缺了原因的话，
    /// 「读不出来」会退化成「未知块」—— 而块名是对的，作者会查错方向。
    Unreadable {
        /// 为什么读不出来（原文，给人看）
        reason: String,
    },
}

/// 模板页的来源。
///
/// 要 `Debug` 不是为了好看：实现方那几样（目录、数据库连接）多半已经 `Debug`，
/// 而没有它的话调用方连「这一份上下文里带了什么」都打印不出来 —— 一份渲染上下文
/// 打印不出来，排问题时就少了一双手。
///
/// [`crate::link::LinkResolver`] 上也有同一条约束，理由一样。
///
/// ## 实现有多短
///
/// 十行，没有依赖，也没有任何关于「模板页存在哪里」的假设：
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_and_the_plain_name_are_the_same_key() {
        assert_eq!(page_key("甲").as_deref(), Some("甲"));
        assert_eq!(page_key("template:甲").as_deref(), Some("甲"));
        assert_eq!(page_key("Template:甲").as_deref(), Some("甲"));
        // 大小写与首尾空白都宽松
        assert_eq!(page_key("  甲  ").as_deref(), Some("甲"));
        assert_eq!(page_key("甲甲甲").as_deref(), Some("甲甲甲"));
    }

    #[test]
    fn a_name_with_nothing_left_after_the_prefix_is_not_a_page() {
        assert_eq!(page_key(""), None);
        assert_eq!(page_key("   "), None);
        assert_eq!(page_key("template:"), None);
        assert_eq!(page_key("template:   "), None);
    }

    /// **一个从零写的实现够不够用** —— 这条钉的是「本模块不给实现」这个决定。
    ///
    /// 如果哪天真在库里加了什么「更方便」的模板页来源，这条会提醒你重新想一遍
    /// 那是不是又把「存在哪」这件事定进了库里。
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

    #[test]
    fn a_prefix_that_is_not_the_template_one_is_kept() {
        // `帮助:入门` 是一页的名字，不是「入门页在 template 里」
        assert_eq!(page_key("帮助:入门").as_deref(), Some("帮助:入门"));
    }

    #[test]
    fn the_display_name_is_always_prefixed() {
        assert_eq!(display_name("卡片"), "template:卡片");
    }
}
