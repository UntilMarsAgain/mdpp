use crate::project::config::NavEntry;
use crate::project::site::{Site, href, normalize};
use crate::render::page;
use crate::render::page::NavLink;

/// 解析后的一项导航。`target` 为空表示它只是分组；有 `children` 时渲染成
/// 下拉菜单，父项自己的页面入口由渲染层自动补的「概述」子项承担。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedNav {
    pub label: String,
    pub target: Option<String>,
    pub overview: Option<String>,
    pub children: Vec<ResolvedNav>,
}

/// 导航栏解析：配置里那几项 + 自动补上的目录按钮，对着站点查好**相对这一页的地址**。
///
/// 导航是「站点级」的（各页都同一条），只有「哪一项是当前页」随页变 —— 所以这里先
/// 解析出一份「键 → 目标页键」的树，等到渲染某一页时再配上相对地址与当前项
/// （见 [`nav_for`]）。
///
/// 解析不出来的**报错**而不是当红链报：导航是作者手写的配置，写错了该当场知道，
/// 不该让整站的导航悄悄少一项。
pub(super) fn resolve_nav(
    configured: &[NavEntry],
    toc_key: &str,
    site: &Site,
) -> Result<Vec<ResolvedNav>, String> {
    let mut out: Vec<ResolvedNav> = Vec::new();
    for entry in configured {
        out.push(resolve_entry(entry, site)?);
    }

    // 目录页：生成的那一页自动在导航末尾补一个按钮，除非作者已经在任何层级指过它
    if !toc_key.is_empty() && !contains_target(&out, toc_key) {
        out.push(ResolvedNav {
            label: toc_key.to_string(),
            target: Some(toc_key.to_string()),
            overview: None,
            children: Vec::new(),
        });
    }

    Ok(out)
}

fn resolve_entry(entry: &NavEntry, site: &Site) -> Result<ResolvedNav, String> {
    match entry {
        NavEntry::SameName(label) => {
            // 字符串：显示文字就是「页名」，按 [[名字]] 那套规则找
            let target = resolve_target(label, site).ok_or_else(|| {
                format!(
                    "nav 里的「{label}」站内没有这样的页 —— 写 `{{ \"{label}\" = \"页的键\" }}` 那样指定"
                )
            })?;
            Ok(ResolvedNav {
                label: label.trim().to_string(),
                target: Some(target),
                overview: None,
                children: Vec::new(),
            })
        }
        NavEntry::Named(map) => {
            let (label, key) = map.iter().next().expect("check 里验过只有一项");
            let target = resolve_key(label, key, site)?;
            Ok(ResolvedNav {
                label: label.trim().to_string(),
                target: Some(target),
                overview: None,
                children: Vec::new(),
            })
        }
        NavEntry::Node(node) => {
            let mut children = Vec::new();
            for child in &node.children {
                children.push(resolve_entry(child, site)?);
            }
            let target = match node.page.as_deref().map(str::trim) {
                Some("") => {
                    return Err(format!("nav 里的 {:?} 的 page 是空的", node.label));
                }
                Some(key) => Some(resolve_key(&node.label, key, site)?),
                None => None,
            };
            if target.is_none() && children.is_empty() {
                return Err(format!(
                    "nav 里的 {:?} 既没有 page 也没有 children —— 它不会指向任何页",
                    node.label
                ));
            }
            let overview = node
                .overview
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string);
            Ok(ResolvedNav {
                label: node.label.trim().to_string(),
                target,
                overview,
                children,
            })
        }
    }
}

/// 把配置里的页键解析成站点里真实存在的页键。错误里带上显示文字，方便定位。
fn resolve_key(label: &str, key: &str, site: &Site) -> Result<String, String> {
    let target =
        normalize(key).ok_or_else(|| format!("nav 里 {label:?} → {key:?} 不是个站内页的键"))?;
    crate::project::site::check_key(&target)?;
    let page = site
        .by_key(&target)
        .ok_or_else(|| format!("nav 里 {label:?} → {key:?} 站内没有这样的页"))?;
    Ok(page.key.clone())
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

/// 目录页是否已经在树的任意层级出现，避免自动目录按钮重复。
fn contains_target(list: &[ResolvedNav], key: &str) -> bool {
    list.iter()
        .any(|item| item.target.as_deref() == Some(key) || contains_target(&item.children, key))
}

/// 一页的导航：把「站点级」的那棵导航树配上相对地址与当前项。
/// `from` 是当前页的键（算相对地址用）。
///
/// 有子项的父项渲染成下拉菜单的标题，本身不落链接；如果它同时有 `target`，
/// 就在子项开头补一个「概述」（可用 `overview` 改名），否则父页会从导航里消失。
pub(super) fn nav_for(list: &[ResolvedNav], from: &str, home: &str) -> Vec<NavLink> {
    list.iter().map(|item| nav_link(item, from, home)).collect()
}

fn nav_link(item: &ResolvedNav, from: &str, home: &str) -> NavLink {
    let children: Vec<NavLink> = item
        .children
        .iter()
        .map(|child| nav_link(child, from, home))
        .collect();
    let active = item.target.as_deref() == Some(from);

    if children.is_empty() {
        return NavLink {
            label: item.label.clone(),
            href: item
                .target
                .as_ref()
                .map(|target| href(from, target, home))
                .unwrap_or_default(),
            active,
            children,
        };
    }

    let mut children = children;
    if let Some(target) = &item.target {
        let target_href = href(from, target, home);
        if !contains_href(&children, &target_href) {
            let label = item
                .overview
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or("概述")
                .to_string();
            children.insert(
                0,
                NavLink {
                    label,
                    href: target_href,
                    active,
                    children: Vec::new(),
                },
            );
        }
    }

    NavLink {
        label: item.label.clone(),
        href: String::new(),
        active: false,
        children,
    }
}

fn contains_href(list: &[NavLink], href: &str) -> bool {
    list.iter()
        .any(|link| link.href == href || contains_href(&link.children, href))
}

/// 目录页的正文：全站每一页一个链接，配着它的键当说明。
/// 首页显示站点名（没有站点名就叫「首页」），别的页显示页名
pub(super) fn toc_body(site: &Site, site_name: &str, toc_key: &str) -> String {
    let mut items = String::new();
    for page in site.pages() {
        let label = if page.key == site.home() {
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
            page::escape(&href(toc_key, &page.key, site.home())),
            page::escape(&label),
            page::escape(&page.key),
        ));
    }
    format!("<ul class=\"mdpp-toc-page\">\n{items}</ul>")
}
