use super::*;
use clap::CommandFactory;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("mdpp").chain(args.iter().copied())).expect("不该报错")
}

fn fail(args: &[&str]) -> String {
    Cli::try_parse_from(std::iter::once("mdpp").chain(args.iter().copied()))
        .expect_err("本该报错")
        .to_string()
}

/// 子命令各自的参数，从顶层解析进去
fn render(args: &[&str]) -> Render {
    match parse(args).command {
        Command::Render(render) => render,
        other => panic!("要的是 render，给的是 {other:?}"),
    }
}

/// derive 出来的命令定义自洽 —— 少一个 `id`、写重一个短选项，clap 会在启动时炸。
/// 这一条让那种炸法在 `cargo test` 里就发生。
#[test]
fn the_command_definition_is_valid() {
    Cli::command().debug_assert();
}

/// 少一个子命令就打印帮助 —— 「不知道该敲什么」时不该只回一句错误
#[test]
fn no_subcommand_prints_the_help() {
    let cli = Cli::try_parse_from(["mdpp"]).expect_err("本该报「缺子命令」");
    assert_eq!(
        cli.kind(),
        clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
}

/// 一个字不认的东西要说清是**哪个字**不认，并给出「你是不是想写什么」
#[test]
fn an_unknown_subcommand_is_refused_by_name() {
    let message = fail(&["buidl"]);
    assert!(message.contains("buidl"), "{message}");
}

/// 全局选项在子命令**前后**都能写：`mdpp -d build` 与 `mdpp build -d` 是一回事
#[test]
fn the_global_flags_work_on_either_side() {
    assert!(parse(&["-d", "build"]).debug);
    assert!(parse(&["build", "-d"]).debug);
    assert!(parse(&["--debug", "render"]).debug);
    assert!(!parse(&["render"]).debug);
}

/// `-C, --dir` 与 `--debug` 一条待遇：**两侧都能写**。不给就是 `None`（不切，
/// 老老实实待在敲命令的地方）
#[test]
fn the_workdir_flag_lands_on_either_side_of_the_subcommand() {
    use std::path::Path;

    for args in [vec!["-C", "某处", "build"], vec!["build", "--dir", "某处"]] {
        let cli = parse(&args);
        assert_eq!(cli.workdir.as_deref(), Some(Path::new("某处")), "{args:?}");
    }
    assert!(parse(&["build"]).workdir.is_none());

    // 帮助里得看得见它：这是「不在项目目录里时怎么用」的唯一入口
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains("-C"), "帮助里少了 -C：\n{help}");
}

/// 一个文件都不给**不是**用法错误：inputs 空着，渲染那边才走标准输入；
/// `--full` 默认也是关的
#[test]
fn render_without_arguments_reads_standard_input() {
    let render = render(&["render"]);
    assert!(render.inputs.is_empty());
    assert!(!render.full);
}

/// 位置参数按敲进去的次序收 —— 多文件时输出的段落次序，以及默认标题取的
/// 「第一个文件名」，都指着这个次序
#[test]
fn files_land_in_order() {
    let render = render(&["render", "甲.md", "乙.md"]);
    assert_eq!(render.inputs.len(), 2);
    assert!(render.inputs[0].ends_with("甲.md"));
    assert!(render.inputs[1].ends_with("乙.md"));
}

/// 短写与长写是同一件事的两种写法，两样都得认 —— 帮助里两样都列着，
/// 只认一种，人照另一半敲就是一句「不认识」
#[test]
fn short_and_long_forms_both_work() {
    let one = render(&["render", "-f", "-L"]);
    assert!(one.full);
    assert!(one.list_blocks);
    assert!(render(&["render", "--list-blocks"]).list_blocks);
    assert!(render(&["render", "--stats"]).stats);
    assert!(render(&["render", "-s"]).stats);
    // 长选项与短选项各自都能单独用
    assert!(render(&["render", "--unsafe-html"]).unsafe_html);
    assert!(render(&["render", "-u"]).unsafe_html);
}

