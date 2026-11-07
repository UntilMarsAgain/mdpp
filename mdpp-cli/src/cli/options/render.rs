use std::path::PathBuf;

use clap::Args;

use super::help::HELP_TEMPLATE;

/// `mdpp render` —— 渲染文件。
///
/// 这一组选项**原样**是从过去的顶层命令搬过来的：一个都没改语义，只是换了个位置。
#[derive(Args, Debug)]
#[command(
    next_help_heading = "选项",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = HELP_TEMPLATE,
    override_usage = "mdpp render [选项] [文件]..."
)]
pub struct Render {
    /// 要渲染的文件；不给则从标准输入读
    #[arg(value_name = "文件", help_heading = "参数")]
    pub inputs: Vec<PathBuf>,

    /// 输出完整 HTML 文档（内联默认样式与脚本），而不是片段
    #[arg(short, long)]
    pub full: bool,

    /// 完整文档的 <title>（默认取第一个文件名）
    #[arg(short, long, value_name = "标题")]
    pub title: Option<String>,

    // 取值与 `config::THEMES` 是同一套，但 clap 的 derive 宏只吃字面量，引不了那个常量 ——
    // 改一边就得改另一边（`only_system_light_and_dark_are_themes` 盯着这一处）
    /// 完整文档的主题（system / light / dark，不给就是跟随系统）；::theme 的两节由它挑
    #[arg(
        long,
        value_name = "主题",
        value_parser = ["light", "dark", "system"],
        hide_possible_values = true
    )]
    pub theme: Option<String>,

    /// 附加 CSS，可多次；按给定次序接在默认样式之后
    #[arg(long, value_name = "文件", requires = "full")]
    pub css: Vec<PathBuf>,

    /// 附加 JS，可多次；按给定次序接在默认脚本之后
    ///
    /// 默认脚本只做「点一下就切选项卡」。方向键、动画、记忆选中项属于调用方。
    #[arg(long, value_name = "文件", requires = "full")]
    pub js: Vec<PathBuf>,

    /// mermaid / math / 代码高亮 前端脚本的加载方式：cdn（默认，从 CDN 引入）、none（不加载）。
    /// 页面里若用了 ::mermaid / ::math 或写了代码块，才会把对应的资源写进 HTML。
    /// 代码高亮的 JS 连同主题（github 浅色 / 深色，随系统深浅自动挑一份）一并引入；
    /// `--css` 附加的样式排在主题之后，用自己的配色还能换掉它。
    #[arg(
        long,
        value_name = "方式",
        value_parser = ["cdn", "none"],
        default_value = "cdn",
        hide_possible_values = true
    )]
    pub external_script: String,

    /// 允许正文中的原始 HTML（默认转义）
    #[arg(short = 'u', long)]
    pub unsafe_html: bool,

    /// 启用排版替换（-- 变 –，直引号变弯引号）
    #[arg(long)]
    pub typographer: bool,

    /// 启用 Setext 标题（标题下一行写 ===）
    #[arg(long)]
    pub setext: bool,

    /// 启用缩进代码块（四空格开头）
    #[arg(long)]
    pub indented_code: bool,

    /// 关闭行内公式 $…$
    #[arg(long)]
    pub no_math: bool,

    /// 关闭内部链接 [[…]]
    #[arg(long)]
    pub no_wikilink: bool,

    /// [[目标]] 的 href 前缀
    #[arg(short = 'p', long, value_name = "前缀")]
    pub link_prefix: Option<String>,

    /// 模板页目录；<目录>/<名字>.mdpp 是一页，::名字 从这里取
    #[arg(short = 'T', long, value_name = "目录")]
    pub template_dir: Option<PathBuf>,

    /// 列出内置块名后退出
    #[arg(short = 'L', long)]
    pub list_blocks: bool,

    /// 把统计写到标准错误
    #[arg(short, long)]
    pub stats: bool,
}

impl Render {
    /// `<title>`：没给就用第一个输入的文件名；再没有就叫「未命名」。
    /// 不能是空串 —— 那会渲染出一个空的 `<title>`。
    pub fn title_or(&self) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        self.inputs
            .first()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名".to_string())
    }
}
