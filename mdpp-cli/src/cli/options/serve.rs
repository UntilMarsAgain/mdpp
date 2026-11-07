use clap::Args;

use super::help::HELP_TEMPLATE_BARE;

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
