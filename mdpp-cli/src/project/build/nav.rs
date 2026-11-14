use crate::project::config::NavEntry;
use crate::project::site::{INDEX_KEY, Site, href, normalize};
use crate::render::page;
use crate::render::page::NavLink;

pub(super) fn resolve_nav(
    configured: &[NavEntry],
    toc_key: &str,
    site: &Site,
) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();

    for entry in configured {
        let (label, target) = match entry {
            NavEntry::SameName(label) => {
                // 字符串：显示文字就是「页名」，按 [[名字]] 那套规则找
                let key = resolve_target(label, site).ok_or_else(|| {
                    format!(
                        "nav 里的「{label}」站内没有这样的页 —— 写 `{{ \"{label}\" = \"页的键\" }}` 那样指定"
                    )
                })?;
                (label.clone(), key)
            }
            NavEntry::Named(map) => {
                let (label, key) = map.iter().next().expect("check 里验过只有一项");
                let target = normalize(key)
                    .ok_or_else(|| format!("nav 里 {label:?} → {key:?} 不是个站内页的键"))?;
                crate::project::site::check_key(&target)?;
                let page = site
                    .by_key(&target)
                    .ok_or_else(|| format!("nav 里 {label:?} → {key:?} 站内没有这样的页"))?;
                (label.clone(), page.key.clone())
            }
        };
        out.push((label.trim().to_string(), target));
    }

    // 目录页：生成的那一页自动在导航末尾补一个按钮，除非作者已经指过它
    if !toc_key.is_empty() && !out.iter().any(|(_, target)| target == toc_key) {
        out.push((toc_key.to_string(), toc_key.to_string()));
    }

    Ok(out)
}

/// 按 [[名字]] 那套规则找一个页：键名、或全站唯一叫这名字的页。
/// 认不出就是 `None`（报错的活交给 [`resolve_nav`]）
pub(super) fn resolve_target(label: &str, site: &Site) -> Option<String> {
    let key = normalize(label)?;
    if let Some(page) = site.by_key(&key) {
        return Some(page.key.clone());
    }
    let candidates = site.by_stem(&key);
    match candidates.as_slice() {
        [page] => Some(page.key.clone()),
        _ => None,
    }
}

/// 一页的导航：把「站点级」的那份清单配上相对地址与当前项。
/// `from` 是当前页的键（算相对地址用）
pub(super) fn nav_for(list: &[(String, String)], from: &str) -> Vec<NavLink> {
    list.iter()
        .map(|(label, target)| NavLink {
            label: label.clone(),
            href: href(from, target),
            active: from == target,
        })
        .collect()
}

/// 目录页的正文：全站每一页一个链接，配着它的键当说明。
/// 首页显示站点名（没有站点名就叫「首页」），别的页显示页名
pub(super) fn toc_body(site: &Site, site_name: &str, toc_key: &str) -> String {
    let mut items = String::new();
    for page in site.pages() {
        let label = if page.key == INDEX_KEY {
            let name = site_name.trim();
            if name.is_empty() {
                "首页".to_string()
            } else {
                name.to_string()
            }
        } else {
            page.stem().to_string()
        };
        items.push_str(&format!(
            "<li><a href=\"{}\">{}</a> <span class=\"mdpp-toc-page__path\">{}</span></li>\n",
            page::escape(&href(toc_key, &page.key)),
            page::escape(&label),
            page::escape(&page.key),
        ));
    }
    format!("<ul class=\"mdpp-toc-page\">\n{items}</ul>")
}