/// 带值的两种写法是同一件事：`--title 某篇` 与 `--title=某篇` ——
/// 只认一种，另一种写法就被当成缺值报错
#[test]
fn values_can_follow_or_use_equals() {
    assert_eq!(
        render(&["render", "--title", "某篇"]).title.as_deref(),
        Some("某篇")
    );
    assert_eq!(
        render(&["render", "--title=某篇"]).title.as_deref(),
        Some("某篇")
    );
}

/// 标题的兜底链：`--title` 优先，其次第一个输入的**文件名**（不是整条路径），
/// 再没有也得给个「未命名」—— 每一档都不能塌成空串
#[test]
fn the_title_defaults_to_the_first_file_name() {
    assert_eq!(render(&["render", "甲.md"]).title_or(), "甲.md");
    assert_eq!(
        render(&["render", "/a/b/乙.md", "丙.md"]).title_or(),
        "乙.md"
    );
    assert_eq!(
        render(&["render", "--title", "卷首", "甲.md"]).title_or(),
        "卷首"
    );
    assert_eq!(render(&["render"]).title_or(), "未命名");
}

/// 主题只认那三个值：写错时要在**解析时**就说，而不是渲染到一半才发现。
/// `system` 是「跟随系统」，与干脆不给 `--theme` 一样
#[test]
fn only_system_light_and_dark_are_themes() {
    for theme in ["system", "light", "dark"] {
        assert_eq!(
            render(&["render", "--theme", theme]).theme.as_deref(),
            Some(theme),
            "{theme} 该认得"
        );
    }
    assert!(render(&["render"]).theme.is_none(), "不给就是跟随系统");

    let message = fail(&["render", "--theme", "auto"]);
    for value in ["system", "light", "dark"] {
        assert!(message.contains(value), "{message}");
    }
}

/// `--css` / `--js` 可以给多次，按给的次序
#[test]
fn extra_files_are_collected_in_order() {
    let options = render(&[
        "render", "--full", "--css", "a.css", "--css", "b.css", "--js", "x.js",
    ]);
    assert_eq!(options.css.len(), 2);
    assert_eq!(options.js.len(), 1);
    assert!(options.css[0].ends_with("a.css"));
    assert!(options.css[1].ends_with("b.css"));
}

/// 片段里没有地方放这些东西，所以给了 `--css` 却不给 `--full` 是个用法错误。
/// 静默忽略才是坑 —— 人会以为自己的样式进去了。
#[test]
fn attaching_files_needs_the_full_mode() {
    let message = fail(&["render", "--css", "a.css"]);
    assert!(message.contains("--full"), "{message}");
    let message = fail(&["render", "--js", "x.js"]);
    assert!(message.contains("--full"), "{message}");
    // 给了 --full 就不报
    assert_eq!(render(&["render", "--full", "--css", "a.css"]).css.len(), 1);
}

/// 缺值要说清楚，不能默默当成空串 —— 那会渲染出一个空的 title。
#[test]
fn a_missing_value_is_a_usage_error() {
    let message = fail(&["render", "--title"]);
    assert!(message.contains("--title"), "{message}");
}

/// `--` 之后的一律是文件名：路径以 `-` 开头的那种
#[test]
fn everything_after_a_bare_double_dash_is_a_file_name() {
    let options = render(&["render", "--", "--full"]);
    assert_eq!(options.inputs.len(), 1);
    assert_eq!(options.inputs[0].to_string_lossy(), "--full");
    assert!(!options.full, "`--` 之后那个不是选项");
}

/// `new` 的页名是**必填**的 —— 少给一个就该问，而不是拿空名字去建文件
#[test]
fn new_needs_a_page_name() {
    assert!(fail(&["new"]).contains("页名"));
    match parse(&["new", "指南/入门"]).command {
        Command::New(new) => assert_eq!(new.name, "指南/入门"),
        other => panic!("要的是 new，给的是 {other:?}"),
    }
}

