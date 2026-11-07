use clap::Args;

use super::help::HELP_TEMPLATE_BARE;

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
    /// 并行渲染的线程数；0 = 按 CPU 核数，1 = 单线程
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub jobs: usize,

    /// 保留兼容：`build` 现在总是整目录替换（失败时不动旧站点），这个开关不再改变行为
    #[arg(long)]
    pub clean: bool,

    /// 有红链就算失败（退出码非零）
    #[arg(long)]
    pub strict: bool,

    /// mermaid / math / 代码高亮 前端脚本的加载方式：cdn（默认，从 CDN 引入）、none（不加载）。
    /// 页面里若用了 ::mermaid / ::math 或写了代码块，才会把对应的资源写进 HTML。
    /// 代码高亮的 JS 连同主题（github 浅色 / 深色，随系统深浅自动挑一份）一并引入；
    /// `--css` 附加的样式排在主题之后，用自己的配色还能换掉它。
    #[arg(
        long,
        value_name = "方式",
        value_parser = ["cdn", "none"],
        default_value = "cdn",
        hide_possible_values = true
    )]
    pub external_script: String,

    /// 内部开关：允许清空项目外的目录。
    ///
    /// 只有 `serve` 用得到 —— 它出到系统临时目录里的预览缓存。命令行上**不暴露**，
    /// 于是 `mdpp build --clean` 永远删不到项目外面。
    #[arg(skip)]
    pub allow_outside: bool,
}
