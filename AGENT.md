# AGENT.md

给后续智能体与贡献者的入口。**先读这一份，再按路径跳转**，不要一上来全库搜索。

## 0. 项目是什么

mdpp 是 Markdown++ 渲染器：在 markdown-it 之上加了一层「`::名字 参数=值` 开块、
缩进划边界」的块级方言，以及 `[[站内链接]]`、模板页、资源收集等能力。

```text
mdpp/        库（可发布）       mdpp → HTML 片段，不知道文件系统/终端/项目
mdpp-cli/    命令行（不发布）   文件渲染、整站构建、本地预览、HTTP/SSE
```

- 版本：`0.1.0`（库与命令行一起走）。
- Rust edition 2024，MSRV 1.85。
- **目前没有 CI**，发布前按第 1 节的命令手工验证。
- 注释、提交信息、文档一律用中文；代码标识符用英文。

## 1. 最短路径：常用命令

在仓库根执行：

```sh
cargo check --workspace
cargo test --workspace

# 提交前
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc -p mdpp --no-deps
cargo package -p mdpp --allow-dirty --no-verify
```

当前测试规模约 **537** 个：库单测 312、CLI 单测 185、CLI 集成测试 29、示例 3、doctest 8。
数字会变，以 `cargo test --workspace` 的输出为准。

跑命令行：

```sh
cargo run -p mdpp-cli -- render examples/tour.mdpp
cargo run -p mdpp-cli -- render --full --theme light examples/tour.mdpp > /tmp/tour.html
cargo run -p mdpp-cli -- render -T examples/templates examples/tour.mdpp

# 建一个临时项目并构建
cargo run -p mdpp-cli -- init /tmp/mdpp-demo
cargo run -p mdpp-cli -- -C /tmp/mdpp-demo build
cargo run -p mdpp-cli -- -C /tmp/mdpp-demo serve
```

**stdout 是产物通道，日志/统计/错误走 stderr**。改命令时不要往 stdout 打提示。

## 2. 仓库布局

```text
.
├── AGENT.md                 本文件
├── README.md                用户文档（它的 Rust 代码块也是 doctest）
├── CHANGELOG.md             版本变更
├── Cargo.toml               workspace 根，只有 [workspace] 与 [profile.release]
├── mdpp/                    库
│   ├── src/lib.rs           公开门面与根重导出
│   └── src/
│       ├── api/             author：给块/模板作者的公开工具
│       ├── runtime/         一次渲染的运行时
│       │   ├── engine/      Mdpp / 构建器 / 渲染 / 核心规则 / 测试
│       │   ├── context.rs   渲染上下文（链接解析器、模板页、深度）
│       │   ├── assets.rs    资源收集（CSS/JS）
│       │   └── state.rs     线程局部渲染状态（crate-private）
│       ├── parse/           只面对文本/语法树
│       │   ├── markdown.rs  parse_markdown
│       │   ├── raw.rs       原始区
│       │   ├── slug.rs      标题 id
│       │   ├── targets.rs   链接目标
│       │   └── syntax/      行内语法：baseline / mark / math / wikilink
│       ├── block/           `::` 方言与注册表
│       │   ├── scanner.rs   认块、量缩进、定内容模式
│       │   ├── dispatch.rs  按名字分发、诊断框
│       │   ├── expand.rs    用户自定义块展开
│       │   ├── fill/        {{}} 填空 + CSS 收窄
│       │   ├── html/        ::html 白名单净化 + <markdown> 槽位
│       │   ├── template.rs  模板页
│       │   ├── registry/    Block / BlockSpec / 注册表 / 分词
│       │   └── stdlib/      内置块（callouts、panels、navbox、text……）
│       ├── link/            LinkResolver / LinkTarget / ResolvedLink
│       └── support/         telemetry
├── mdpp-cli/                命令行
│   ├── src/main.rs          解析参数、-C 切目录、分发五个子命令
│   ├── src/
│   │   ├── cli/options/     Cli / Command / render / init+new / build / serve / help
│   │   ├── render/          render 命令、页面外壳、链接与模板实现
│   │   │   └── page/        Page::document、资源、顶栏/底栏、转义
│   │   ├── project/         项目、配置、整站、预览
│   │   │   ├── build/       构建编排 / 原子替换 / 文件拷贝 / 导航 / 单页 / 报告
│   │   │   ├── serve/       预览循环 / 重建 / 监视 / 临时目录
│   │   │   └── site/        页面键、路径、相对链接、红链解析
│   │   ├── server/http.rs   预览用 HTTP + SSE
│   │   └── support/         color / limits / telemetry / url
│   ├── assets/              **前端综合文件**（见第 5 节）
│   └── site/                `mdpp init` 拷走的新项目骨架
├── docs/                    示例站点（docs/build/ 是生成物，已忽略）
├── examples/                命令行示例（tour / links / templates）
└── test/                    示例产物快照（tour.html 等）
```

