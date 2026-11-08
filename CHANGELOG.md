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

### 用户模板与条件

- 用户模板调用头改为严格语法检查：等号两侧的空格一律忽略，重复参数、空参数名、
  `@` 开头的参数名都会报 `template--problem`，不再退回普通段落或静默切错。
- 模板占位符改为严格填空：`{{名字}}` 缺失时报 `template--problem`，`{{?名字}}`
  缺失时填空串；不再像 MediaWiki 那样原样保留。
- 裸 flag 改为 `key=on` 的语法糖；显式写成 `key=` 才是空值，模板的可选参数
  缺失也因此能与裸 flag 分开。
- 新增 `{{@名字}}` 晚绑定引用：在块头分词之后从当前模板作用域取值，
  值里有空格、引号、换行也不会破坏参数；`{{?@名字}}` 缺失时填空串，
  `body` / `template` 是保留名。
- 新增 `::if`：按 `condition-exist` / `condition-defined` / `condition-empty` /
  `condition-equals` / `condition-truthy` / `condition-yes` / `condition-no` 与
  `condition-not` 选择 `[then]` / `[else]` 分支；`condition-yes` 认
  `yes` / `on` / `true` / `1`，`condition-no` 认 `no` / `off` / `false` / `0`。
- 模板嵌套上限提高到 64 层，并可由调用方改：库用
  `RenderContext::with_max_template_depth`，单文件模式用
  `render --max-template-depth`，站点模式用 `mdpp.toml` 的
  `max_template_depth`。`::demo` 等渲染期嵌套另有独立预算，不再能把进程栈打穿。
- `::template:名字` 不进入库语法：它仍是普通段落，模板命名空间由调用方识别。
- 模板目录不跟符号链接，模板文件沿用单份输入 32 MiB 上限；大小写只差一点的
  同名模板直接报错，不再静悄悄互相覆盖。
- 新增基础包装块 `::element`（替代 `::class`）与 `::surface`；前者支持受控的
  `tag` / `class` / `role` / `aria-*` / `data-mdpp-*`，后者提供经校验的底色、字色、
  尺寸、圆角与紧凑模式。
- `::surface` 的 class 改为调用者拥有：写了 `class=` 就按原样使用，不再自动追加
  `surface` 或 `surface--*`；高度、圆角、`mini` 改为行内样式。需要按参数有无切换
  类名时由模板写 `::if`。
- 新增原样文本原语 `::text`：正文不解析 markdown，只做 HTML 转义。
- `::element` / `::surface` 支持 `tag=auto`：单段落用 `<p>`，多段或含块用 `<div>`。
- `::element` 白名单补充 `details` / `summary` / `dl` / `dt` / `dd` / `time` / `mark`
  等安全结构标签；`a` / `img` / `video` 这类需要 URL 校验的标签仍不开放。
- 新增原生折叠原语 `::disclose`：一份正文 + 布尔状态，输出
  `<details>` / `<summary>`；`fold` 现在是它上面的一层标准库模板。非法 `color=`
  与空正文都在写出开标签之前报出来，不会留下半个框。
- 新增变体组原语 `::variants`：`control=theme` 由页面主题挑面板，`control=choice`
  生成按钮栏与 `hidden` 面板，每节的 `::css` / `::js` 按选项延迟挂载；`theme` /
  `tabs` 现在是它上面的两层标准库模板。类名、按钮文字与默认标题全部由模板给。
- `columns` / `fields` / `fold` / `theme` / `tabs` / `indent` 从 Rust 内置块迁成标准库
  模板；新增 `disclose` / `variants` / `each` / `rows` / `inline` / `style-vars` 六个
  原语，内置块最终是 23 个。
- 新增行内渲染原语 `::inline`：只取单段行内 HTML，不产生 `<p>`，可用于标题、
  标签和组名。
- 新增 `::style-vars`：只接受 `--名字` 形式的 CSS 自定义属性，值限定为颜色与尺寸；
  `--名字-bg` 自动按亮度补 `--名字-fg`，并与 `::surface` 的单盒子外观分工。
- `::element` 增加受控的 `scope` / `colspan` / `rowspan` / `hidden` / `type`，并保留
  `aria-*` / `data-mdpp-*` 白名单。
