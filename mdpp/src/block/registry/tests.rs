use super::*;
use markdown_it::{Node, Renderer};

fn noop(_: &Block, _: &Node, _: &mut dyn Renderer) {}

/// 头一行切成名字加参数的那一步 —— 派发、渲染、诊断读的都是这一份，切错了没人看得出来
#[test]
fn parsing_a_header_yields_name_and_params() {
    let (name, params) = Block::parse_header("::note key=value flag 另一个=值").unwrap();
    assert_eq!(name, "note");
    assert_eq!(
        params,
        vec![
            ("key".to_string(), "value".to_string()),
            ("flag".to_string(), "on".to_string()),
            ("另一个".to_string(), "值".to_string()),
        ]
    );
}

/// 引号里的空白不算分隔、`\"` 出来是一个引号 —— 少了这条，带空格的值会被劈成几段参数
#[test]
fn quotes_protect_spaces_and_escapes() {
    let (name, params) =
        Block::parse_header(r#"::"两 个词" 标题="含 空格" 引号="他说\"好\"" flag"#).unwrap();
    assert_eq!(name, "两 个词");
    assert_eq!(
        params,
        vec![
            ("标题".to_string(), "含 空格".to_string()),
            ("引号".to_string(), "他说\"好\"".to_string()),
            ("flag".to_string(), "on".to_string()),
        ]
    );
}

/// 冒号不是块名的一部分：`::template:卡片` 与 `::a:b` 一样是普通段落
#[test]
fn a_colon_in_a_name_is_not_a_block() {
    assert!(Block::parse_header("::template:卡片 title=甲").is_none());
    assert!(Block::parse_header("::Template:卡片").is_none());
    assert!(Block::parse_header("::a:b").is_none());
    assert!(Block::parse_header("::帮助:入门").is_none());
}

/// 认不出的头一律退回普通段落 —— 宁可当普通文字，也不要猜作者想要什么
#[test]
fn lines_that_are_not_headers_are_rejected() {
    for line in [
        "::",
        "::=x",
        "::a:b",
        ":name",
        "普通一行",
        "::note key=\"没关上",
    ] {
        assert!(Block::parse_header(line).is_none(), "{line:?} 不该被认成头");
    }
}

/// 引号外的反斜杠是**字面**的：路径里到处都是它，不该在这里被吃掉
#[test]
fn backslash_outside_quotes_is_literal() {
    let (_, params) = Block::parse_header(r"::note path=C:\笔记").unwrap();
    assert_eq!(params, vec![("path".to_string(), r"C:\笔记".to_string())]);
}

/// 等号两侧留白也认 —— `key = value` 与 `key=value` 是同一个意思。
///
/// 这条是**必须**的：`tokenize` 按空白切，而不带 `=` 的 token 算「开关式参数」，
/// 所以修复之前 `title = 提醒` 会变成一个空的 `title` 加一个叫「提醒」的开关，
/// 而值整个丢掉 —— 写的人只看到「参数没生效」，查不出是哪个字被吃掉了。
#[test]
fn spaces_around_the_equals_are_fine() {
    let (name, params) = Block::parse_header("::note title = 提醒 color = #0af").unwrap();
    assert_eq!(name, "note");
    assert_eq!(
        params,
        vec![
            ("title".to_string(), "提醒".to_string()),
            ("color".to_string(), "#0af".to_string())
        ]
    );

    // 两种写法混着写也一样
    let (_, mixed) = Block::parse_header("::note a = 1 b=2 c = 3").unwrap();
    assert_eq!(
        mixed,
        vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
            ("c".to_string(), "3".to_string())
        ]
    );
}

/// 等号一边贴一边空也认 —— 空白只是排版，不该改变参数的意思
#[test]
fn spaces_on_either_side_of_the_equals_are_ignored() {
    let (_, params) =
        Block::parse_header("::note a= 1 b =2 c= \"含 空格\" d =\"含 空格\" e=\"\" f=\"\"")
            .unwrap();
    assert_eq!(
        params,
        vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
            ("c".to_string(), "含 空格".to_string()),
            ("d".to_string(), "含 空格".to_string()),
            ("e".to_string(), String::new()),
            ("f".to_string(), String::new()),
        ]
    );
}

/// 等号后带空格时，值里即使有 `=` 也还是值；只有像 `b=` 这样的下一个键才切开
#[test]
fn a_spaced_value_may_contain_equals_but_a_following_key_stays_a_key() {
    let (_, params) = Block::parse_header("::note a= x=y b= c=\"z=\" d= \"v=\" f=\"\"").unwrap();
    assert_eq!(
        params,
        vec![
            ("a".to_string(), "x=y".to_string()),
            ("b".to_string(), String::new()),
            ("c".to_string(), "z=".to_string()),
            ("d".to_string(), "v=".to_string()),
            ("f".to_string(), String::new()),
        ]
    );
}

/// 两个空值参数相邻时，前一个的 `=` 不该把后一个键吃掉
#[test]
fn adjacent_empty_values_keep_their_own_keys() {
    let (_, params) = Block::parse_header("::note a=\"\" b=\"\" c= d=\"\"").unwrap();
    assert_eq!(
        params,
        vec![
            ("a".to_string(), String::new()),
            ("b".to_string(), String::new()),
            ("c".to_string(), String::new()),
            ("d".to_string(), String::new()),
        ]
    );
}

