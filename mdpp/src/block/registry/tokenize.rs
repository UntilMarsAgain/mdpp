/// 一个头 token。除了文字，还记两件 `pairs` 需要的事：
///
/// - `unquoted_equals`：引号外的 `=` 有几个（`key="a=b"` 只有分隔符那一个）；
/// - `ends_with_unquoted_equals`：最后一个字符是不是引号外的 `=`。
///
/// 有了它们，`f=""`、`key="a="`、`key= value`、`a="" b=""` 才不会互相混淆。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) text: String,
    pub(super) unquoted_equals: usize,
    pub(super) ends_with_unquoted_equals: bool,
}

/// 把头里的 token 收成 `key=value` 参数。
///
/// 等号两侧的空格**一律忽略**：`key=value`、`key = value`、`key= value`、
/// `key ="value"` 是同一个意思。切分按**第一个** `=` 来，所以 `key="a=b"` 的值
/// 就是 `a=b`。
///
/// 裸 flag 是语法糖：只写 `key` 等于 `key=on`。显式写成 `key=` 才是空值 ——
/// 模板里的可选参数缺失时会落成 `key=`，这样两者就分得开。
///
/// 只有一个歧义要特别处理：`a="" b=""` 里 `a=` 后面那个 `b=` 是**下一个参数**，
/// 不是值。未加引号、里面又带 `=` 的 token 按「像另一个键」处理；真要把 `a=b`
/// 当值，请加引号。
pub(super) fn pairs(tokens: &[Token]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        // `key = value`：等号是**下一个** token（`tokenize` 已经吃掉了空白）
        if tokens.get(index + 1).is_some_and(|next| next.text == "=") {
            // 再往后没有 token 时给空串：`key =` 是空值，不是「参数没给」
            let value = tokens
                .get(index + 2)
                .map(|next| next.text.clone())
                .unwrap_or_default();
            out.push((token.text.clone(), value));
            index += 3;
            continue;
        }

        // `key= value` / `key= "value"`：等号贴在键上，值在下一个 token。
        if token.text.len() > 1
            && token.unquoted_equals == 1
            && token.ends_with_unquoted_equals
            && !token.text.starts_with('=')
        {
            let key = token.text[..token.text.len() - 1].to_string();
            if let Some(next) = tokens.get(index + 1) {
                // 文字以 `=` 结尾、而且那个 `=` 在引号外的下一个 token，更像
                // `b=""` 这样的新键；`a=b` 与引号里的 `"x="` 仍然算值。
                let looks_like_key = next.text.ends_with('=') && next.unquoted_equals > 0;
                if !looks_like_key && next.text != "=" {
                    out.push((key, next.text.clone()));
                    index += 2;
                    continue;
                }
            }
            out.push((key, String::new()));
            index += 1;
            continue;
        }

        // `key =value` / `key ="value"`：等号贴在值上，键是前一个 token。
        if token.unquoted_equals == 0 {
            if let Some(next) = tokens.get(index + 1) {
                if next.text.len() > 1 && next.text.starts_with('=') {
                    out.push((token.text.clone(), next.text[1..].to_string()));
                    index += 2;
                    continue;
                }
            }
        }

        // 落单的 `=`：没有键可配，它自己就是个开关式参数
        if token.text == "=" {
            out.push((token.text.clone(), String::new()));
            index += 1;
            continue;
        }
        match token.text.split_once('=') {
            Some((key, value)) => out.push((key.to_string(), value.to_string())),
            // 裸 flag：语法糖，等价于 `key=on`
            None => out.push((token.text.clone(), "on".to_string())),
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
    tokenize_meta(line).map(|tokens| tokens.into_iter().map(|token| token.text).collect())
}

/// 与 [`tokenize`] 同一条扫描，但保留「这个 token 里出现过引号」这一位。
///
/// `pairs` 需要它来判断 `a="" b=""` 里的 `b=` 是下一个键而不是 `a` 的值；
/// 公共的 [`tokenize`] 不需要，保持原来的 `Vec<String>` 形状。
pub(super) fn tokenize_meta(line: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut unquoted_equals = 0usize;
    let mut last_unquoted_equals = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();

    while let Some(ch) = chars.next() {
        match quote {
            Some(closing) => {
                last_unquoted_equals = false;
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
                    last_unquoted_equals = false;
                }
                ch if ch.is_whitespace() => {
                    if started {
                        tokens.push(Token {
                            text: std::mem::take(&mut current),
                            unquoted_equals,
                            ends_with_unquoted_equals: last_unquoted_equals,
                        });
                        started = false;
                        unquoted_equals = 0;
                        last_unquoted_equals = false;
                    }
                }
                '=' => {
                    current.push(ch);
                    started = true;
                    unquoted_equals += 1;
                    last_unquoted_equals = true;
                }
                ch => {
                    current.push(ch);
                    started = true;
                    last_unquoted_equals = false;
                }
            },
        }
    }

    if quote.is_some() {
        return None;
    }
    if started {
        tokens.push(Token {
            text: current,
            unquoted_equals,
            ends_with_unquoted_equals: last_unquoted_equals,
        });
    }
    Some(tokens)
}
