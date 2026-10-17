// mdpp —— Markdown++ 的命令行
//
// 输出默认是**片段**（像 `cmark` 那样）：那段 HTML 往哪去、外面套什么样式，是调用方的
// 事。`render --full` 才输出一个完整文档（内嵌本包那份默认样式表）。

// 这些模块只属于这个二进制，库那边一概不知道它们存在 —— 所以它们住在 `mdpp-cli`
// 这个包里，并且对 `mdpp::` 不可见（`pub(crate)`）。
mod color;
mod context;
mod http;
mod options;
mod page;
mod project;
mod render;
mod telemetry;
mod url;

use options::{Cli, Command, NAME};
use telemetry::install;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    // clap 自己处理 `--help` / `--version` / 用法错误（后者走退出码 2，
    // 与「跑起来了但失败」的 1 区分开 —— cmark 也是这么分的）
    let cli = match Cli::try_parse() {
        // 注意这里**什么都不往 stdout 打**：stdout 是产物要去的地方
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayVersion
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    ExitCode::SUCCESS
                }
                _ => ExitCode::from(2),
            };
        }
    };

    // `-C, --dir`：先把进程站到那儿 —— 从这一刻起**一切相对路径**（往上找 mdpp.toml
    // 的起点、`render` 的输入与附件、`init` 不给目录时的「当前目录」）都从它算，
    // 于是下面每个子命令一行都不用改。
    //
    // **只在这里切一次**，且切在解析之后、分发之前：各命令自己不碰进程当前目录 ——
    // 测试是并发的，函数里动它会互相串味（这条规矩见 `new_page::create` 的文档）
    if let Some(dir) = &cli.workdir {
        if let Err(error) = std::env::set_current_dir(dir) {
            // 1（跑起来了但失败），不是 2：参数是合法的，是那个目录进不去
            eprintln!("{NAME}: -C 进不去 {}：{error}", dir.display());
            return ExitCode::FAILURE;
        }
    }

    // 订阅器在解析之后才装：否则连「这组参数是不是错的」都还没定，就已经打了一堆日志
    install(cli.debug);

    // 每个子命令自己决定「成功」是什么形状，返回一句**给人看的**失败原因
    let outcome = match &cli.command {
        Command::Render(options) => render::run(options),
        Command::Init(options) => project::init::run(options),
        Command::New(options) => project::new_page::run(options),
        Command::Build(options) => project::build::run(options),
        Command::Serve(options) => project::serve::run(options),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{NAME}: {message}");
            ExitCode::FAILURE
        }
    }
}
