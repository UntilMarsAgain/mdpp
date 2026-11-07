use std::path::{Component, Path, PathBuf};

use crate::project::{Project, shown};
use tracing::debug;

pub(super) fn staging_dir(site_dir: &Path) -> Result<PathBuf, String> {
    let parent = site_dir
        .parent()
        .ok_or_else(|| format!("site_dir（{}）没有上一级目录", shown(site_dir)))?;
    // 名字只用 ASCII、也不带产物目录名：`build_dir` 可以由用户写成中文，
    // 但临时目录不参与展示，没必要跟着它一起变成非 ASCII。
    Ok(parent.join(format!(".mdpp-tmp-{}", std::process::id())))
}

/// 把出好的临时目录换成正式目录。
///
/// 顺序：旧目录先改名到备份 → 临时目录改名到正式 → 删备份。第二步失败时把
/// 备份改回去，于是「失败」不会留下半个站点，也不会把旧站点弄丢。
pub(super) fn swap_into_place(staging: &Path, site_dir: &Path) -> Result<(), String> {
    if !staging.is_dir() {
        return Err(format!("临时产物目录不在（{}）", shown(staging)));
    }
    let parent = site_dir
        .parent()
        .ok_or_else(|| format!("site_dir（{}）没有上一级目录", shown(site_dir)))?;
    let backup = parent.join(format!(".mdpp-old-{}", std::process::id()));
    remove_any(&backup);

    let had_old = site_dir.exists();
    if had_old {
        std::fs::rename(site_dir, &backup)
            .map_err(|error| format!("挪不开旧站点 {}：{error}", shown(site_dir)))?;
    }
    if let Err(error) = std::fs::rename(staging, site_dir) {
        // 换名失败：把旧站点挪回去，再报错
        if had_old {
            let _ = std::fs::rename(&backup, site_dir);
        }
        return Err(format!(
            "换不了站点目录 {}：{error}（旧站点已尽量还原）",
            shown(site_dir)
        ));
    }
    if had_old {
        remove_any(&backup);
    }
    Ok(())
}

/// 删一个文件或目录；不关心它是什么、也不在意它已经不在了
pub(super) fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// 一页渲染完的产物（还没套外壳、还没落盘）。
/// `build` 的最终目录校验：整目录替换比 `--clean` 更不能删错地方。
///
/// **`site_dir` 是配置里的一行字**，打错一个字就可能让这条命令去删整个项目 ——
/// 甚至去删项目外面的一整个目录。所以这里不是「检查一下」，而是：
///
/// - 默认**必须**严格位于项目根之内，且不能是正文/模板目录或它们的子目录；
/// - 无论谁调用，都不能是项目根、正文、模板的**祖先**，不能是 `/`、`$HOME`、
///   系统临时目录本身；
/// - 比较之前先做词法归一 + 已存在部分的符号链接解析；比较的是真路径，不是字面。
///
/// `allow_outside` 只给 `serve` 用（它出到系统临时目录里的预览缓存）。命令行
/// 上的 `build` 永远拿不到这个开关。
pub(super) fn check_clean(
    project: &Project,
    site_dir: &Path,
    allow_outside: bool,
) -> Result<(), String> {
    let site_dir = real_path(site_dir);
    let root = real_path(&project.root);
    let pages = real_path(&project.pages_dir());
    let templates = real_path(&project.templates_dir());

    if !allow_outside {
        if site_dir == root || !site_dir.starts_with(&root) {
            return Err(format!(
                "site_dir（{}）必须在项目目录（{}）里面，且不能就是项目根 —— \
                 `build` 会整目录替换它",
                shown(&site_dir),
                shown(&root)
            ));
        }
        for (what, keep) in [("正文", &pages), ("模板", &templates)] {
            if site_dir == *keep || site_dir.starts_with(keep) {
                return Err(format!(
                    "site_dir（{}）不能是{what}目录或它的子目录 —— 替换它会连内容一起删掉",
                    shown(&site_dir)
                ));
            }
        }
    }

    // 这几种位置，就算调用方允许「项目外」也不该清。
    if site_dir.parent().is_none() {
        return Err(format!(
            "site_dir（{}）是根目录，拒绝替换",
            shown(&site_dir)
        ));
    }
    for (what, keep) in [("项目根", &root), ("正文", &pages), ("模板", &templates)] {
        if keep.starts_with(&site_dir) {
            return Err(format!(
                "site_dir（{}）是{what}目录的上一层 —— 替换它会连内容一起删掉",
                shown(&site_dir)
            ));
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if real_path(&home) == site_dir {
            return Err("site_dir 指向家目录，拒绝替换".to_string());
        }
    }
    if real_path(&std::env::temp_dir()) == site_dir {
        return Err("site_dir 指向系统临时目录本身，拒绝替换".to_string());
    }
    Ok(())
}

/// 先验再删：`serve` 的临时目录与 `build_into` 的直写路径都走这里
pub(super) fn clean(project: &Project, site_dir: &Path, allow_outside: bool) -> Result<(), String> {
    check_clean(project, site_dir, allow_outside)?;
    if site_dir.exists() {
        std::fs::remove_dir_all(site_dir)
            .map_err(|error| format!("清不了 {}：{error}", shown(site_dir)))?;
        debug!(dir = shown(site_dir), "已清空产物目录");
    }
    Ok(())
}

/// 词法归一 + 已存在部分的符号链接解析。
///
/// `a/../b` 与 `b` 要比较成同一个东西；`build` 是指向别处的符号链接时，
/// 也要按它真正指向的地方比较。不存在的尾部按字面接回去 —— 那正是
/// `build` 要建出来的目录。
pub(super) fn real_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut lexical = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other.as_os_str()),
        }
    }
    // 已存在的最深一层做 canonicalize，剩下的尾部接回去
    let mut probe = lexical.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !probe.exists() {
        match probe.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                probe.pop();
            }
            None => break,
        }
    }
    let mut out = std::fs::canonicalize(&probe).unwrap_or(probe);
    for name in tail.iter().rev() {
        out.push(name);
    }
    out
}
