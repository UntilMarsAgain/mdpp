use std::path::Path;

use crate::project::{self, Project, ensure_dir, shown, under};
use crate::render::page;

pub(super) fn favicon(project: &Project, site_dir: &Path) -> Result<Option<String>, String> {
    copy_asset_into(
        project,
        site_dir,
        project.config.favicon.as_deref(),
        "favicon",
    )
}

/// 顶栏里、站点名旁边那个图标：与 [`favicon`] 同一条路 —— 同一个配置形状、同样拷进
/// `assets/`、同样给出「相对站点根」的那一份地址。
pub(super) fn logo(project: &Project, site_dir: &Path) -> Result<Option<String>, String> {
    copy_asset_into(project, site_dir, project.config.logo.as_deref(), "logo")
}

/// 把配置里指的一个文件拷进产物的 `assets/`，给出它的**文件名**（相对站点根）。
///
/// `favicon` 与 `logo` 走的是同一条路：读进来、按字节拷进 `assets/`、只把文件名
/// 交出去（页面按各自深度加前缀，见 [`assets_href`]）。两份各写一份的话，
/// 「读不到就报错」与「按字节拷而不是按文本读」都要各写一次 —— 而后者正是当初
/// favicon 只认 `.svg`、`.png` 直接失败的那个坑。
///
/// **按字节拷**：这两个字段指的都是图片，`.png` / `.ico` / `.gif` 是二进制，
/// `read_to_string` 读它们会因「不是合法 UTF-8」失败，于是最常见的用法反而用不了。
pub(super) fn copy_asset_into(
    project: &Project,
    site_dir: &Path,
    configured: Option<&std::path::Path>,
    what: &str,
) -> Result<Option<String>, String> {
    let Some(configured) = configured else {
        return Ok(None);
    };
    let from = under(&project.root, configured);
    let name = configured
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{what} 不是个文件名字：{}", shown(configured)))?;
    let to = site_dir.join(page::ASSET_DIR).join(&name);
    if let Some(parent) = to.parent() {
        ensure_dir(parent)?;
    }
    std::fs::copy(&from, &to)
        .map_err(|error| format!("读不了 {what} {}：{error}", shown(&from)))?;
    // 只给**文件名**：配置里写的是项目里的相对路径，而产物里它在 `assets/` 下
    Ok(Some(crate::support::url::encode_href(&name)))
}

/// 顶栏与底栏那几句话 —— 配置里那几个单行字段，原样递过去。
/// `nav` 是这一页要用的那一份（每页里「哪一项是当前页」不一样）；
/// `logo` 是站点名旁边那个图标的地址（**已带好这一页的深度前缀**）
///
/// `footer` 会用 `parser` 做一遍 mdpp 解析，于是 `footer` 里能写 `::note`、
/// `[[内部链接]]`、`**粗体**` 之类的 mdpp 语法；解析后的 HTML 存进
/// `copyright_html`，原始文本仍留在 `copyright` 里作为兜底。
pub(super) fn copy_override(
    override_dir: &Path,
    root: &Path,
    site_dir: &Path,
) -> Result<(), String> {
    let from = project::under(root, override_dir);
    if !from.exists() {
        return Ok(());
    }
    copy_dir_all(&from, site_dir).map_err(|error| format!("拷不了 override：{error}"))
}

/// 递归把一个目录里的所有文件拷到另一个目录里（保留目录结构，重复就覆盖）
pub(super) fn copy_dir_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if !to.exists() {
        std::fs::create_dir_all(to)?;
    }
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        // 覆盖层也不跟符号链接：产物根是 HTTP 服务器直接服务的目录，
        // 一条指向外面的链接等于把外面挂进站点。
        let meta = std::fs::symlink_metadata(&src)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            copy_dir_all(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}
