use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// 全局都允许的那几个。`class` 留着是刻意的：作者的模板要能挂自己那套样式。
const COMMON: &[&str] = &["class", "title", "lang", "dir"];

/// 白名单本体：`(标签, 它能带的属性)`。
///
/// 属性**逐标签**给：通配一份 `*` 是最容易出事的地方 —— `img` 拿到 `href` 就成了
/// 链接、`a` 拿到 `src` 就成了脚本载体。
/// 表里没有的标签 → 标签本身丢掉、内容留下（ammonia 遇到不在 `tags` 里的标签会
/// 拆掉标签、留下 children）。
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

/// 白名单里的标签名；`allow_marker` 时再加上 `<markdown>` 的占位元素。
pub(super) fn tags(allow_marker: bool) -> HashSet<&'static str> {
    let mut tags: HashSet<&'static str> = RULES.iter().map(|(tag, _)| *tag).collect();
    if allow_marker {
        tags.insert(MARKER_TAG);
    }
    tags
}

/// 每个标签能带的属性；`allow_marker` 时给占位元素加 `data-i`。
pub(super) fn attributes(allow_marker: bool) -> HashMap<&'static str, HashSet<&'static str>> {
    let mut map: HashMap<&'static str, HashSet<&'static str>> = RULES
        .iter()
        .map(|(tag, rules)| (*tag, rules.iter().copied().collect()))
        .collect();
    if allow_marker {
        map.insert(MARKER_TAG, ["data-i"].into_iter().collect());
    }
    map
}

/// 连内容一起丢掉的标签；交给 ammonia 的 `clean_content_tags`。
///
/// 与「不认识」的分界在这里：那边是「标签没了、内容留下」，这边是「内容可能正是危险
/// 的那一份」—— `<script>` 的内容是代码、`<style>` 的内容是能外联的 CSS。
pub(super) fn clean_content() -> HashSet<&'static str> {
    [
        "script", "style", "link", "meta", "base", "object", "embed", "applet", "frame",
        "frameset", "noscript", "template", "form", "input", "button", "select", "option",
        "optgroup", "textarea", "label", "fieldset", "legend", "datalist", "output", "progress",
        "meter", "svg", "math", "canvas",
    ]
    .into_iter()
    .collect()
}

/// `<markdown>` 在骨架里的占位元素名。
///
/// 不用私有区字符：槽位展开发生在净化之后，若按字符串替换，属性值里的槽位也会被
/// 替换成 HTML，等于多开一个注入口。元素名只在标签位置出现，属性值里的 `<` 会被
/// 序列化成 `&lt;`，字符串匹配不上。
pub(super) const MARKER_TAG: &str = "mdpp-markdown";

/// 属性值的最后一道校验。
///
/// ammonia 的 `url_schemes` 只管 `href` / `src`；`srcset` 是一组候选地址，它不拆开，
/// `target` / `cite` / `poster` / `download` 它也不看。
pub(super) fn filter_attribute<'a>(
    _element: &str,
    attribute: &str,
    value: &'a str,
) -> Option<Cow<'a, str>> {
    match attribute {
        "target" => matches!(value, "_blank" | "_self").then_some(Cow::Borrowed(value)),
        "srcset" => safe_srcset(value).then_some(Cow::Borrowed(value)),
        "cite" | "poster" | "download" => safe_url(value).then_some(Cow::Borrowed(value)),
        _ => Some(Cow::Borrowed(value)),
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
