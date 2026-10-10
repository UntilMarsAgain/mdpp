//! 命令行的参数。
//!
//! ## 为什么现在用 clap
//!
//! 之前是手写的，那时理由是「这条命令行要跟着库走，加一个参数解析器就多一份要跟
//! 着版本升级的依赖」。拆成两个包之后那条理由**不成立**了 —— `clap` 是
//! `mdpp-cli` 自己的依赖，库那边一点也看不到它。
//!
//! 手写那三百行要自己做的事，clap 都做了：`--opt=值` 与 `--opt 值` 两种写法、
//! `-abc` 组合短选项、`--` 之后一律是文件名、拼错的选项报得像个人话、`--help` 与
//! `--version`、以及错误时给一句「你是不是想写 `--`。而这些在测试里都得一条条
//! 补 —— 补一遍就是几百行断言。

use std::path::PathBuf;

use clap::Parser;

/// 命令名（错误信息里用它）
pub const NAME: &str = "mdpp";

/// 把 Markdown++ 渲染成 HTML
///
/// 不给文件就从标准输入读。给多个文件就依次渲染，每个文件一段。
#[derive(Parser, Debug)]
#[command(
    name = "mdpp",
    version,
    about,
    long_about = None,
    after_help = HELP_NOTE,
)]
pub struct Cli {
    /// 要渲染的文件
    #[arg(value_name = "文件")]
    pub inputs: Vec<PathBuf>,

    /// 输出完整 HTML 文档（内联默认样式与默认脚本）而不是片段
    #[arg(short, long)]
    pub full: bool,

    /// 完整文档的 <title>（默认取第一个文件名）
    #[arg(short, long, value_name = "标题")]
    pub title: Option<String>,

    /// 完整文档写死主题，::theme 的两节由它挑
    #[arg(long, value_name = "light|dark", value_parser = ["light", "dark"])]
    pub theme: Option<String>,

    /// 附带自己的 CSS（可以给多次，按给的次序接在默认那份后面）
    #[arg(long, value_name = "文件", requires = "full")]
    pub css: Vec<PathBuf>,

    /// 附带自己的 JS（可以给多次，按给的次序接在默认那段后面）
    ///
    /// 默认那段只做「点一下就切选项卡」这一件事 —— 最小可用的那份。
    /// 方向键、动画、记住上次选中的那一栏这些属于各自的界面，由这里接进来。
    #[arg(long, value_name = "文件", requires = "full")]
    pub js: Vec<PathBuf>,

    /// 允许正文里的 raw HTML（默认转义）
    #[arg(short = 'u', long)]
    pub unsafe_html: bool,

    /// 打开排版替换（-- 变 –，直引号变弯引号）
    #[arg(long)]
    pub typographer: bool,

    /// 打开 Setext 标题（标题下一行写 ===）
    #[arg(long)]
    pub setext: bool,

    /// 打开缩进代码块（四空格开头）
    #[arg(long)]
    pub indented_code: bool,

    /// 关掉 $…$
    #[arg(long)]
    pub no_math: bool,

    /// 关掉 [[内部链接]]
    #[arg(long)]
    pub no_wikilink: bool,

    /// 给 [[目标]] 配上 <前缀>目标 的 href
    #[arg(short = 'p', long, value_name = "前缀")]
    pub link_prefix: Option<String>,

    /// 模板页目录：<目录>/<名字>.mdpp 就是那一页，于是 ::名字 能取到它
    #[arg(short = 'T', long, value_name = "目录")]
    pub template_dir: Option<PathBuf>,

    /// 列出这一版带的块名，然后退出
    #[arg(short = 'L', long)]
    pub list_blocks: bool,

    /// 把统计写到标准错误
    #[arg(short, long)]
    pub stats: bool,

    /// 装一个 DEBUG 级的日志订阅器（写标准错误）
    #[arg(short, long)]
    pub debug: bool,
}

impl Cli {
    /// `<title>`：没给就用第一个输入的文件名；再没有就叫「未命名」
    ///
    /// 不能是空串：那会渲染出一个空的 `<title>`。
    pub fn title_or(&self) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        self.inputs
            .first()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名".to_string())
    }
}

/// `--help` 末尾那一段：块怎么写。
///
/// 挂在 `after_help` 上，而不是在 `main` 里另打一遍 —— 输出通道是 **stdout**，
/// 而 stdout 是**产物**（HTML）要去的地方。往那里打一行说明，等于让每一份输出都
/// 前缀一段不相干的话（这个坑踩过一次）。
const HELP_NOTE: &str = "\
块写法（块的内容按缩进划边界，遇到不更深的行就结束）：

    ::note title=\"提醒\"
      这里可以写 **markdown**，也可以写 [[内部链接]]。

  行内公式是 $E = mc^2$，多行的用 ::math。

例子见仓库里的 examples/，那里有一份把全部块都过一遍的 tour.mdpp。";

#[cfg(test)]
mod tests {
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

