//! 命令行的参数：**子命令**与各自的选项。
//!
//! 命令分两拨：
//!
//! - **文件**（[`Render`]）：把一个或几个文件渲染成 HTML，往标准输出写。
//! - **项目**（[`Init`] / [`New`] / [`Build`]）：一个目录里有 `mdpp.toml`、`pages/`、
//!   `templates/` 与 `build/`，能整站出静态页面。
//!
//! `--debug` / `--help` / `--version` / `-C, --dir` 是**全局的**（`global = true`）——
//! 它们与「干什么」无关（`-C` 说的是**在哪儿干**），写在子命令上会让人以为
//! `mdpp -d build` 不行。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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
    help_template = TOP_HELP_TEMPLATE,
    override_usage = "mdpp <子命令> [选项]",
    disable_help_flag = true,
    disable_version_flag = true,
    // 全局的 `-V` 会被递到每个子命令上，子命令自己也得有个版本号可报，
    // 否则 `mdpp render -V` 与 `Cli::command().debug_assert()` 都会炸
    propagate_version = true,
    // `mdpp help build` 里那句说明是 clap 写死的英文，而 `-h` 在每个子命令上都管用
    disable_help_subcommand = true,
    subcommand_required = true,
    arg_required_else_help = true,
)]
pub struct Cli {
    /// 干什么
    #[command(subcommand)]
    pub command: Command,

    /// 启用 DEBUG 级日志（标准错误）
    #[arg(short, long, global = true)]
    pub debug: bool,

    // 字段名是 `workdir` 而不是 `dir` —— `init` 的**位置参数**占着 `dir` 这个 id，
    // 两个 id 撞了 `Cli::command().debug_assert()` 当场炸（见 the_command_definition_is_valid）。
    // 用 `//` 不用 `///`：doc 会进 `--help`，而这条是给改代码的人看的，不是给用的人
    /// 先切到这个目录再干活（`make -C` / `git -C` 那个 `-C`）
    ///
    /// 从那一刻起一切相对路径都从它算：往上找 `mdpp.toml` 的起点、`render` 的输入
    /// 文件与 `--css` / `--js`、`init` 不给目录时的那个「当前目录」。
    #[arg(short = 'C', long = "dir", global = true, value_name = "目录")]
    pub workdir: Option<PathBuf>,

    /// 显示帮助
    #[arg(short = 'h', long, action = clap::ArgAction::Help, global = true)]
    pub help: Option<bool>,

    /// 显示版本
    #[arg(short = 'V', long, action = clap::ArgAction::Version, global = true)]
    pub version: Option<bool>,
}

/// 子命令。名字就是命令行的那个词（`mdpp render`），因此这一层不加 `#[command(name)]`。
#[derive(Subcommand, Debug)]
pub enum Command {
    /// 渲染文件，HTML 写到标准输出
    Render(Render),

    /// 建一个新项目：mdpp.toml 与 pages/、templates/、build/
    Init(Init),

    /// 在 pages/ 下新建一页
    New(New),

    /// 把 pages/ 渲染成一个可静态部署的站点，写进 site_dir
    Build(Build),

    /// 本地预览这个站点：改一个字，浏览器自己刷新
    #[command(
        disable_help_flag = true,
        disable_version_flag = true,
        help_template = HELP_TEMPLATE_BARE,
        override_usage = "mdpp serve [选项]"
    )]
    Serve(Serve),
}

/// `mdpp render` —— 渲染文件。
///
/// 这一组选项**原样**是从过去的顶层命令搬过来的：一个都没改语义，只是换了个位置。
#[derive(Args, Debug)]
#[command(
    next_help_heading = "选项",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = HELP_TEMPLATE,
    override_usage = "mdpp render [选项] [文件]..."
)]
pub struct Render {
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
}

impl Render {
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

/// `mdpp init` —— 建项目骨架。
#[derive(Args, Debug)]
#[command(
    next_help_heading = "选项",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = HELP_TEMPLATE,
    override_usage = "mdpp init [选项] [目录]"
)]
pub struct Init {
    /// 项目目录；不给就是当前目录
    #[arg(value_name = "目录", help_heading = "参数")]
    pub dir: Option<PathBuf>,

    /// 站点名（写进 mdpp.toml 的 site_name）
    #[arg(long, value_name = "名字")]
    pub name: Option<String>,
}

/// `mdpp new` —— 新建一页。
#[derive(Args, Debug)]
#[command(
    next_help_heading = "选项",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = HELP_TEMPLATE,
    override_usage = "mdpp new [选项] <页名>"
)]
pub struct New {
    /// 页名；带 `指南/入门` 这样的子目录也行，目录会自动建
    #[arg(value_name = "页名", help_heading = "参数")]
    pub name: String,

    /// 这一页的标题（默认取页名的最后一段）
    #[arg(short, long, value_name = "标题")]
    pub title: Option<String>,
}

/// `mdpp serve` —— 本地预览。
///
/// 没有「哪个文件」这一说：它出的是**整个站点**，到临时目录里。
#[derive(Args, Debug)]
#[command(
    next_help_heading = "选项",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = HELP_TEMPLATE_BARE,
    override_usage = "mdpp serve [选项]"
)]
pub struct Serve {
    /// 端口；写 0 让系统挑一个空闲的
    #[arg(short, long, value_name = "端口", default_value_t = 8080)]
    pub port: u16,

    /// 听在哪个地址上。默认只听本机；要让容器外面的浏览器看得到，写 0.0.0.0
    #[arg(long, value_name = "地址", default_value = "127.0.0.1")]
    pub host: String,
}

/// `mdpp build` —— 整站。
#[derive(Args, Debug)]
#[command(
    next_help_heading = "选项",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = HELP_TEMPLATE_BARE,
    override_usage = "mdpp build [选项]"
)]
pub struct Build {
    /// 输出之前先清空 site_dir（默认不清 —— 清错了不好找回来）
    #[arg(long)]
    pub clean: bool,

    /// 有红链就算失败（退出码非零）
    #[arg(long)]
    pub strict: bool,
}

/// 顶层 `--help` 的版式。
///
/// `{options}` / `{positionals}` / `{subcommands}` **各自不带英文小标题**（`{all-args}`
/// 才带，而那一半是 clap 写死的）—— 所以「子命令：」这几个字得自己写上去。
///
/// 换 clap 版本时要复查这一段 —— 模板里的占位符是它的内部约定。
const TOP_HELP_TEMPLATE: &str = "\
{about-with-newline}
用法：{usage}

{options}

子命令：
{subcommands}{after-help}";

/// 子命令 `--help` 的版式：参数在前、选项在后，**没有子命令**。
///
/// 那几个占位符都不带行尾换行，所以两段之间的空行得写在模板里 —— 而 `{positionals}`
/// 为空时（`build` / `serve`）会多出两个空行，那种命令要用下面那份。
const HELP_TEMPLATE: &str = "\
{about-with-newline}
用法：{usage}

{positionals}

{options}{after-help}";

/// 没有位置参数的子命令用的那份（见 [`HELP_TEMPLATE`] 为什么非得有两份）
const HELP_TEMPLATE_BARE: &str = "\
{about-with-newline}
用法：{usage}

{options}{after-help}";

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

    /// 主题只认两个值：写错时要在**解析时**就说，而不是渲染到一半才发现
    #[test]
    fn only_light_and_dark_are_themes() {
        assert_eq!(
            render(&["render", "--theme", "dark"]).theme.as_deref(),
            Some("dark")
        );
        let message = fail(&["render", "--theme", "auto"]);
        assert!(
            message.contains("light") && message.contains("dark"),
            "{message}"
        );
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
}
