/// 全局都允许的那几个。`class` 留着是刻意的：作者的模板要能挂自己那套样式。
const COMMON: &[&str] = &["class", "title", "lang", "dir"];

/// 布尔属性：只写名字、不带值。
///
/// `<details open>` 与 `<details open="">` 在 HTML 里是一回事，但输出成
/// `open=""` 读起来像值被清空了 —— 输出是我们自己拼的，就拼成作者写的样子。
pub(super) const BOOLEAN: &[&str] = &["open", "controls", "loop", "muted", "reversed"];

/// 白名单本体：`(标签, 它能带的属性)`。
///
/// 属性**逐标签**给：通配一份 `*` 是最容易出事的地方 —— `img` 拿到 `href` 就成了
/// 链接、`a` 拿到 `src` 就成了脚本载体。
/// 表里没有的标签 → 标签本身丢掉、内容留下（见 [`walk_node`]）。
const RULES: &[(&str, &[&str])] = &[
    // —— 分块与分区
    ("address", COMMON),
    ("article", COMMON),
    ("aside", COMMON),
    ("blockquote", &["class", "title", "lang", "dir", "cite"]),
    ("details", &["class", "title", "lang", "dir", "open"]),
    ("div", COMMON),
    ("dl", COMMON),
    ("dt", COMMON),
    ("dd", COMMON),
    ("figcaption", COMMON),
    ("figure", COMMON),
    ("footer", COMMON),
    ("hgroup", COMMON),
    ("header", COMMON),
    ("h1", COMMON),
    ("h2", COMMON),
    ("h3", COMMON),
    ("h4", COMMON),
    ("h5", COMMON),
    ("h6", COMMON),
    ("hr", COMMON),
    ("li", &["class", "title", "lang", "dir", "value"]),
    ("main", COMMON),
    ("nav", COMMON),
    (
        "ol",
        &["class", "title", "lang", "dir", "start", "reversed", "type"],
    ),
    ("p", COMMON),
    ("section", COMMON),
    ("summary", COMMON),
    ("ul", COMMON),
    // —— 行内
    (
        "a",
        &[
            "class", "title", "lang", "dir", "href", "target", "rel", "download",
        ],
    ),
    ("abbr", COMMON),
    ("b", COMMON),
    ("bdi", COMMON),
    ("bdo", &["class", "title", "lang", "dir"]),
    ("cite", COMMON),
    ("code", COMMON),
    ("data", &["class", "title", "lang", "dir", "value"]),
    (
        "del",
        &["class", "title", "lang", "dir", "cite", "datetime"],
    ),
    ("dfn", COMMON),
    ("em", COMMON),
    ("i", COMMON),
    (
        "ins",
        &["class", "title", "lang", "dir", "cite", "datetime"],
    ),
    ("kbd", COMMON),
    ("mark", COMMON),
    ("q", &["class", "title", "lang", "dir", "cite"]),
    ("rp", COMMON),
    ("rt", COMMON),
    ("ruby", COMMON),
    ("s", COMMON),
    ("samp", COMMON),
    ("small", COMMON),
    ("span", COMMON),
    ("strong", COMMON),
    ("sub", COMMON),
    ("sup", COMMON),
    ("time", &["class", "title", "lang", "dir", "datetime"]),
    ("u", COMMON),
    ("var", COMMON),
    ("wbr", COMMON),
    ("br", COMMON),
    // —— 表格
    ("caption", COMMON),
    ("col", &["class", "title", "lang", "dir", "span"]),
    ("colgroup", &["class", "title", "lang", "dir", "span"]),
    ("table", COMMON),
    ("tbody", COMMON),
    (
        "td",
        &[
            "class", "title", "lang", "dir", "colspan", "rowspan", "headers", "abbr",
        ],
    ),
    ("tfoot", COMMON),
    (
        "th",
        &[
            "class", "title", "lang", "dir", "colspan", "rowspan", "headers", "abbr", "scope",
        ],
    ),
    ("thead", COMMON),
    ("tr", COMMON),
    // —— 代码与预格式化（`::code` 那类自定义标记是靠 class 挂在这层上的）
    ("pre", COMMON),
    // —— 图片与音视频
    (
        "img",
        &[
            "class", "title", "lang", "dir", "src", "alt", "width", "height", "loading",
        ],
    ),
    ("picture", COMMON),
    (
        "source",
        &[
            "class", "title", "lang", "dir", "src", "srcset", "type", "media",
        ],
    ),
    (
        "track",
        &[
            "class", "title", "lang", "dir", "src", "kind", "srclang", "label",
        ],
    ),
    (
        "audio",
        &[
            "class", "title", "lang", "dir", "src", "controls", "loop", "muted", "preload",
        ],
    ),
    (
        "video",
        &[
            "class", "title", "lang", "dir", "src", "controls", "loop", "muted", "preload",
            "poster", "width", "height",
        ],
    ),
];

