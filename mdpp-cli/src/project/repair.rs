//! `mdpp repair` —— 恢复内建的站点模板与前端资源。
//!
//! 站点把 `template.html` 与 `assets/` 里的三份文件当成可编辑的源码。
//! 改坏了、删了、想回到出厂状态时，用这条命令：
//!
//! - `template.html` 回到项目根；
//! - `assets/mdpp.css`、`assets/site.css`、`assets/mdpp.js` 回到 `assets/`；
//! - 原来存在的文件先改名成 `.bak`（已有 `.bak` 就用 `.bak.1`、`.bak.2`……），
//!   不直接覆盖。
//!
//! 这是恢复内建文件，不是切换到内建模式；内建模式由 `build --builtin` 负责。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tracing::info;

use crate::project::{Project, ensure_dir, shown};
use crate::render::page::{ASSET_DIR, CORE_CSS, SCRIPT_FILE, SITE_CSS, SiteFiles, TEMPLATE_FILE};

pub fn run() -> Result<(), String> {
    let project = Project::open()?;
    let files = SiteFiles::embedded();
    let mut written = 0usize;

    restore(
        &project,
        Path::new(TEMPLATE_FILE),
        "template.html",
        files.template.as_bytes(),
        &mut written,
    )?;
    restore(
        &project,
        &Path::new(ASSET_DIR).join(CORE_CSS),
        "assets/mdpp.css",
        files.core_css.as_bytes(),
        &mut written,
    )?;
    restore(
        &project,
        &Path::new(ASSET_DIR).join(SITE_CSS),
        "assets/site.css",
        files.site_css.as_bytes(),
        &mut written,
    )?;
    restore(
        &project,
        &Path::new(ASSET_DIR).join(SCRIPT_FILE),
        "assets/mdpp.js",
        files.script.as_bytes(),
        &mut written,
    )?;

    info!(
        root = %project.root.display(),
        files = written,
        "站点模板与前端资源已恢复"
    );
    Ok(())
}

fn restore(
    project: &Project,
    relative: &Path,
    what: &str,
    content: &[u8],
    written: &mut usize,
) -> Result<(), String> {
    let path = project.contained(relative, what)?;
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    if path.exists() {
        let backup = backup_path(&path)?;
        std::fs::rename(&path, &backup)
            .map_err(|error| format!("备份不了 {}：{error}", shown(&path)))?;
        info!(from = %path.display(), to = %backup.display(), "已备份旧文件");
    }
    std::fs::write(&path, content).map_err(|error| format!("写不了 {}：{error}", shown(&path)))?;
    *written += 1;
    Ok(())
}

/// `.bak`，已存在就 `.bak.1`、`.bak.2`…… —— 不覆盖上一次的备份。
fn backup_path(path: &Path) -> Result<PathBuf, String> {
    let mut candidate = OsString::from(path.as_os_str());
    candidate.push(".bak");
    let mut candidate = PathBuf::from(candidate);
    let mut suffix = 0usize;
    while candidate.exists() {
        suffix += 1;
        candidate = PathBuf::from(format!("{}.bak.{suffix}", path.display()));
        if suffix > 9999 {
            return Err(format!("{} 的备份文件名用尽了", path.display()));
        }
    }
    Ok(candidate)
}
