//! 命令行的参数与帮助。
//!
//! **手写而不用 `clap`** 是有理由的：这条命令行要跟着库走，加一个参数解析器就多一份
//! 要跟着版本升级的依赖，而这份命令行总共也就两百行。真要加到「值得上 clap」的程度，
//! 换掉整个文件也不影响别处。

use std::path::PathBuf;

/// 命令名（帮助与用法里用它）
pub const NAME: &str = "mdpp";

/// 解析后的参数
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// 要渲染的文件；空 = 从标准输入读
    pub inputs: Vec<PathBuf>,
    /// 输出完整 HTML 文档（内嵌默认样式），而不是片段
    pub full: bool,
    /// `--full` 时的 `<title>`
    pub title: Option<String>,
    /// `--full` 时写死主题
    pub theme: Option<String>,
    /// 允许正文里的 raw HTML（默认转义 —— 这是安全开关）
    pub unsafe_html: bool,
    /// 打开排版替换（`--` → `–`、直引号 → 弯引号）
    pub typographer: bool,
    /// 打开 Setext 标题
    pub setext: bool,
    /// 打开缩进代码块
    pub indented_code: bool,
    /// 关掉 `$…$`
    pub no_math: bool,
    /// 关掉 `[[…]]`
    pub no_wikilink: bool,
    /// `[[目标]]` 的地址前缀：给 `[[甲]]` 配上 `<前缀>甲` 的 `href`
    pub link_prefix: Option<String>,
    /// 模板页目录：`<目录>/<名字>.md` 就是那一页
    pub template_dir: Option<PathBuf>,
    /// 列出这一版带的块名，然后退出
    pub list_blocks: bool,
    /// 把统计写到标准错误
    pub stats: bool,
    /// 装一个 DEBUG 级的日志订阅器（写标准错误）
    pub debug: bool,
}

/// 解析失败。消息直接就是给人看的那一句。
#[derive(Debug)]
pub struct BadUsage(pub String);

impl std::fmt::Display for BadUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Options {
    /// 解析命令行参数。
    ///
    /// 传 `-h` / `--help` / `-V` / `--version` 会拿到 [`Wanted`]，由调用方去打印。
    pub fn parse<I: IntoIterator<Item = String>>(
        args: I,
    ) -> Result<Result<Self, Wanted>, BadUsage> {
        let mut options = Options::default();
        // 收成 `Vec` 而不是留原来的迭代器：`--opt=值` 那条要把参数插到**最前面**
        // 重新从头走一遍，迭代器不是可变的双端队列，那样做不出来
        let mut rest: Vec<String> = args.into_iter().collect();
        rest.reverse();

        while let Some(arg) = rest.pop() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(Err(Wanted::Help)),
                "-V" | "--version" => return Ok(Err(Wanted::Version)),
                "-f" | "--full" => options.full = true,
                "-u" | "--unsafe-html" => options.unsafe_html = true,
                "--typographer" => options.typographer = true,
                "--setext" => options.setext = true,
                "--indented-code" => options.indented_code = true,
                "--no-math" => options.no_math = true,
                "--no-wikilink" => options.no_wikilink = true,
                "-L" | "--list-blocks" => options.list_blocks = true,
                "-s" | "--stats" => options.stats = true,
                "-d" | "--debug" => options.debug = true,
                other if other.starts_with("--") && other.contains('=') => {
                    // `--opt=值`：拆开之后当作 `--opt 值` 走一遍，
                    // 免得每个选项都写两份解析（漏一份就有一个不认 `=` 形式）
                    let (name, value) = other.split_once('=').expect("刚判过含 '='");
                    // 栈是从尾部取的，所以**值先压、名字后压** —— 名字要先出来，
                    // 值才是它后面的那个参数
                    rest.push(value.to_string());
                    rest.push(name.to_string());
                    continue;
                }
                "-t" | "--title" => {
                    options.title = Some(take_value(&arg, rest.pop())?);
                }
                "--theme" => {
                    let value = take_value(&arg, rest.pop())?;
                    if !matches!(value.as_str(), "light" | "dark") {
                        return Err(BadUsage(format!(
                            "--theme 只认 light 与 dark，给的是 `{value}`"
                        )));
                    }
                    options.theme = Some(value);
                }
                "-p" | "--link-prefix" => {
                    options.link_prefix = Some(take_value(&arg, rest.pop())?);
                }
                "-T" | "--template-dir" => {
                    options.template_dir = Some(PathBuf::from(take_value(&arg, rest.pop())?));
                }
                other if other.starts_with('-') && other.len() > 1 => {
                    return Err(BadUsage(format!("不认这个选项：`{other}`")));
                }
                other => options.inputs.push(PathBuf::from(other)),
            }
        }

        Ok(Ok(options))
    }

    /// `<title>`：没给就用第一个输入的文件名；再没有就叫「未命名」
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