/// 按标签名查属性规则。`None` = 不在白名单里。
pub(super) fn allowed(tag: &str) -> Option<&'static [&'static str]> {
    RULES
        .iter()
        .find(|(name, _)| *name == tag)
        .map(|(_, attrs)| *attrs)
}

/// 这些标签**连内容一起**丢掉。
///
/// 与「不认识」的分界在这里：那边是「标签没了、内容留下」，这边是「内容可能正是危险
/// 的那一份」—— `<script>` 的内容是代码、`<style>` 的内容是能外联的 CSS。
pub(super) fn skip_subtree(tag: &str) -> bool {
    matches!(
        tag,
        "script"
            | "style"
            | "link"
            | "meta"
            | "base"
            | "object"
            | "embed"
            | "applet"
            | "frame"
            | "frameset"
            | "noscript"
            | "template"
            | "form"
            | "input"
            | "button"
            | "select"
            | "option"
            | "optgroup"
            | "textarea"
            | "label"
            | "fieldset"
            | "legend"
            | "datalist"
            | "output"
            | "progress"
            | "meter"
            | "svg"
            | "math"
            | "canvas"
            | "audio-g"
    )
}

/// 不用写结束标签的那些。
pub(super) fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// 属性留不留、拿什么值出去。`None` = 丢掉。返回的是**收拾过的值**：协议在这里验过，
/// `target` 收窄过。
pub(super) fn keep_attribute(name: &str, rules: &[&str], value: &str) -> Option<String> {
    if !rules.contains(&name) {
        return None;
    }
    // `style` 不在表里，但 `on*` 更该在这儿再兜一层 —— 免得以后加属性时手滑
    if name == "style" || name.starts_with("on") {
        return None;
    }
    match name {
        "srcset" => safe_srcset(value).then(|| value.to_string()),
        "href" | "src" | "cite" | "poster" | "download" => {
            safe_url(value).then(|| value.to_string())
        }
        // `_top` 与框架名能把人带走，而这里没有框架
        "target" => matches!(value, "_blank" | "_self").then(|| value.to_string()),
        "rel" => Some(value.to_string()),
        _ => Some(value.to_string()),
    }
}

/// `target="_blank"` 的 `a` 补上 `rel="noopener noreferrer"`；已有的 `rel`
/// 保留原词，只补缺的那两个。
pub(super) fn finish_a_attrs(tag: &str, attrs: &mut Vec<(String, String)>) {
    if tag != "a" {
        return;
    }
    let blank = attrs
        .iter()
        .any(|(name, value)| name == "target" && value == "_blank");
    if !blank {
        return;
    }
    match attrs.iter_mut().find(|(name, _)| name == "rel") {
        Some((_, rel)) => {
            let mut tokens: Vec<&str> = rel.split_whitespace().collect();
            for wanted in ["noopener", "noreferrer"] {
                if !tokens
                    .iter()
                    .any(|token| token.eq_ignore_ascii_case(wanted))
                {
                    tokens.push(wanted);
                }
            }
            *rel = tokens.join(" ");
        }
        None => attrs.push(("rel".to_string(), "noopener noreferrer".to_string())),
    }
}

/// `srcset` 是**一组**候选地址，不是一个地址：`https://x 1x, javascript:... 2x`
/// 里第一个协议合法，不代表整条都合法。逐个候选验，有一个不过就整条丢掉。
fn safe_srcset(value: &str) -> bool {
    value.split(',').all(|candidate| {
        let url = candidate.split_whitespace().next().unwrap_or("");
        !url.is_empty() && safe_url(url)
    })
}

/// 这个地址能不能放行。
///
/// 只认 `http` / `https` / `mailto` 与相对地址（含 `#片段`）。判定之前先照浏览器的
/// 规矩把协议那一段里的**空白与控制字符**去掉 —— 否则 `java\nscript:alert(1)` 与
/// ` javascript:` 都是绕得过去的，而浏览器认得它们。
fn safe_url(value: &str) -> bool {
    // 相对地址与片段没有协议这一段，直接放行
    let probe: String = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        .collect();
    let Some((scheme, _)) = probe.split_once(':') else {
        return true; // 没有协议：相对地址或 `#片段`
    };
    // `split_once` 给的 scheme 里可能混着 `/`、`?`、`#` —— 那就不是协议了，是相对地址
    if scheme.contains(['/', '?', '#']) {
        return true;
    }
    let scheme = scheme.to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "mailto")
}

// ---------------------------------------------------------------- 转义
