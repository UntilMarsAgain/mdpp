/// 把头里的 token 收成 `key=value` 参数。
///
/// 两种写法都认：
///
/// - `key=value`（等号贴着）—— 按**第一个**切：`key="a=b"` 的值就是 `a=b`；
/// - `key = value`（等号两侧留白）—— 两边的空白本来就被 [`tokenize`] 吃掉了，
///   所以这里只需要把落单的 `=` 与它前后两个 token 拼回去。
///
/// 第二种是**必须**认的，不是顺手加的：`key = value` 会被切成 `key` / `=` / `value`
/// 三个 token，而不带 `=` 的 token 算「开关式参数」，于是 `key` 静悄悄变成一个空值、
/// 值整个丢掉 —— 写的人看到的是「参数没生效」，查不到是哪个字被吃掉了。
pub(super) fn pairs(tokens: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        // `key = value`：等号是**下一个** token
        if tokens.get(index + 1).is_some_and(|next| next == "=") {
            // 再往后没有 token 时给空串：`key =` 是空值，不是「参数没给」
            let value = tokens.get(index + 2).cloned().unwrap_or_default();
            out.push((token.clone(), value));
            index += 3;
            continue;
        }
        // 落单的 `=`：没有键可配，它自己就是个开关式参数
        if token == "=" {
            out.push((token.clone(), String::new()));
            index += 1;
            continue;
        }
        match token.split_once('=') {
            Some((key, value)) => out.push((key.to_string(), value.to_string())),
            None => out.push((token.clone(), String::new())),
        }
        index += 1;
    }
    out
}

/// 把一行切成 token：空白分隔，但**引号里的空白不算分隔**。
///
/// 单引号与双引号都可以；引号内用反斜杠保护引号本身与反斜杠，引号外反斜杠没有
/// 特殊含义（路径里到处都是反斜杠，不该在这里被吃掉）。
///
/// 引号没有配对就整行走不通：返回 `None`，这一行退回普通段落。
pub fn tokenize(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();

    while let Some(ch) = chars.next() {
        match quote {
            Some(closing) => {
                if ch == '\\' {
                    match chars.next() {
                        Some('\\') => current.push('\\'),
                        Some(next) if next == closing => current.push(closing),
                        // 只认这两个转义，其余原样保留（连反斜杠一起）
                        Some(next) => {
                            current.push('\\');
                            current.push(next);
                        }
                        None => return None,
                    }
                } else if ch == closing {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    // `""` 是一个空值的 token，不是没有 token
                    started = true;
                }
                ch if ch.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                ch => {
                    current.push(ch);
                    started = true;
                }
            },
        }
    }

    if quote.is_some() {
        return None;
    }
    if started {
        tokens.push(current);
    }
    Some(tokens)
}