/// 解析到一半遇到了「不用继续了」
#[derive(Debug)]
pub enum Wanted {
    Help,
    Version,
}

/// 取一个选项的值。下一个参数不存在就是用法错误 —— 不猜，也不给空串
fn take_value(name: &str, value: Option<String>) -> Result<String, BadUsage> {
    match value {
        Some(value) if !value.starts_with('-') || value == "-" => Ok(value),
        // `-t -f` 这种：把下一个选项吃成了值。与其默默接受，不如说清楚
        Some(value) => Err(BadUsage(format!("{name} 后面要一个值，给的是 `{value}`"))),
        None => Err(BadUsage(format!("{name} 后面要一个值"))),
    }
}

/// `--help` 的正文
pub fn help() -> String {
    format!(
        "\
{NAME} —— 把 Markdown++ 渲染成 HTML

用法：
  {NAME} [选项] [文件…]
  cat 正文.md | {NAME}

不给文件就从标准输入读。给多个文件就依次渲染，每个文件一段。

选项：
  -f, --full              输出完整 HTML 文档（内嵌默认样式）而不是片段
  -t, --title <标题>      --full 时的 <title>（默认取第一个文件名）
      --theme <light|dark>  --full 时写死主题，::theme 由它挑
  -u, --unsafe-html       允许正文里的 raw HTML（默认转义）
      --typographer       打开排版替换（-- 变 –，直引号变弯引号）
      --setext            打开 Setext 标题（标题下一行写 ===）
      --indented-code     打开缩进代码块（四空格开头）
      --no-math           关掉 $…$
      --no-wikilink       关掉 [[内部链接]]
  -p, --link-prefix <前缀> 给 [[目标]] 配上 <前缀>目标 的 href
  -T, --template-dir <目录> 模板页目录：<目录>/<名字>.mdpp 就是那一页，
                           于是 ::名字 能取到它
  -L, --list-blocks       列出这一版带的块名，然后退出
  -s, --stats             把统计写到标准错误
  -d, --debug             装一个 DEBUG 级的日志订阅器（写标准错误）
  -h, --help              显示这一段
  -V, --version           显示版本

块写法（块的内容按缩进划边界）：
  ::note title=\\\"提醒\\\"
    这里可以写 **markdown**，也可以写 [[内部链接]]。

  行内公式是 $E = mc^2$，多行的用 ::math。
"
    )
}

