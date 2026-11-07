use std::path::PathBuf;

use clap::Args;

use super::help::HELP_TEMPLATE;

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
