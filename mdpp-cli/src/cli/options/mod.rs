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

mod build;
mod help;
mod project;
mod render;
mod serve;

pub use build::Build;
pub use project::{Init, New};
pub use render::Render;
pub use serve::Serve;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use help::{HELP_NOTE, HELP_TEMPLATE_BARE, TOP_HELP_TEMPLATE};

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

#[cfg(test)]
mod tests;
