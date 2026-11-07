//! 百分号编解码 —— 地址在哪儿写，写法就不一样。
//!
//! | 函数 | 策略 | 谁在用，为什么 |
//! |---|---|---|
//! | [`decode_exact`] | 解码；解出来不是 UTF-8 就 `None` | `assets`：报告里要的是**作者写下的那一份**，不要一串 `�` |
//! | [`encode_href`] | 只编码会截断地址的 ASCII，**中文原样留着** | 站内链接与资源地址：写出来要看得见、能手改 |
//!
//! 库那边（`mdpp::targets`，链接目标）对 `[](<…>)` 里的空格也取「只编码真正碍事的
//! 那几个」这条 —— 与 [`encode_href`] 同一个取法，但那份代码在另一个包里，只能各写各的。

/// 百分号解码，**保真**那一份：解出来的字节不是合法 UTF-8 就 `None`。
///
/// 用在**要对人说话**的地方（报告缺了哪个文件）：退回作者写下的原串，也比在报告里
/// 摆一串 `�` 强。
pub fn decode_exact(value: &str) -> Option<String> {
    String::from_utf8(decode_bytes(value)).ok()
}

/// 解出来的那些字节
///
/// 认不出的 `%`（`%GG`、末尾的 `%`）**原样留着**：地址是别人给的，猜它什么意思
/// 比照原样处理更容易出事。
fn decode_bytes(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

/// 百分号编码，**`href` 里写的**那一份：只编码会截断地址的字符（`#` 把地址切开、
/// 空格被浏览器改写），**非 ASCII 一个都不碰**。
///
/// 理由是可读性：浏览器与服务端都认 UTF-8 的路径，而百分号编码过的中文在站点里
/// 没法看、也没法手改 —— 地址既要机器跟得上，也要人改得动。
pub fn encode_href(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        let plain = !ch.is_ascii()
            || ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '.' | '_' | '/' | '~');
        if plain {
            out.push(ch);
        } else {
            let mut buffer = [0u8; 4];
            for byte in ch.encode_utf8(&mut buffer).as_bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `href` 那一份：**中文留着**，只动碍事的那几个字符 —— 编了码的地址没法读
    #[test]
    fn encode_href_keeps_the_chinese_and_encodes_the_troublemakers() {
        assert_eq!(encode_href("/指南/安装"), "/指南/安装");
        assert_eq!(encode_href("a b"), "a%20b");
        assert_eq!(encode_href("a#b"), "a%23b");
        assert_eq!(encode_href("a%b"), "a%25b");
        // 留着的那些字与编码出去的那些，解得回来也推得回去
        assert_eq!(
            decode_exact(&encode_href("指南/a b")).as_deref(),
            Some("指南/a b")
        );
    }

    /// 认不出的 `%` 不猜：原样留着，让它在下游照原样失败
    #[test]
    fn a_percent_that_is_not_a_code_stays_put() {
        assert_eq!(decode_exact("%GG").as_deref(), Some("%GG"));
        assert_eq!(decode_exact("100%").as_deref(), Some("100%"));
        assert_eq!(decode_exact("%FF"), None);
    }
}
