/// 顶层 `--help` 的版式。
///
/// `{options}` / `{positionals}` / `{subcommands}` **各自不带英文小标题**（`{all-args}`
/// 才带，而那一半是 clap 写死的）—— 所以「子命令：」这几个字得自己写上去。
///
/// 换 clap 版本时要复查这一段 —— 模板里的占位符是它的内部约定。
pub(super) const TOP_HELP_TEMPLATE: &str = "\
{about-with-newline}
用法：{usage}

{options}

子命令：
{subcommands}{after-help}";

/// 子命令 `--help` 的版式：参数在前、选项在后，**没有子命令**。
///
/// 那几个占位符都不带行尾换行，所以两段之间的空行得写在模板里 —— 而 `{positionals}`
/// 为空时（`build` / `serve`）会多出两个空行，那种命令要用下面那份。
pub(super) const HELP_TEMPLATE: &str = "\
{about-with-newline}
用法：{usage}

{positionals}

{options}{after-help}";

/// 没有位置参数的子命令用的那份（见 [`HELP_TEMPLATE`] 为什么非得有两份）
pub(super) const HELP_TEMPLATE_BARE: &str = "\
{about-with-newline}
用法：{usage}

{options}{after-help}";

/// `--help` 末尾那一段：块怎么写。
///
/// 挂在 `after_help` 上，而不是在 `main` 里另打一遍 —— 输出通道是 **stdout**，
/// 而 stdout 是**产物**（HTML）要去的地方。
pub(super) const HELP_NOTE: &str = "\
块的内容按缩进划边界，遇到不更深的行就结束：

    ::note title=\"提醒\"
      这里可以写 **markdown**，也可以写 [[内部链接]]。

行内公式是 $E = mc^2$，多行的用 ::math 块。

全部块名与示例见仓库的 examples/，其中 tour.mdpp 是一个块的巡览。";