### 依赖方向（改模块前先看一眼）

```text
link/           最底层，不依赖 block/runtime
parse/          大体只面对文本与 markdown-it 树
runtime/        一次渲染：engine、context、assets、state
block/          scanner → registry/parse；dispatch → 内置块/模板
api/            公开作者门面，落在 block/runtime 之上
mdpp-cli/       只通过 `mdpp::` 公开 API 使用库
```

还存在两处**设计上允许**的环，不要为了「目录好看」硬拆：

1. `runtime::engine` 要装 `::` 块规则，而 `block::scanner` 又要回调解解析；
2. `block::registry::lookup` 通过线程局部拿当前注册表，而 `runtime::state`
   持有 `BlockRegistry`。

彻底消掉它们需要把渲染上下文从线程局部改成显式参数，那是独立的设计工作。

## 3. 关键入口速查

| 想改什么 | 先看这里 |
|---|---|
| 库的公开 API / 根路径 | `mdpp/src/lib.rs` |
| `Mdpp` 构建与渲染 | `mdpp/src/runtime/engine/{mod,builder,render}.rs` |
| 标题 id、外链标记 | `mdpp/src/runtime/engine/rules.rs` |
| 渲染状态 / 嵌套渲染 | `mdpp/src/runtime/state.rs`、`runtime/context.rs` |
| 块扫描、缩进边界 | `mdpp/src/block/scanner.rs` |
| 块分发与诊断框 | `mdpp/src/block/dispatch.rs` |
| 自定义块展开 | `mdpp/src/block/expand.rs` |
| 模板页加载 | `mdpp/src/block/template.rs` |
| 内置块 | `mdpp/src/block/stdlib/`，注册表在 `stdlib/mod.rs` |
| 块类型/内容模式/注册表 | `mdpp/src/block/registry/` |
| `{{}}` 填空与 CSS 收窄 | `mdpp/src/block/fill/` |
| `::html` 白名单净化 | `mdpp/src/block/html/`（规则在 `rules.rs`） |
| 行内语法 | `mdpp/src/parse/syntax/` |
| 命令行参数 | `mdpp-cli/src/cli/options/` |
| `render` 单文件命令 | `mdpp-cli/src/render/mod.rs` |
| 整页外壳 / 占位符 | `mdpp-cli/src/render/page/` + `assets/site.html` |
| 项目配置 | `mdpp-cli/src/project/config.rs` |
| 页面键、相对链接、红链 | `mdpp-cli/src/project/site/{paths,links}.rs` |
| 整站构建 | `mdpp-cli/src/project/build/` |
| 本地预览 | `mdpp-cli/src/project/serve/` + `server/http.rs` |
| 正文引用资源拷贝 | `mdpp-cli/src/project/assets.rs` |
| 前端样式/交互 | `mdpp-cli/assets/{mdpp.css,site.css,mdpp.js}` |

## 4. 公开 API 规矩

库的物理目录可以继续调整，但**根上的公开路径必须保持稳定**：

```rust
pub use api::author;
pub use block::registry;
pub use parse::{raw, slug, syntax, targets};
pub use runtime::{assets, context, engine};
pub use support::telemetry;
```

因此这些旧路径都仍然可用，不要改成物理路径：

```text
mdpp::engine::{Mdpp, MdppBuilder, Rendered}
mdpp::context::RenderContext
mdpp::assets::{Asset, AssetKind, AssetMode, AssetUse, use_asset}
mdpp::registry::{Block, BlockRegistry, BlockSpec, ContentMode}
mdpp::raw / mdpp::slug / mdpp::syntax / mdpp::targets
mdpp::author / mdpp::telemetry
```

几条硬约束：

