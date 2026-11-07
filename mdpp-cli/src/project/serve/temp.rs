use std::path::PathBuf;

/// 这一次预览临时用的目录
///
/// **不放项目的 `build/`**：预览到一半的东西不该被当成产物，而 `--clean` 一清
/// 就把上一次好好的那一遍弄没了。
///
/// 目录名带 pid，于是几次预览（包括两个终端同时开）互不相干。也因为如此，**进程被
/// 杀掉时它会留在临时目录里**：下一回起的是另一个 pid，撞不上；下面那段 `exists` 清的
/// 只是 pid 号恰好被系统复用回来的旧目录。想清干净自己 `rm -rf` 也行。
///
/// **名字里只有 ASCII** —— `std::env::temp_dir()` 给的那条路径在某些环境（老 Windows、
/// 非 UTF-8 的 locale、带转义的挂载点）会被系统按自己的编码重新解释，夹在中间的
/// 非 ASCII 字符是最容易在那一步变成乱码、于是 `exists` 与 `remove_dir_all` 指不到
/// 同一个目录的东西。这个目录名没有任何给人看的价值，`mdpp-preview-` 已经说清了它是谁的
pub(super) fn temp_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(preview_name());
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|error| format!("清不了 {}：{error}", dir.display()))?;
    }
    crate::project::ensure_dir(&dir)?;
    Ok(dir)
}

/// 这一次预览的临时目录叫什么
///
/// **名字里只有 ASCII** —— `std::env::temp_dir()` 给的那条路径在某些环境（老 Windows、
/// 非 UTF-8 的 locale、带转义的挂载点）会被系统按自己的编码重新解释，夹在中间的
/// 非 ASCII 字符是最容易在那一步变成乱码、于是 `exists` 与 `remove_dir_all` 指不到
/// 同一个目录的东西。这个目录名没有任何给人看的价值，`mdpp-preview-` 已经说清了它是谁的
pub(super) fn preview_name() -> String {
    format!("mdpp-preview-{}", std::process::id())
}

/// 预览的缓存目录：临时目录下面再垫一层，每一遍出在它自己的子目录里
///
/// **垫这一层是为了换根** —— `http` 那边拿的是一个可以改指向的 [`http::Root`]，
/// 而它指的是这一层里的某个子目录。一遍出完，指针挪到那个子目录上。
pub(super) fn cache_dir() -> Result<PathBuf, String> {
    let cache = temp_dir()?.join("cache");
    crate::project::ensure_dir(&cache)?;
    Ok(cache)
}
