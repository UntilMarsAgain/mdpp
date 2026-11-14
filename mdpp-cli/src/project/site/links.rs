use std::sync::{Arc, Mutex};

use mdpp::link::{LinkResolver, LinkTarget, ResolvedLink};
use tracing::debug;

use super::paths::{check_key, href, is_template_target, normalize};
use super::{Missing, Page, Site};

/// 站内的链接解析器：**每一页一份**（相对地址要看当前页在哪），
/// 但红链的账记在**全站一份**的表里。
pub struct Links {
    site: Arc<Site>,
    /// 当前页的键（算相对地址用）
    from: String,
    /// 全站的红链表
    missing: Arc<Mutex<Vec<Missing>>>,
}

impl Links {
    /// 给一页配一个解析器。红链记进 `missing`
    pub fn for_page(site: Arc<Site>, page: &Page, missing: Arc<Mutex<Vec<Missing>>>) -> Links {
        debug!(page = %page.key, "这一页的链接解析器");
        Links {
            site,
            from: page.key.clone(),
            missing,
        }
    }

    /// 记一笔红链。**锁不住就当没发生** —— 报告缺一条比 panic 强
    fn note(&self, target: &str, why: String) {
        if let Ok(mut list) = self.missing.lock() {
            let already = list
                .iter()
                .any(|one| one.page == self.from && one.target == target.trim());
            if !already {
                list.push(Missing {
                    page: self.from.clone(),
                    target: target.trim().to_string(),
                    why,
                });
            }
        }
    }
}

impl LinkResolver for Links {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        // 模板页不是站内的一页 —— 它是写模板的文件，不进站点。落成红链是对的，
        // 但那不是「少写了一页」，所以不进报告
        if is_template_target(target) {
            debug!(target, "指向模板页，不是站内页面");
            return None;
        }

        let key = normalize(target)?;
        // 指向 `..` 的链接不是站内页；当红链处理，别让它变成产物里的路径。
        if check_key(&key).is_err() {
            return None;
        }

        if let Some(page) = self.site.by_key(&key) {
            return Some(self.link_to(page));
        }

        let candidates = self.site.by_stem(&key);
        match candidates.as_slice() {
            [page] => Some(self.link_to(page)),
            [] => {
                self.note(target, "站内没有这一页".to_string());
                None
            }
            _ => {
                let keys: Vec<&str> = candidates.iter().map(|page| page.key.as_str()).collect();
                self.note(target, format!("好几页都叫这个名字：{}", keys.join("、")));
                None
            }
        }
    }
}

impl Links {
    /// 落成一个带 `href` 的链接。顺带把**落到哪一页**记在属性上 ——
    /// `[[入门]]` 这种只写文件名的写法，光看正文看不出它指的是哪个目录下的那一页
    fn link_to(&self, page: &Page) -> ResolvedLink {
        ResolvedLink::Link(
            LinkTarget::new(href(&self.from, &page.key)).attr("data-mdpp-page", page.key.clone()),
        )
    }
}