- `mdpp::state` 是 `pub(crate)`，**不对外**。
- `#![deny(missing_docs)]`：公开项必须有文档；新增公开类型/函数先想好它该不该公开。
- `#![forbid(unsafe_code)]`：两个 crate 都是。
- `Mdpp` 必须保持 `Send + Sync`。不要往解析器/注册表里塞 `Rc`、`RefCell`、
  非 `Send` 的闭包；每次渲染的可变状态放 `state` 的线程局部里。
- `Mdpp::parse` 与 `Mdpp::render_node` 要用**同一个** `Mdpp`，AST 才认得同一套规则。
- `markdown_it` 从库根重导出，使用方写块时不必自己加依赖。
- `mdpp-cli` 是 binary-only（`doc = false`），没有对外 Rust API，可以自由改内部结构。

## 5. 前端资源约束（重要）

`mdpp-cli/assets/` 只有四个文件，其中 **CSS/JS 是刻意的综合文件**：

```text
mdpp.css    正文唯一综合样式：变量、排版、所有块、打印与无 JS 降级
site.css    站点外壳：顶栏、导航、正文卡片、页首页尾
mdpp.js     本地交互：选项卡、导航框折叠/展开、主题、图表初始化
site.html   整页模板，带 10 个 {{placeholder}}
```

规则：

- **不要拆分 `mdpp.css` / `site.css` / `mdpp.js`**。它们要能单独拷走、单独分发。
  Rust 侧可以拆模块，前端这三份保持综合。
- `site.html` 的占位符是：
  `{{lang_attr}}` `{{theme_attr}}` `{{title}}` `{{icons}}` `{{meta}}`
  `{{styles}}` `{{bar}}` `{{body}}` `{{foot}}` `{{scripts}}`。
  改模板时同步 `mdpp-cli/src/render/page/mod.rs` 的填充表。
- 资源由 `include_dir!` 在**编译期**嵌入。改 `assets/` 后必须重新编译，运行中的
  旧二进制不会自己更新。
- `render --full` **内联** CSS/JS；`build`/`serve` 把资源拷到产物 `assets/`，
  页面用相对 `<link>` / `<script src>`。
- `mdpp.css` 的 class / `data-*` 与 `mdpp.js`、`site.html` 的 `<noscript>` 降级
  是同一套契约。改类名/属性名时三处一起改：样式、脚本、无 JS 回退。
- 无 JS 时：选项卡全部展开并由 `data-label` 显示节标题，折叠的导航框展开，
  `.navbox__toggle` 隐藏。
- 外部脚本（mermaid / KaTeX / highlight.js）只在 `external-script=cdn` 且页面里
  真的用到对应块时加载；`none` 时一个都不加。KaTeX 的 CSS 也要跟着加。
- `mdpp-cli/site/` 是 `mdpp init` 的项目骨架，与 `assets/` 是两回事。**除非任务
  明确要求，不要改这个骨架**。

## 6. 项目与构建的不变量

这些是踩过坑之后钉下来的，改 `project/` 时逐条对照：

- **页面键** = `pages/` 下的相对路径，去掉 `.md` / `.mdpp`，小写归一；
  产物是 `build/<键>/index.html`，首页键是 `index`。
- 页面键会变成路径：拒绝空段、`.`、`..`、反斜杠、控制字符、Windows 保留名。
  **先校验键，再拼 `PathBuf`**。
- 站内链接一律用**相对地址**（`../` × 当前页深度），整个 `build/` 可以拷到任意
  子路径。`base_url` 只影响 `<link rel="canonical">`。
- `[[目标]]` 指不到页面时渲染**红链**（没有 `href`），并记进报告；
  `--strict` 下算失败。指向模板页的链接不算「缺页」。
- `build_dir` 必须在项目根之内，且不能是正文/模板目录或其子目录、也不能是它们
  的祖先。比较前做词法归一 + 已存在部分的真实路径解析。
- 构建是**整目录替换**：先出到同级 `.mdpp-tmp-<pid>`，全部成功才 `rename`；
  失败不动旧站点。`--clean` 只是兼容开关，不再改变行为。
- `serve` 出到系统临时目录的 `mdpp-preview-<pid>/cache/turn-N`，每遍一个子目录，
  全部写完后才把服务的根指针挪过去；旧的一遍留给正在读的连接。
