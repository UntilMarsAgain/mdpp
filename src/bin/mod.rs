//! 命令行自己的那些零件 —— **只**被 [`crate`]（也就是 `src/main.rs`）用到，
//! 库那边一概不知道它们存在。
//!
//! 放在 `src/bin/` 而不是散在 `src/` 根下，是因为混进去会让人以为库也用得上：
//! `src/` 根目录是**两个 target 共用**的，而这里的三个文件只属于二进制那一个。
//!
//! [`crate`] 指回本 crate 的根（`src/main.rs`）。这里的东西全是 `pub(crate)`：
//! 它们不构成库的接口，`mdpp::` 下面看不到任何一个。

mod context;
mod options;
mod telemetry;

pub(crate) use context::{link_resolver, template_pages};
pub(crate) use options::{help, version, BadUsage, Options, Wanted, NAME};
pub(crate) use telemetry::install;

pub(crate) use tracing::{debug, info};
