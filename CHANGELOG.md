# 变更记录

## 0.1.0（未发布）

这一版把 demo 形状收成了可部署的形状，公开 API 仍按 0.1 处理，版本号不变。

### 安全与数据完整性

- 页面键统一校验：拒绝 `.` / `..` / 反斜杠 / 控制字符 / Windows 保留名；页面扫描
  不再跟随符号链接。`pages/a\..\..\x.mdpp` 这类文件名不能再把产物写到站点目录外。
- `build_dir` 必须在项目目录之内；`--clean` 在删除任何东西之前先验路径，不能清到
  项目外、项目根、正文/模板目录或它们的上一层。
- `::html` 的 `<markdown>` 槽位不再受 `raw_html` 影响，始终再净化一遍。
- `srcset` 逐个候选地址校验协议；`target="_blank"` 自动补
  `rel="noopener noreferrer"`；媒体 `src` 的协议判定会先去掉空白与控制字符。
- 单份输入上限 32 MiB；预览服务器改用 `axum` + `tower-http`：请求解析、MIME、
  range、HEAD、目录索引与 `..` 防护交给库，SSE 仍由本库发。

### 正确性

- 链接目标空格编码与 `[标签]` 分节改成**探针解析 + `srcmap`**：块引用/列表里
  的围栏不再被误改，列表项里紧跟列表记号开的围栏也不会吞掉下一个标签。
- 嵌套块（`::note` 等）里的链接目标同样会编码空格。
- 标题 `id` 按整棵树唯一化：同页同名标题得到 `安装` / `安装-1`；删除
  `DropDuplicateHeadingIds` 补丁。

### 构建与并发

- `build` 改为整目录替换：先出到临时目录，全部成功才换名；失败时旧站点不动。
- `--jobs N` 并行出页，产物与单线程逐字节一致。
- `Mdpp` 改为 `Send + Sync`，一个解析器可被多线程共享。
- 渲染状态收拢成一份 `RenderState`，嵌套渲染的层数与主题槽会正确继承。
- 资源去重从线性扫描改成哈希集合。

### 扩展 API

- `Mdpp::parse` / `parse_with` 与 `render_node` / `render_node_with_assets`：可以把
  文档解析成 markdown-it AST，改完再渲染。
- `BlockSpec::with_mode_fn`：同一个自定义块可以按实例选择
  `Markdown` / `Sections` / `Raw`。
- `mdpp::author`：写自定义块/模板时用的稳定工具面（填空、分节、净化、诊断框）。
- `use_asset` / `AssetUse`：自定义块可以登记自己的 CSS / JS。
- `mdpp::variants`：`Group` / `Variant` / `Control` / `Scope`，把「一列选项 +
  每个选项自己的延迟资源」做成公开能力；`::tabs` 与 `::theme` 都改用它。
- `AssetUse::Deferred` / `DeferredAsset` / `AssetPolicy`：变体作用域里的
  CSS / JS 变成惰性资源；`use_asset_with` 与 `use_asset_once` 可以选择
  「每次激活执行」或「只执行一次」。
- `mdpp/examples/bench.rs`：单线程与多线程渲染的轻量基准。

### 前端

- `::tabs` 的面板资源也按选项延迟挂载：未选中的面板不再在页面加载时直接
  执行其中的 CSS / JS。
- 所有由 mdpp 生成的 `<script>` 都带 `data-mdpp-version` 与 `data-mdpp-mode`；
  延迟脚本还带 `data-mdpp-group` / `value` / `key` / `run`。`mdpp.js` 提供
  `window.mdppVariants` 的 `get` / `set` / `refresh` / `rescan` / `onChange` /
  `hasRun` / `runCount` / `markRun`，并继续保留 `window.mdppThemeInject` 别名。
- 动态加载只做 DOM 挂载：`render --full` 内联在 `<template>`，`build` 写成
  `assets/` 下的静态文件；不引入 fetch、服务端接口或运行时后端。
- 站外链接标记改为「方框 + 斜向箭头」的 CSS mask 图标（取自 MediaWiki 的
  external-link 图标），颜色跟 `currentColor` 走；图标不跟链接文字下划线。
  打印时恢复成 ` (URL)` 文本。
- 页面外壳加 `<noscript>` 降级：没有 JS 时 tabs 全部展开并带节标题，折叠的
  navbox 也展开；与打印样式同一套。
- `mdpp.js` 修复：重复包含时 `data-mdpp-bound` 空值判断失效、嵌套 tabs / navbox
  误绑到外层、主题固定判断、整页级初始化重复执行。
- 引入 KaTeX 的 CSS；此前只有 JS，公式的排版本该依赖那份样式表。

### 工程

- `mdpp` 的源码按职责收进 `runtime` / `parse` / `block` / `api` / `support`，
  `mdpp-cli` 收进 `cli` / `render` / `project` / `server` / `support`；根上的
  `mdpp::engine`、`mdpp::context`、`mdpp::raw` 等公开路径通过重导出保持不变。
- 大文件按职责继续拆分：`engine` → `mod/builder/render/rules/tests`，
  `registry` → `block/spec/table/tokenize`，`html` → `mod/markdown/rules/escape`，
  `fill` → `mod/css`，`navbox` → `mod/parse/render`，
  `page` → `mod/assets/chrome/text`，`build` → `mod/clean/files/nav/page/report`，
  `site` → `mod/paths/links`，`serve` → `mod/rebuild/temp/watch`，
  `cli/options` → `mod/render/project/build/serve/help`。
- 仓库根新增简明的 `AGENTS.md`：常用命令、入口索引、硬约束与提交前检查。
- 前端 `mdpp.css` / `site.css` / `mdpp.js` 保持各自一份综合文件，便于分发。
- `cargo fmt`、`cargo clippy -D warnings` 全绿；两个 crate 都 `forbid(unsafe_code)`。
- 默认生成的文件/目录名与模板占位符只用 ASCII（`cache/turn-N`、`.mdpp-tmp-*`、
  `toc = "contents"`、`{{title}}` / `{{home}}` 等）；中文内容仍原样支持，
  但不再作为默认路径名或参数键。
- README 的配置名与块数量修正，补充 `--jobs`、原子构建、AST 与扩展 API 说明。
