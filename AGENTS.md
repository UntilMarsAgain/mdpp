# AGENTS.md

给 AI 编程助手的说明。人类用户请看 [README.md](README.md)。

## 项目

mdpp 是 Markdown++ 渲染器：markdown-it 之上加 `::名字 参数=值` 块语法、
`[[站内链接]]`、模板页与资源收集。

- `mdpp/`：可发布库，只输出 HTML 片段，不碰文件系统、终端或项目。
- `mdpp-cli/`：命令行（`render` / `init` / `new` / `build` / `serve`），不发布。

版本 `0.1.0`，Rust edition 2024，MSRV 1.85。**目前没有 CI。**

## 常用命令

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p mdpp --no-deps
cargo package -p mdpp --allow-dirty --no-verify

cargo run -p mdpp-cli -- render examples/tour.mdpp
cargo run -p mdpp-cli -- render --full examples/tour.mdpp > /tmp/tour.html
```

stdout 只放产物，日志和错误走 stderr。CLI 参数测试在 `mdpp-cli/src/cli/options/tests.rs`，
端到端测试在 `mdpp-cli/tests/`。

## 先看哪里

| 要改什么 | 入口 |
|---|---|
| 库的公开 API | `mdpp/src/lib.rs` |
| 解析与渲染 | `mdpp/src/runtime/engine/` |
| 块语法、分发、内置块 | `mdpp/src/block/` |
| 命令行子命令 | `mdpp-cli/src/cli/options/` |
| 整站构建 | `mdpp-cli/src/project/build/` |
| 本地预览 | `mdpp-cli/src/project/serve/` 与 `server/http.rs` |
| 页面外壳 | `mdpp-cli/src/render/page/` 与 `assets/site.html` |
| 前端样式与脚本 | `mdpp-cli/assets/` |

各目录的 `//!` 文档已经写了职责与设计理由，先读最近的模块文档，再全库搜索。

## 硬约束

- 注释、提交信息、文档用中文；标识符用英文。
- 库根部的公开路径必须稳定：`mdpp::engine`、`mdpp::context`、`mdpp::raw`、
  `mdpp::registry`、`mdpp::assets` 等通过 `lib.rs` 重导出，不随物理目录变化。
  `mdpp::state` 保持 crate-private。
- `mdpp` 与 `mdpp-cli` 都 `forbid(unsafe_code)`；库 `deny(missing_docs)`。
- `Mdpp` 必须保持 `Send + Sync`；每次渲染的可变状态放线程局部，不放解析器。
- `mdpp-cli/assets/` 的 `mdpp.css`、`site.css`、`mdpp.js` 是**综合文件**，
  不要拆分。改 class / `data-*` 时同步 `mdpp.js` 与 `site.html` 的无 JS 降级。
- `mdpp-cli/site/` 是 `mdpp init` 的项目骨架，不是页面资源；除非任务要求，不要动。
- `::html` 必须走白名单净化；`raw_html` 默认关闭是安全边界。
- `build_dir` 必须在项目内；`allow_outside` 只给 `serve` 的临时预览用。
- 版本保持 `0.1.0`，不要顺手发版；不要添加 CI。

## 提交前

1. 跑 `cargo fmt --all` 与 `cargo test --workspace`。
2. 跑 `cargo clippy --workspace --all-targets -- -D warnings`。
3. 确认工作区没有无关改动，提交信息用中文短句。

拆分文件时只保留一个 `#[cfg(test)] mod tests;`，并注意 `pub(super)` 不能再导出为
更宽的可见性；这些坑在历史提交里有例子。
