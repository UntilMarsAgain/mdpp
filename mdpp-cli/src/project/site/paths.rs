use std::path::PathBuf;

use crate::support::url::encode_href;

use super::{INDEX_KEY, PAGE_SUFFIXES};

/// 砍掉末尾的页面后缀（带点，比大小写不敏感 —— 归一本来就要转小写）。
/// 不是那两个之一就原样留着：`2.0.说明` 里的 `.说明` 不该被当成后缀砍掉
pub(super) fn strip_suffix(text: &str) -> &str {
    PAGE_SUFFIXES
        .iter()
        .find_map(|suffix| {
            let cut = text.len().checked_sub(suffix.len() + 1)?;
            // 先比一比再切：`cut` 未必落在字符边界上（`入门` 的长度正好让 `.mdpp`
            // 那次尝试切在字的中间），而 `text[..cut]` 会当场 panic
            let dotted = format!(".{suffix}");
            text.get(cut..)
                .filter(|tail| tail.eq_ignore_ascii_case(&dotted))
                .map(|_| &text[..cut])
        })
        .unwrap_or(text)
}

/// 页面键的校验：键最终会变成产物里的路径，任何一段都不能是 `.` / `..`、
/// 不能带反斜杠或控制字符。**在算输出路径之前调用** —— 校验不过的键
/// 连 `PathBuf` 都不该拼出来。
///
/// 反斜杠在 Unix 上本来是合法文件名字符，但它在 Windows 上是分隔符；一个键
/// 如果两边解释不同，同一个项目在两个平台上的产物就会不一样。这里统一拒绝。
pub fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("页面键不能为空".to_string());
    }
    for segment in key.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!(
                "页面键「{key}」里有一段不是页名（空段、`.` 或 `..`）"
            ));
        }
        if segment.contains('\\') {
            return Err(format!(
                "页面键「{key}」里不能有反斜杠 —— 目录层级用 `/` 写"
            ));
        }
        if segment.chars().any(char::is_control) {
            return Err(format!("页面键「{key}」里有控制字符"));
        }
        // Windows 上这些名字不能当文件/目录名。站点通常要能跨平台构建，
        // 所以在哪个平台上都拒绝，而不是等到 Windows 上才炸。
        if windows_reserved(segment) {
            return Err(format!(
                "页面键「{key}」里的「{segment}」是 Windows 保留名，换个名字"
            ));
        }
    }
    Ok(())
}

/// Windows 设备名（不区分大小写，带不带后缀都算）
fn windows_reserved(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let Some(rest) = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
    else {
        return false;
    };
    // COM1..COM9 / LPT1..LPT9；COM0 与 COM10 不是保留名
    rest.len() == 1 && matches!(rest.as_bytes()[0], b'1'..=b'9')
}

/// 输出文件：`<键>/index.html`。
///
/// **首页是个例外**：`index` 落在 `build/index.html`，而不是
/// `build/index/index.html` —— 后者会让站点根上多出一层没人访问得到的目录。
///
/// 这里再挡一次 `.` / `..`：调用方应当先走 [`check_key`]，但拼路径这一步
/// 是最后一道闸，不能只靠上游记得查。
pub fn output_of(key: &str) -> PathBuf {
    let mut out = PathBuf::new();
    if key != INDEX_KEY {
        for segment in key.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                continue;
            }
            out.push(segment);
        }
    }
    out.push("index.html");
    out
}

/// 一个链接目标的**归一**写法：拿来查表的那把钥匙。
///
/// 宽松到能容下人手写出来的东西，严格到不会认错：`\` 当 `/`、前后空白、
/// 前导 `./` 与 `/`、结尾的 `/`，以及 `.md` / `.mdpp` 后缀，一律去掉；比大小写不敏感。
/// 空串不算目标。
pub fn normalize(target: &str) -> Option<String> {
    let mut text = target.trim().replace('\\', "/");
    text = text.trim_start_matches('/').to_string();
    text = text.strip_prefix("./").unwrap_or(&text).to_string();
    text = strip_suffix(text.trim_end_matches('/')).to_string();
    (!text.is_empty()).then(|| text.to_lowercase())
}

/// `template:` 开头的目标是**模板页**，不是站内页面
pub fn is_template_target(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed
        .split_once(':')
        .is_some_and(|(prefix, _)| prefix.trim().eq_ignore_ascii_case("template"))
}

/// 从一页走到站点根的资源目录：`../` × (键的段数) + `assets`。
///
/// 与 [`href`] 同样要数层数：产物是目录式地址，`示例/` 里写 `assets/mdpp.css` 会解析到
/// `/示例/assets/mdpp.css` —— 那里没有东西，于是**样式与脚本全丢，而且不报错**
/// （页面照出得来，只是光着身子）。
pub fn assets_href(key: &str) -> String {
    format!(
        "{}{}",
        "../".repeat(segments(key)),
        crate::render::page::ASSET_DIR
    )
}

/// 从 `from`（一页的键）走到 `to` 的地址。
///
/// **页与页之间是相对的**，每退一层加一个 `../`，目标总是以 `/` 结尾 —— 产物是
/// **目录**，不是文件。相对地址而不是从站点根算的绝对地址：这样 `build/index.html`
/// 拷到任何地方、挂在子路径下，链接都还对。
///
/// 要退几层 = **`from` 的段数**：键 `指南/深入/安装` 的页落在 `/指南/深入/安装/`，
/// 从那儿回站点根要退三层。而首页落在站点根上（`build/index.html`，段数 0），退 0 层。
///
/// 首页**不是**特例：`../` × 0 就是空串，空串在地址里指的是当前目录，正是首页自己。
pub fn href(from: &str, to: &str) -> String {
    // 首页不能走下面那条：它没有 `<根>/index/` 那个目录，退回去也只是回到它自己
    if to == INDEX_KEY {
        return root_href(from);
    }
    let mut out = "../".repeat(segments(from));
    out.push_str(&encode_href(to));
    out.push('/');
    out
}

/// 一个键有几段。首页（`index`）落在站点根上，**算 0 段** ——
/// 它不是 `<根>/index/` 那个目录，而是 `<根>/index.html` 那个文件
fn segments(key: &str) -> usize {
    if key == INDEX_KEY {
        return 0;
    }
    key.split('/').filter(|one| !one.is_empty()).count()
}

/// 从 `from` 走到站点根的地址：`../` × [`segments`]
///
/// 首页给的是 `./` 而不是空串：空串在地址里指的是「当前目录」，可从 `/示例/` 出发
/// 那指的是 `/示例/` 自己 —— 首页链接到自己，看着像坏了
fn root_href(from: &str) -> String {
    let depth = segments(from);
    if depth == 0 {
        return String::from("./");
    }
    "../".repeat(depth)
}