    /// derive 出来的命令定义自洽 —— 少一个 `id`、写重一个短选项，clap 会在启动时炸。
    /// 这一条让那种炸法在 `cargo test` 里就发生。
    #[test]
    fn the_command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_means_standard_input() {
        let cli = parse(&[]);
        assert!(cli.inputs.is_empty());
        assert!(!cli.full);
    }

    #[test]
    fn files_land_in_order() {
        let cli = parse(&["甲.md", "乙.md"]);
        assert_eq!(cli.inputs.len(), 2);
        assert!(cli.inputs[0].ends_with("甲.md"));
        assert!(cli.inputs[1].ends_with("乙.md"));
    }

    #[test]
    fn short_and_long_forms_both_work() {
        assert!(parse(&["-f"]).full);
        assert!(parse(&["--full"]).full);
        assert!(parse(&["--list-blocks"]).list_blocks);
        assert!(parse(&["-L"]).list_blocks);
        assert!(parse(&["--stats"]).stats);
        assert!(parse(&["--debug"]).debug);
        assert!(parse(&["-d"]).debug);
        assert!(parse(&["--unsafe-html"]).unsafe_html);
        assert!(parse(&["-u"]).unsafe_html);
    }

    #[test]
    fn values_can_follow_or_use_equals() {
        assert_eq!(parse(&["--title", "某篇"]).title.as_deref(), Some("某篇"));
        assert_eq!(parse(&["--title=某篇"]).title.as_deref(), Some("某篇"));
    }

    #[test]
    fn the_title_defaults_to_the_first_file_name() {
        assert_eq!(parse(&["甲.md"]).title_or(), "甲.md");
        assert_eq!(parse(&["/a/b/乙.md", "丙.md"]).title_or(), "乙.md");
        assert_eq!(parse(&["--title", "卷首", "甲.md"]).title_or(), "卷首");
        assert_eq!(parse(&[]).title_or(), "未命名");
    }

    /// 主题只认两个值：写错时要在**解析时**就说，而不是渲染到一半才发现
    #[test]
    fn only_light_and_dark_are_themes() {
        assert_eq!(parse(&["--theme", "dark"]).theme.as_deref(), Some("dark"));
        let message = fail(&["--theme", "auto"]);
        assert!(
            message.contains("light") && message.contains("dark"),
            "{message}"
        );
    }

    /// `--css` / `--js` 可以给多次，按给的次序
    #[test]
    fn extra_files_are_collected_in_order() {
        let cli = parse(&["--full", "--css", "a.css", "--css", "b.css", "--js", "x.js"]);
        assert_eq!(cli.css.len(), 2);
        assert_eq!(cli.js.len(), 1);
        assert!(cli.css[0].ends_with("a.css"));
        assert!(cli.css[1].ends_with("b.css"));
    }

    /// 片段里没有地方放这些东西，所以给了 `--css` 却不给 `--full` 是个用法错误。
    /// 静默忽略才是坑 —— 人会以为自己的样式进去了。
    #[test]
    fn attaching_files_needs_the_full_mode() {
        let message = fail(&["--css", "a.css"]);
        assert!(message.contains("--full"), "{message}");
        let message = fail(&["--js", "x.js"]);
        assert!(message.contains("--full"), "{message}");
        // 要有个别的东西（`-`）也不能替代 `--full`
        // 给了 --full 就不报
        assert_eq!(parse(&["--full", "--css", "a.css"]).css.len(), 1);
    }

    /// 一个不认的选项要报得像个人话，并带上「你是不是想写什么」
    #[test]
    fn an_unknown_option_is_refused_by_name() {
        let message = fail(&["--nope"]);
        assert!(message.contains("--nope"), "{message}");
    }

    /// 缺值要说清楚，不能默默当成空串 —— 那会渲染出一个空的 title。
    ///
    /// 断言的是**报的是什么选项**，不是报的那句话长什么样：clap 的错误信息是英文的
    /// （本地化它是另一件事，不该在换解析器的时候顺手塞进来）。
    #[test]
    fn a_missing_value_is_a_usage_error() {
        let message = fail(&["--title"]);
        assert!(message.contains("--title"), "{message}");
    }

    /// `--` 之后的一律是文件名：路径以 `-` 开头的那种
    #[test]
    fn everything_after_a_bare_double_dash_is_a_file_name() {
        let cli = parse(&["--", "--full"]);
        assert_eq!(cli.inputs.len(), 1);
        assert_eq!(cli.inputs[0].to_string_lossy(), "--full");
        assert!(!cli.full, "`--` 之后那个不是选项");
    }

    /// `--help` 末尾要带块写法 —— 那是 `--help` 唯一的用法说明
    #[test]
    fn the_help_note_is_in_the_help_text() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("::note"), "{help}");
        assert!(help.contains("缩进"), "{help}");
    }

    /// 帮助里要能看见每个选项 —— 少写一个 `#[arg]` 就少一行，而那行是唯一的文档
    #[test]
    fn the_help_lists_every_option() {
        let help = Cli::command().render_long_help().to_string();
        for flag in [
            "--full",
            "--title",
            "--theme",
            "--css",
            "--js",
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
            "--debug",
        ] {
            assert!(help.contains(flag), "帮助里少了 {flag}");
        }
    }
}