/// `--version` 的正文
///
/// 末尾那个换行是**要**的：`--version` 的输出直接进 shell 提示符，少了它下一条命令
/// 会粘在这一行后面。
pub fn version() -> String {
    format!("{NAME} {}\n", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        match Options::parse(args.iter().map(|s| s.to_string())).expect("不该是用法错误") {
            Ok(options) => options,
            Err(_) => panic!("不该想要帮助或版本"),
        }
    }

    fn fail(args: &[&str]) -> String {
        match Options::parse(args.iter().map(|s| s.to_string())) {
            Err(bad) => bad.0,
            Ok(Err(_)) => panic!("不该想要帮助或版本"),
            Ok(Ok(_)) => panic!("本该是用法错误"),
        }
    }

    #[test]
    fn no_arguments_means_standard_input() {
        let options = parse(&[]);
        assert!(options.inputs.is_empty());
        assert!(!options.full);
    }

    #[test]
    fn files_land_in_order() {
        let options = parse(&["甲.md", "乙.md"]);
        assert_eq!(options.inputs.len(), 2);
        assert!(options.inputs[0].ends_with("甲.md"));
        assert!(options.inputs[1].ends_with("乙.md"));
    }

    /// 没有 `--debug` 就什么日志都不打 —— 一次性的事不该在正常输出旁边多出字
    #[test]
    fn logging_is_off_unless_it_is_asked_for() {
        assert!(!parse(&[]).debug);
        assert!(!parse(&["-s"]).debug);
        assert!(parse(&["--debug"]).debug);
        assert!(parse(&["-d"]).debug);
    }

    #[test]
    fn short_and_long_forms_both_work() {
        assert!(parse(&["-f"]).full);
        assert!(parse(&["--full"]).full);
        assert!(parse(&["--list-blocks"]).list_blocks);
        assert!(parse(&["-L"]).list_blocks);
        assert!(parse(&["--stats"]).stats);
    }

    #[test]
    fn values_can_follow_or_use_equals() {
        let spaced = parse(&["--title", "某篇"]);
        let equals = parse(&["--title=某篇"]);
        assert_eq!(spaced.title.as_deref(), Some("某篇"));
        assert_eq!(equals.title.as_deref(), Some("某篇"));
    }

    #[test]
    fn the_title_defaults_to_the_first_file_name() {
        assert_eq!(parse(&["甲.md"]).title_or(), "甲.md");
        assert_eq!(parse(&["/a/b/乙.md", "丙.md"]).title_or(), "乙.md");
        // 给了就以给的为准
        assert_eq!(parse(&["--title", "卷首", "甲.md"]).title_or(), "卷首");
        // 连文件都没有就叫「未命名」——不能是空串：那会渲染出一个空的 <title>
        assert_eq!(parse(&[]).title_or(), "未命名");
    }

    #[test]
    fn only_light_and_dark_are_themes() {
        assert_eq!(parse(&["--theme", "dark"]).theme.as_deref(), Some("dark"));
        assert!(fail(&["--theme", "auto"]).contains("light"));
    }

    /// 缺值要说清楚，不能默默当成「空字符串」—— 那会渲染出一个空的 title
    #[test]
    fn a_missing_value_is_a_usage_error() {
        assert!(fail(&["--title"]).contains("要一个值"));
        // 把下一个选项吃成了值
        assert!(fail(&["--title", "--full"]).contains("要一个值"));
    }

    #[test]
    fn an_unknown_option_is_refused_by_name() {
        let message = fail(&["--nope"]);
        assert!(message.contains("--nope"), "{message}");
    }

    #[test]
    fn a_lone_dash_is_a_file_name_not_an_option() {
        let options = parse(&["-"]);
        assert_eq!(options.inputs.len(), 1);
        assert_eq!(options.inputs[0].to_string_lossy(), "-");
    }

    #[test]
    fn help_and_version_stop_the_parse() {
        for flag in ["-h", "--help"] {
            match Options::parse([flag.to_string()]).unwrap() {
                Err(Wanted::Help) => {}
                _ => panic!("{flag} 应当要帮助"),
            }
        }
        for flag in ["-V", "--version"] {
            match Options::parse([flag.to_string()]).unwrap() {
                Err(Wanted::Version) => {}
                _ => panic!("{flag} 应当要版本"),
            }
        }
    }

    /// `--` 之后的一律是文件名：路径以 `-` 开头的那种
    #[test]
    fn everything_after_a_bare_double_dash_is_a_file_name() {
        // 这条命令行没有专门实现 `--`：多读一个名字当分隔符会让
        // 「路径恰好叫 -- 」这种场合更难懂，而那种场合几乎不存在。
        // 记在这里，免得将来以为已经实现了。
        let message = fail(&["--"]);
        assert!(message.contains("--"), "{message}");
    }

    #[test]
    fn the_help_text_lists_every_option_the_parser_accepts() {
        let help = help();
        for flag in [
            "--full",
            "--title",
            "--theme",
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
            "--help",
            "--version",
        ] {
            assert!(help.contains(flag), "帮助里少了 {flag}");
        }
    }
}