/// `build` 默认不清空、不较真 —— 那两件都是「改了行为」，得明写
#[test]
fn build_is_forgiving_by_default() {
    match parse(&["build"]).command {
        Command::Build(build) => {
            assert!(!build.clean);
            assert!(!build.strict);
        }
        other => panic!("要的是 build，给的是 {other:?}"),
    }
}

/// `serve` 有自己的选项：端口，以及听在哪个地址上
#[test]
fn serve_has_a_port_and_an_address() {
    let serve = match parse(&["serve"]).command {
        Command::Serve(serve) => serve,
        other => panic!("要的是 serve，给的是 {other:?}"),
    };
    assert_eq!(serve.port, 8080);
    assert_eq!(serve.host, "127.0.0.1");

    let serve = match parse(&["serve", "-p", "0", "--host", "0.0.0.0"]).command {
        Command::Serve(serve) => serve,
        other => panic!("要的是 serve，给的是 {other:?}"),
    };
    assert_eq!(serve.port, 0);
    assert_eq!(serve.host, "0.0.0.0");
}

/// 帮助里要能看见每个子命令 —— 少写一个变体，那个命令就没人知道了
#[test]
fn the_help_lists_every_subcommand() {
    let help = Cli::command().render_long_help().to_string();
    for name in ["render", "init", "new", "build", "serve"] {
        assert!(help.contains(name), "帮助里少了 {name}：\n{help}");
    }
}

/// `render` 的那些选项都搬过来了，一个都不能少 —— 少一个就是少一个功能
#[test]
fn the_render_help_lists_every_option() {
    let help = render_command().render_long_help().to_string();
    for flag in [
        "--full",
        "--title",
        "--theme",
        "--css",
        "--js",
        "--external-script",
        "--unsafe-html",
        "--typographer",
        "--setext",
        "--indented-code",
        "--no-math",
        "--no-wikilink",
        "--link-prefix",
        "--template-dir",
        "--list-blocks",
        "--stats",
    ] {
        assert!(help.contains(flag), "render 的帮助里少了 {flag}：\n{help}");
    }
}

/// `--help` 末尾要带块写法 —— 那是 `--help` 唯一的用法说明
#[test]
fn the_help_note_is_in_the_help_text() {
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains("::note"), "{help}");
    assert!(help.contains("缩进"), "{help}");
}

/// 帮助里不该混进英文 —— 那一半是 clap 写死的，用 [`HELP_TEMPLATE`] 与
/// `override_usage` 替掉。升级 clap 时这条会先红。
#[test]
fn the_help_is_chinese_throughout() {
    for help in [
        Cli::command().render_long_help().to_string(),
        render_command().render_long_help().to_string(),
    ] {
        for english in [
            "Usage:",
            "Options:",
            "Arguments:",
            "Commands:",
            "Print help",
            "Print version",
        ] {
            assert!(!help.contains(english), "帮助里混进了 {english}：\n{help}");
        }
        assert!(help.contains("用法："), "{help}");
        assert!(help.contains("显示帮助"), "{help}");
    }
    // 主题的取值写进了帮助正文，而不是留在 clap 那句英文的 possible values 里
    let help = render_command().render_long_help().to_string();
    assert!(help.contains("light") && help.contains("dark"), "{help}");
    assert!(!help.contains("possible values"), "{help}");
}

/// 取 `render` 那一个子命令的命令对象。
///
/// **先 `build()` 再取**：全局的 `-d/-h/-V` 是在 build 时递到子命令上的，
/// 没 build 就取到的是一棵没长全的树 —— 帮助里会少了那三个选项。
fn render_command() -> clap::Command {
    let mut command = Cli::command();
    command.build();
    command
        .get_subcommands()
        .find(|sub| sub.get_name() == "render")
        .expect("有 render 这个子命令")
        .clone()
}