- `::each` 增加 `min=` / `max=` / `nonempty` / `check`，可以在模板顶部做预检，
  不输出内容。
- 新增 `::noinclude` / `::includeonly`：按当前有没有模板作用域判断，页面直接渲染时
  只输出 `noinclude`，作为模板引入时只输出 `includeonly`；两者都是透明的 Markdown
  容器，参数会报用法问题。
- 新增行表格原语 `::rows` 与 `Block::rows()`：按 `|` 把 `[__data__]` 里的正文拆成
  单元格行，模板用 `{{@row.0}}` / `{{@row.1}}` 取格；`max=` 可限制拆几刀。
  `fields` 现在是 `rows max=2` 上面的一层两列表格模板。
- `Section` 节点新增 `body` 原文，`ContentMode::Sections` 在解析 children 的同时
  保留每一节的原文；`each` / `rows` 也因此能直接用 `Sections` 模式，不再另走
  无缩进的裸行数据。
- `::indent` 改为 `::surface indent=` 上面的一层模板：Rust 只校验缩进等级，1.6em 的比例
  回到 `mdpp.css` 的 `.indent`。
- 分节语法收紧：`[标签]` 在基础缩进上，正文必须再缩进一层，标签层上出现没有标签头
  的正文直接报错；不再有匿名空标签节，分节也不再需要 `raw_zones` 特判。`theme` 的
  “两边都显示”改为显式 `[both]` / `[all]`，原先写在第一个标签之前的内容不再生效。
- 新增模板遍历原语 `::each`：`[__template__]` 下写模板，`[__data__]` 下放数据分节；
  每节用 `{{@item.label}}` / `{{@item.body}}` / `{{@item.index}}` 等变量，支持
  `as=` / `only=` / `skip=` / `required`。
- 标准库模板、示例、文档站示例与测试全部迁移到「标签正文缩进」的新分节写法。
- 提示框 `note` / `tip` / `warning` / `danger` / `error` 共用 `callout` 底模板，并新增
  `collapsible` / `collapsible=closed`：前者展开，后者默认折着；不写或
  `collapsible=off` 仍是原来的静态形状。折叠部分是 `::disclose` 的 `plain` /
  `details` 两种形状。
- `mdpp/stdlib/` 通过 `include_dir!` 嵌入库自带的标准库模板；目前把原 Rust 实用块
  `small` / `title` / `quote` / `signature` / `flatlist` / `banner` / `aside` / `note` /
  `tip` / `warning` / `danger` / `error` / `columns` / `fields` / `fold` / `theme` / `tabs` /
  `indent` 迁成模板，默认
  `RenderContext` 已带上，项目 `templates/` 同名页优先覆盖。
- 示例、文档站和 `mdpp init` 骨架里的模板仍属于各自项目，不进入库标准库。

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
- 渲染状态收拢成一份 `RenderEnv`，嵌套渲染的层数与变体作用域会正确继承。
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
- `mdpp.js` 按职责整理为环境、组状态、延迟资源、主题、navbox、图表、公共 API、
  启动八个段落；仍然是单个分发文件。
- 引入 KaTeX 的 CSS；此前只有 JS，公式的排版本该依赖那份样式表。

### 工程

- `mdpp render` 新增 `-o, --output <文件>`：产物直接写文件并覆盖旧内容，不写时
  仍走标准输出；输入在打开输出之前读取，输入输出同名也不会截断。
- 渲染状态收拢成 `runtime/env.rs` 的 `RenderEnv`：上下文、注册表、资源收集器、
  变体作用域与层数从散落的线程局部读写变成一份明确的环境；其它模块只读这一份。
- 第二个依赖环已断开：`RenderContext` 移到纯类型模块 `runtime/render_context.rs`，
  注册表作用域在 `block/registry/scope.rs`，资源类型在 `runtime/asset.rs`；
  这些纯类型不再回头依赖 `RenderEnv`。只剩有意的 `engine ↔ block` 行为环。
- `wikilink` 使用自己的 `WikiLink` 节点，解析阶段只记目标，渲染时再问
  `LinkResolver`；`parse` 不再在解析时查询链接上下文。
- HTML 文本/属性转义改用 `html-escape`，删掉库与命令行里各自手写的实体表。
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
