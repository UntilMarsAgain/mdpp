//! 模板页：**不在注册表里的块名，去哪里找正文**。
//!
//! ## 为什么它是一个 trait 而不是一张表
//!
//! 模板页的正文明明是「某个地方的一页 markdown」，但**那个地方是什么**本库不能猜：
//! 是一堆目录里的 `.md` 文件？是数据库的一行？是加密存的、渲染前才解出来的？
//!
//! 所以这里只给一个问法（[`TemplateSource::page`]）与两种答案（[`TemplatePage`]）。
//! 使用方自己实现，把 [`TemplatePages`] 递进 [`RenderContext`](crate::RenderContext) 即可。
//!
//! ## 名字怎么归一
//!
//! [`page_key`] 管这一件事：`甲`、`模板:甲`、`template:甲`（大小写与首尾空白都宽松）
//! 都指同一页。归一是**库**做的，所以实现方不必在 `page` 里自己再处理一遍 ——
//! 而漏掉这一步的症状很隐蔽（`::甲` 能用、`::template:甲` 却报未知块）。
//!
//! [模板命名空间]: https://zh.wikipedia.org/wiki/Template:

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
pub trait TemplateSource: Send + Sync {
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

/// 一个用 `HashMap` 装好的模板页表 —— 命令行、测试、小型使用方直接用它。
///
/// ```no_run
/// use std::collections::HashMap;
/// use std::sync::Arc;
/// use mdpp::block::template::{MapPages, TemplatePage, TemplatePages};
///
/// let mut pages = HashMap::new();
/// pages.insert("卡片".to_string(), TemplatePage::Ready("::banner\n  {{标题}}\n".into()));
/// let source: TemplatePages = Arc::new(MapPages(pages));
/// # let _ = source;
/// ```
#[derive(Debug, Clone, Default)]
pub struct MapPages(pub std::collections::HashMap<String, TemplatePage>);

impl TemplateSource for MapPages {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        // 查两遍：一遍按调用方给的名字，一遍按归一后的键 ——
        // 这样「`::卡片`」与「`::template:卡片`」指向同一页，而**存表时**用哪个写法都行。
        self.0
            .get(name)
            .or_else(|| self.0.get(&name.to_lowercase()))
            .cloned()
    }
}

/// 块名的下一站：模板页。扫描器在注册表里没查到时来这里。
///
/// 顺着写下来是：`::卡片` → [`Block::name`] 是 `卡片` → [`page_key`] 归一 → 问来源。
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