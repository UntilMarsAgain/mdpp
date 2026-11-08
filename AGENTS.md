# AGENTS.md

## 项目

Markdown++是一个简易、高效、紧密联系的标记语言和静态站点生成器，
基于[markdown-it](https://github.com/Markdown-it/markdown-it)构造，
并融入了多种增强文档间联系的语法。

Markdown++的缩写是mdpp，这个项目由三个部分组成：

- mdpp: Markdown++的解析库，它提供了解析mdpp文档的方法，并提供接口，供外部调用者使用。
  然而，它仅仅提供跨文档链接、模板的接口，具体搜索方法需要调用者提供。
- mdpp-cli: Markdown++的官方CLI工具，有单文档编译模式和站点模式。
  单文档编译模式相当于cmark工具，站点模式相当于mkdocs工具。
  mdpp-cli是mdpp库的一个普通下游程序，没有额外的访问权限。
- docs: 用mdpp-cli工具建设的站点，用于mdpp的文档，此站点仍在建设中。

三者的关系是：

`docs`--[用户]->`mdpp-cli`--[调用]->`mdpp`。

## 编译和测试方法

对于自动测试，使用如下这些方法：
```shell
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p mdpp --no-deps
cargo package -p mdpp --allow-dirty --no-verify
```

对于用户可见的语法测试，使用这些方法：

```shell
cargo run -q -p mdpp-cli -- -C docs serve -p 3080
cargo run -p mdpp-cli -- render --link-prefix /notes/ -T examples/templates examples/tour.mdpp -o test/tour.html --full --theme system
```

## 先看哪里

| 要改什么 | 入口 |
|---|---|
| 库的公开 API | `mdpp/src/lib.rs` |
| 解析与渲染 | `mdpp/src/runtime/engine/` |
| 渲染环境与跨切面状态 | `mdpp/src/runtime/env.rs` |
| 变体组与延迟资源 | `mdpp/src/runtime/variants.rs`、`runtime/asset.rs`、`runtime/assets.rs` |
| 块语法、分发、内置块 | `mdpp/src/block/` |
| 命令行子命令 | `mdpp-cli/src/cli/options/` |
| 整站构建 | `mdpp-cli/src/project/build/` |
| 本地预览 | `mdpp-cli/src/project/serve/` 与 `server/http.rs` |
| 页面外壳 | `mdpp-cli/src/render/page/` 与 `assets/site.html` |
| 前端样式与脚本 | `mdpp-cli/assets/` |

各目录的 `//!` 文档已经写了职责与设计理由，先读最近的模块文档，再全库搜索。

## 要求

- 注释、提示信息、文档性质的文本使用中文；标识符、文件名性质的文本用英文。
- 版本保持 `0.1.0`；不要添加 CI。
- 保持代码结构良好，拆分较大模块，但`mdpp.js`和`mdpp.css`保持大文件。


## 提交信息格式

首行必须为“类别（方面，如必要）+一句中文简述”。如：

```
feat: 裸 flag 改为 on 并迁移 banner
refactor: 迁移 aside 与提示框到标准库模板
feat(cli): 嵌入标准库模板并迁移外围模板
docs: 新增 AGENT.md 与模块拆分记录
chore: 文件系统默认名与模板参数改用 ASCII
```

剩下可按需添加信息