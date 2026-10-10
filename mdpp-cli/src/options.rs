//! 命令行的参数。

use std::path::PathBuf;

use clap::Parser;

/// 命令名（错误信息里用它）
pub const NAME: &str = "mdpp";

/// 把 Markdown++ 渲染成 HTML
#[derive(Parser, Debug)]
#[command(
    name = "mdpp",
    version,
    about,
    long_about = None,
    after_help = HELP_NOTE,
    next_help_heading = "选项",
    help_template = HELP_TEMPLATE,
    override_usage = "mdpp [选项] [文件]...",
    disable_help_flag = true,
    disable_version_flag = true,
)]
pub struct Cli {
    /// 要渲染的文件；不给则从标准输入读
    #[arg(value_name = "文件", help_heading = "参数")]
    pub inputs: Vec<PathBuf>,

    /// 输出完整 HTML 文档（内联默认样式与脚本），而不是片段
    #[arg(short, long)]
    pub full: bool,

    /// 完整文档的 <title>（默认取第一个文件名）
    #[arg(short, long, value_name = "标题")]
    pub title: Option<String>,

    /// 完整文档的主题（light 或 dark）；::theme 的两节由它挑
    #[arg(long, value_name = "主题", value_parser = ["light", "dark"], hide_possible_values = true)]
    pub theme: Option<String>,

    /// 附加 CSS，可多次；按给定次序接在默认样式之后
    #[arg(long, value_name = "文件", requires = "full")]
    pub css: Vec<PathBuf>,

    /// 附加 JS，可多次；按给定次序接在默认脚本之后
    ///
    /// 默认脚本只做「点一下就切选项卡」。方向键、动画、记忆选中项属于调用方。
    #[arg(long, value_name = "文件", requires = "full")]
    pub js: Vec<PathBuf>,

    /// 允许正文中的原始 HTML（默认转义）
    #[arg(short = 'u', long)]
    pub unsafe_html: bool,

    /// 启用排版替换（-- 变 –，直引号变弯引号）
    #[arg(long)]
    pub typographer: bool,

    /// 启用 Setext 标题（标题下一行写 ===）
    #[arg(long)]
    pub setext: bool,

    /// 启用缩进代码块（四空格开头）
    #[arg(long)]
    pub indented_code: bool,

    /// 关闭行内公式 $…$
    #[arg(long)]
    pub no_math: bool,

    /// 关闭内部链接 [[…]]
    #[arg(long)]
    pub no_wikilink: bool,

    /// [[目标]] 的 href 前缀
    #[arg(short = 'p', long, value_name = "前缀")]
    pub link_prefix: Option<String>,

    /// 模板页目录；<目录>/<名字>.mdpp 是一页，::名字 从这里取
    #[arg(short = 'T', long, value_name = "目录")]
    pub template_dir: Option<PathBuf>,

    /// 列出内置块名后退出
    #[arg(short = 'L', long)]
    pub list_blocks: bool,

    /// 把统计写到标准错误
    #[arg(short, long)]
    pub stats: bool,

    /// 启用 DEBUG 级日志（标准错误）
    #[arg(short, long)]
    pub debug: bool,

    /// 显示帮助
    #[arg(short = 'h', long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,

    /// 显示版本
    #[arg(short = 'V', long, action = clap::ArgAction::Version)]
    pub version: Option<bool>,
}

impl Cli {
    /// `<title>`：没给就用第一个输入的文件名；再没有就叫「未命名」。
    /// 不能是空串 —— 那会渲染出一个空的 `<title>`。
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

/// `--help` 的版式。clap 把 `Usage:` / `Options:` / `Print help` 写死成英文，
/// 用 `override_usage` 与这个模板替掉；参数表的内容仍由 clap 排。
///
/// 换 clap 版本时要复查这一段 —— 模板里的占位符是它的内部约定。
const HELP_TEMPLATE: &str = "\
{about-with-newline}
用法：{usage}

{all-args}{after-help}";

/// `--help` 末尾那一段：块怎么写。
///
/// 挂在 `after_help` 上，而不是在 `main` 里另打一遍 —— 输出通道是 **stdout**，
/// 而 stdout 是**产物**（HTML）要去的地方。
const HELP_NOTE: &str = "\
块的内容按缩进划边界，遇到不更深的行就结束：

    ::note title=\"提醒\"
      这里可以写 **markdown**，也可以写 [[内部链接]]。

行内公式是 $E = mc^2$，多行的用 ::math 块。

全部块名与示例见仓库的 examples/，其中 tour.mdpp 是一个块的巡览。";

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

    /// 帮助里不该混进英文 —— 那一半是 clap 写死的，用 [`HELP_TEMPLATE`] 与
    /// `override_usage` 替掉。升级 clap 时这条会先红。
    #[test]
    fn the_help_is_chinese_throughout() {
        let help = Cli::command().render_long_help().to_string();
        for english in [
            "Usage:",
            "Options:",
            "Arguments:",
            "Print help",
            "Print version",
        ] {
            assert!(!help.contains(english), "帮助里混进了 {english}：\n{help}");
        }
        assert!(help.contains("用法："), "{help}");
        assert!(help.contains("显示帮助"), "{help}");
        // 主题的取值写进了帮助正文，而不是留在 clap 那句英文的 possible values 里
        assert!(help.contains("light") && help.contains("dark"), "{help}");
        assert!(!help.contains("possible values"), "{help}");
    }
}