/// 引号里的空格仍然由引号保护 —— 等号两侧留白这条不该把它带跑
#[test]
fn spaces_around_the_equals_keep_quoted_spaces_safe() {
    let (_, params) = Block::parse_header(r#"::note 标题 = "含 空格 的 标题""#).unwrap();
    assert_eq!(
        params,
        vec![("标题".to_string(), "含 空格 的 标题".to_string())]
    );
}

/// `key =` 后面空了是**空值**，不是「参数没给」—— 两种写法一致
#[test]
fn a_trailing_equals_is_an_empty_value() {
    let (_, with_space) = Block::parse_header("::note title =").unwrap();
    let (_, without) = Block::parse_header("::note title=").unwrap();
    assert_eq!(with_space, without);
    assert_eq!(with_space, vec![("title".to_string(), String::new())]);
}

/// 落单的 `=` 不该把后一个参数吃掉：它是开关，不是分隔符
#[test]
fn a_lone_equals_does_not_swallow_the_next_parameter() {
    let (_, params) = Block::parse_header("::note = flag title=提醒").unwrap();
    assert_eq!(
        params,
        vec![
            ("=".to_string(), String::new()),
            ("flag".to_string(), "on".to_string()),
            ("title".to_string(), "提醒".to_string())
        ]
    );
}

/// 开关式参数照旧认得：写成 `key = flag` 时，`flag` 是**值**
#[test]
fn an_equals_with_a_space_still_takes_the_next_token_as_its_value() {
    let (_, params) = Block::parse_header("::note title = flag").unwrap();
    assert_eq!(params, vec![("title".to_string(), "flag".to_string())]);
}

/// 值里带 `=` 的按**第一个**切，与留白那条是同一个规则的两面
#[test]
fn only_the_first_equals_splits() {
    let (_, params) = Block::parse_header(r#"::note 标题 = "a=b=c""#).unwrap();
    assert_eq!(params, vec![("标题".to_string(), "a=b=c".to_string())]);
}

/// 裸 flag 是 `flag=on` 的语法糖；显式 `flag=` 才是空串
#[test]
fn a_bare_flag_is_present_and_on() {
    let (_, params) = Block::parse_header("::x flag empty=").unwrap();
    assert_eq!(
        params,
        vec![
            ("flag".to_string(), "on".to_string()),
            ("empty".to_string(), String::new()),
        ]
    );

    let block = Block {
        name: "x".into(),
        params,
        body: String::new(),
    };
    assert!(block.has("flag"));
    assert_eq!(block.param("flag"), Some("on"));
    assert!(block.has("empty"));
    assert_eq!(block.param("empty"), Some(""));
    assert!(!block.has("other"));
    assert_eq!(block.param("other"), None);
}

/// 同名再灌一次是覆盖，不是追加 —— 换了实现就该只剩新的那一份，表里不留两条同名的
#[test]
fn registering_the_same_name_twice_replaces_rather_than_duplicates() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("a", ContentMode::Markdown, noop);
    registry.register_fn("b", ContentMode::Raw, noop);
    registry.register_fn("a", ContentMode::Sections, noop);

    assert_eq!(registry.len(), 2, "{:?}", registry.names());
    assert_eq!(
        registry.get("a").map(BlockSpec::mode),
        Some(ContentMode::Sections)
    );
}

/// 摘掉之后旧名字就查不到了，换名装回去也只有新名字 —— 一条注册项只认一个名字
#[test]
fn a_block_can_be_taken_out_and_put_back_under_another_name() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("a", ContentMode::Markdown, noop);
    let taken = registry.remove("a").unwrap();
    assert!(!registry.contains("a"));

    registry.register(taken.with_name("c"));
    assert!(registry.contains("c"));
    assert!(!registry.contains("a"));
}

/// 名单按注册次序给，不排序 —— 诊断与帮助里「先加的先出现」，加了什么一眼看得见
#[test]
fn registration_order_is_kept_for_listing() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("z", ContentMode::Raw, noop);
    registry.register_fn("a", ContentMode::Raw, noop);
    registry.register_fn("m", ContentMode::Raw, noop);
    assert_eq!(registry.names(), vec!["z", "a", "m"]);
}

/// 行表格解析：空行跳过，按 `|` 拆，单元格 trim；限量版保留最后一格里的 `|`
#[test]
fn rows_split_cells_and_limited_rows_keep_the_tail() {
    let block = Block {
        name: "表格".into(),
        params: Vec::new(),
        body: "甲|乙|丙\n\n 丁 | 戊 \n没有分隔\n".into(),
    };
    assert_eq!(
        block.rows(),
        vec![
            vec!["甲".to_string(), "乙".to_string(), "丙".to_string()],
            vec!["丁".to_string(), "戊".to_string()],
            vec!["没有分隔".to_string()],
        ]
    );
    assert_eq!(
        block.rows_limited(Some(2)),
        vec![
            vec!["甲".to_string(), "乙|丙".to_string()],
            vec!["丁".to_string(), "戊".to_string()],
            vec!["没有分隔".to_string()],
        ]
    );
}