- 临时目录名**只用 ASCII**：`std::env::temp_dir()` 在某些 locale 下会按自己的
  编码重新解释非 ASCII 名字。
- 正文引用的本地资源只拷**被引用**的，且不许跑出 `pages/`；缺失的逐条报告。
- 覆盖层（`override_dir`）不跟随符号链接；产物根不挂外部文件。
- 产物根总有空的 `.nojekyll`，否则 GitHub Pages 会静默丢掉 `_` 开头的目录。
- `mdpp.toml` 用 `deny_unknown_fields`：拼错键要在解析时就报，不能默默用默认值。
- 默认名保持 ASCII：`cache/turn-N`、`.mdpp-tmp-*`、`toc = "contents"`、
  `{{title}}` / `{{home}}`。

## 7. 测试与代码风格

- 单测跟着模块走：优先 `模块/tests.rs`，在 `mod.rs` 里写
  `#[cfg(test)] mod tests;`。**不要重复声明同一个 `tests` 模块**。
- 测试名是英文小写短句；测试内的说明与断言语义用中文。
- 库测试不要依赖真实文件系统；CLI 测试可以用 `tempfile`。
- 集成测试：
  - `mdpp-cli/tests/project.rs`：命令行端到端（含路径/清理安全）。
  - `mdpp-cli/tests/samples.rs`：`examples/` 每份都必须渲染出零问题框。
- 改块或模板后跑 `samples`；改命令行后跑 `project`。
- 解析/渲染的边界情况优先写成库单测，不要只为 CLI 写。
- 日志用 `tracing`：库侧 `debug!` / `trace!`，订阅器只在 CLI 装。
- CSS/JS 改动至少验证：`render --full` 能打开、`build` 资源链接正确、
  无 JS 时选项卡与导航框仍可读。

## 8. 拆分/移动文件时的检查单

这个仓库做过多轮模块重组，以下错误反复出现：

1. **可见性**：子模块里 `pub(super) fn` 不能在父模块用更宽的
   `pub(super) use` / `pub use` 再导出。要么在定义处放宽到 `pub(crate)`，
   要么在父模块里改成私有 `use`。
2. **父模块的文档与重导出**：拆 `foo.rs` 为 `foo/mod.rs` 时，保留模块头文档与
   原来的公开路径；把子模块的项在 `mod.rs` 里重新导出。
3. **测试模块**：从原文件搬走 `#[cfg(test)] mod tests { ... }` 时，注意别在
   新 `mod.rs` 里再留一个同名声明；测试需要的 `use` 也要一起搬。
4. **测试可见性**：测试常通过 `use super::*` 拿到父模块的导入；父模块删了某个
   `use`，测试会突然报「找不到类型」。显式补测试自己的导入更稳。
5. **文档注释不要悬空**：拆分时如果一段 `///` 后面没有 item，会触发
   `expected item after doc comment`；把注释跟着它描述的 item 走。
6. **clippy 的 `empty_line_after_doc_comments`**：`///` 与 item 之间不要留空行。
7. **公开项缺文档**：`#![deny(missing_docs)]` 会直接挡编译。
8. 改完至少跑：`cargo fmt --all`、`cargo check --workspace`、
   `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`。

## 9. 文档与产物

- 用户文档：`README.md`（命令、配置、库用法）。
- 变更记录：`CHANGELOG.md`，按「新增 / 修复 / 重构」写中文条目。
- 示例站点：`docs/`；构建产物在 `docs/build/`，已忽略，**不要手改**。
- 命令行示例：`examples/*.mdpp` 与 `examples/templates/`，被 `samples.rs` 测。
- 版本保持 `0.1.0`，除非任务明确要求发版；不要顺手改 `Cargo.toml` 的版本。

## 10. 不要做的事

- 不要加 CI（仓库托管与权限尚未就绪）。
- 不要拆前端三份综合 CSS/JS。
- 不要把库变成「知道文件系统/项目/终端」的东西；那些属于 `mdpp-cli`。
- 不要在库内部引入全局可变状态；渲染状态是线程局部的。
- 不要绕过 `::html` 的白名单净化；`raw_html` 默认关闭是安全边界。
- 不要在 `serve` 之外打开 `allow_outside`；它只服务于临时预览目录。
- 不要为了消掉依赖环而大改公开 API，除非任务明确要求。
