//! 递进渲染上下文的那两样东西：`[[目标]]` 的地址、模板页的正文。
//!
//! 它们是这条命令行**仅有的**两处「程序之外的知识」—— 除了这两样，
//! 这条命令行对文件系统一无所知（读文件是为了拿输入，不是为了查东西）。

use std::collections::HashMap;
use std::path::Path;

use mdpp::block::template::{MapPages, TemplatePage, TemplatePages};
use mdpp::link::{LinkResolver, LinkTarget, ResolvedLink};

/// 把 `[[目标]]` 配上「前缀 + 目标」的 `href`。
///
/// 命令行没有仓库，所以目标存不存在**一律当成存在** —— 它只是个拼地址的办法，
/// 判断「在不在」是使用方的事。真的要看存在与否，实现方递自己的解析器即可
/// （见 `mdpp::LinkResolver`）。
struct Prefix {
    prefix: String,
}

impl LinkResolver for Prefix {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        Some(ResolvedLink::Link(
            LinkTarget::new(format!("{}{}", self.prefix, target)).attr("data-doc-target", target),
        ))
    }
}

/// 有前缀就递一个解析器，没有就不递 ——
/// 不递的那些 `[[…]]` 落成不带 `href` 的 `<a class="wikilink" data-doc=…>`，
/// 至少把原文留着。
pub fn link_resolver(prefix: Option<&str>) -> Option<std::sync::Arc<dyn LinkResolver>> {
    let prefix = prefix?;
    Some(std::sync::Arc::new(Prefix {
        prefix: prefix.to_string(),
    }))
}

/// 把一个目录读成模板页表：`<目录>/<名字>.md` 就是那一页。
///
/// 返回 `Err` 的那一句是给人看的（哪个文件读不了），因为**读了一半的模板页表**
/// 只会让 `::那个名字` 报「未知块」，而真正的毛病在文件系统那边。
pub fn template_pages(dir: &Path) -> Result<TemplatePages, String> {
    let entries = std::fs::read_dir(dir).map_err(|error| {
        format!("读不了模板页目录 {}：{error}", dir.display())
    })?;

    let mut pages: HashMap<String, TemplatePage> = HashMap::new();
    // 目录里有什么顺序是不定的，排序一下 —— 输出稳定了，diff 才说得上话
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();

    for path in paths {
        let Some(stem) = path.file_stem().map(|name| name.to_string_lossy().into_owned()) else {
            continue;
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("读不了模板页 {}：{error}", path.display()))?;
        // 键统一小写：归一是库做的（`page_key`），而表里存成什么样它都能认 ——
        // 存一份小写的，两个入口（`::卡片` 与 `::template:卡片`）就都落到同一页
        pages.insert(stem.to_lowercase(), TemplatePage::Ready(text));
    }

    // 这一条发 `info`：装上订阅器的人（`--debug`）正好想知道「模板页到底进了几页」
    tracing::info!(dir = %dir.display(), pages = pages.len(), "模板页目录读完了");

    Ok(std::sync::Arc::new(MapPages(pages)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdpp::{Mdpp, RenderContext};

    #[test]
    fn a_prefix_turns_a_wikilink_into_an_href() {
        let context = RenderContext::new().with_links(link_resolver(Some("/notes/")).unwrap());
        let html = Mdpp::new().render_with("[[甲]]\n", &context);
        assert!(html.contains(r#"href="/notes/甲""#), "{html}");
        assert!(html.contains(r#"data-doc="甲""#), "{html}");
    }

    /// 没有前缀就不递解析器 —— 那些 `[[…]]` 落成不带 href 的写法
    #[test]
    fn without_a_prefix_there_is_no_href() {
        assert!(link_resolver(None).is_none());
        let html = Mdpp::new().render("[[甲]]\n");
        assert!(!html.contains("href="), "{html}");
        assert!(html.contains("wikilink"), "{html}");
    }

    #[test]
    fn a_directory_becomes_a_page_table() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("卡片.md"), "::banner\n  卡片来了\n").expect("写得下");
        std::fs::write(dir.path().join("不是模板.txt"), "不认").expect("写得下");

        let pages = template_pages(dir.path()).expect("读得动");
        let context = RenderContext::new().with_templates(pages);
        let html = Mdpp::new().render_with("::卡片\n", &context);
        assert!(html.contains("banner"), "{html}");
        assert!(html.contains("卡片来了"), "{html}");
    }

    /// `src=` 后面带不带前缀都认 —— 归一是库做的（`page_key`）
    #[test]
    fn the_prefixed_spelling_finds_the_same_page() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        std::fs::write(dir.path().join("卡片.md"), "::banner\n  卡片来了\n").expect("写得下");

        let context = RenderContext::new().with_templates(template_pages(dir.path()).expect("读得动"));
        // 注意第三个要加引号：不加的话头解析会在空白处断开，变成两个参数 ——
        // 那不是归一失灵，是词法如此
        for src in ["卡片", "template:卡片", r#""Template:  卡片  ""#] {
            let html = Mdpp::new().render_with(&format!("::css src={src}\n  .a{{}}\n"), &context);
            assert!(html.contains("卡片来了"), "src={src} 没找到那一页：{html}");
        }
    }

    /// 那一页读不出来要说清原因，并给出一条指向它的链接（能做的下一步）
    #[test]
    fn a_page_that_cannot_be_read_says_so_and_points_at_itself() {
        #[derive(Debug)]
        struct Locked;
        impl mdpp::TemplateSource for Locked {
            fn page(&self, name: &str) -> Option<mdpp::TemplatePage> {
                (name == "卡片").then(|| mdpp::TemplatePage::Unreadable {
                    reason: "需要口令".to_string(),
                })
            }
        }
        let context =
            RenderContext::new().with_templates(std::sync::Arc::new(Locked));
        let html = Mdpp::new().render_with("::卡片\n", &context);
        assert!(html.contains("需要口令"), "{html}");
        assert!(!html.contains("template--unknown"), "{html}");
        assert!(html.contains("wikilink"), "{html}");
    }

    /// 读不了目录就说清是哪个目录 —— 报「未知块」的话作者会去找块名
    #[test]
    fn an_unreadable_directory_says_which_one() {
        let message = template_pages(Path::new("/没有这个目录")).unwrap_err();
        assert!(message.contains("模板页目录"), "{message}");
    }

    /// 空目录不报错 —— 那只是「还没有自定义块」，与「出错」是两件事
    #[test]
    fn an_empty_directory_is_fine() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        assert!(template_pages(dir.path()).is_ok());
    }
}