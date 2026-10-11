//! `mdpp init` —— 建一个项目骨架。
//!
//! 产出的是一个**能立刻 build 的目录**，不是一个空壳：首页、示例页与一个模板页都已经
//! 在那儿，第一次 `mdpp build` 就出得来一个能看的站点。
//!
//! **已有的文件一律不覆盖**（`write_new_file`）—— 在一个已经写了点的目录里重跑 `init`
//! 是常事，默默盖掉首页是最难查的一种丢数据。要换模板页就自己删了重建。

use std::path::{Path, PathBuf};

use crate::options::Init;
use crate::project::{config::SKELETON, ensure_dir, shown, write_new_file};

use tracing::info;

/// 首页
const INDEX_PAGE: &str = "\
# {{站点名}}

这个站点由 `mdpp` 搭起来，正文都在 [`pages/`](pages/) 里，一个文件一页。

- [[示例]] —— 一页样例，几个块都在那儿
- 全部块名与示例见仓库的 `examples/tour.mdpp`

::卡片 标题=首页

改完存盘，`mdpp build` 出站点到 `build/`。
";

/// 随骨架建出来的那一页样例
const SAMPLE_PAGE: &str = "\
# 示例

正文照常写 markdown：**强调**、`行内代码`、[[index|回首页]]。

::note title=\"提示框\"
  提示框里也能写 **markdown** 与 [[index|链接]]。

::tabs
  [读法一]
  照着念。

  [读法二]
  自己试。

::code lang=rust
  fn main() {
      println!(\"代码块原样输出\");
  }
";

/// 随骨架建出来的那一页模板页。`::卡片 标题=…` 从 `templates/卡片.mdpp` 取
const SAMPLE_TEMPLATE: &str = "\
::banner color=#0055a4
  **{{标题}}**

  {{body}}
";

/// 建了 `.gitignore` 的内容：`build/` 是产物，不进版本库
const GITIGNORE: &str = "# mdpp build 的产物，不进版本库\n/build/\n";

pub fn run(options: &Init) -> Result<(), String> {
    let root = match &options.dir {
        Some(dir) => dir.clone(),
        None => PathBuf::from("."),
    };
    ensure_dir(&root)?;

    let name = options
        .name
        .clone()
        .or_else(|| root.file_name().map(|name| name.to_string_lossy().into_owned()))
        .filter(|name| !name.is_empty() && name != ".")
        .unwrap_or_else(|| "我的站点".to_string());

    let pages = config_path(&root, "pages");
    let templates = config_path(&root, "templates");
    let build = config_path(&root, "build");

    let mut created: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();

    // 已有配置就在那儿 —— 这是「补齐缺的那些」，不是「重新初始化」
    if write_new_file(&root.join("mdpp.toml"), &skeleton(&name))? {
        created.push("mdpp.toml".to_string());
    } else {
        kept.push("mdpp.toml".to_string());
    }
    if write_new_file(&pages.join("index.mdpp"), &filled(INDEX_PAGE, &name))? {
        created.push(shown(&pages.join("index.mdpp")));
    } else {
        kept.push(shown(&pages.join("index.mdpp")));
    }
    if write_new_file(&pages.join("示例.mdpp"), SAMPLE_PAGE)? {
        created.push(shown(&pages.join("示例.mdpp")));
    } else {
        kept.push(shown(&pages.join("示例.mdpp")));
    }
    if write_new_file(&templates.join("卡片.mdpp"), SAMPLE_TEMPLATE)? {
        created.push(shown(&templates.join("卡片.mdpp")));
    } else {
        kept.push(shown(&templates.join("卡片.mdpp")));
    }
    if write_new_file(&root.join(".gitignore"), GITIGNORE)? {
        created.push(".gitignore".to_string());
    } else {
        kept.push(".gitignore".to_string());
    }
    // 空的产物目录也要有：让目录结构一眼看得见。`.gitignore` 忽略它，所以里面不放标记文件
    ensure_dir(&build)?;

    info!(dir = shown(&root), pages = created.len(), "项目骨架");
    report(&root, &created, &kept);
    Ok(())
}

/// 目录在配置里叫什么，就按那个名字建 —— `pages_dir` 改了之后骨架要跟着走
fn config_path(root: &Path, dir: &str) -> PathBuf {
    root.join(dir)
}

/// 骨架里那行 `site_name` 换成给定的名字
fn skeleton(name: &str) -> String {
    let placeholder = r#"site_name = "我的站点""#;
    let replacement = format!("site_name = {}", toml_string(name));
    if let Some(at) = SKELETON.find(placeholder) {
        let mut out = String::with_capacity(SKELETON.len());
        out.push_str(&SKELETON[..at]);
        out.push_str(&replacement);
        out.push_str(&SKELETON[at + placeholder.len()..]);
        return out;
    }
    // 模板里那行不见了：这份骨架还是得能用，于是退回原样（init 的测试会红）
    SKELETON.to_string()
}

/// 首页模板里那一个占位
fn filled(template: &str, name: &str) -> String {
    template.replace("{{站点名}}", name)
}

/// TOML 基本字符串里的转义。站点名是人写的，里面带引号不该让配置文件解析不了
fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// 报告一律走**标准错误**：stdout 是产物（HTML）要去的地方，这条规矩对所有子命令都一样
fn report(root: &Path, created: &[String], kept: &[String]) {
    for path in created {
        eprintln!("建了  {path}");
    }
    for path in kept {
        eprintln!("已有  {path}（没动）");
    }
    eprintln!("项目在 {}", shown(root));
    eprintln!("下一步：cd 到那里，`mdpp build`");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Init;
    use crate::project::Config;

    fn init_at(dir: &Path) -> Result<(), String> {
        run(&Init {
            dir: Some(dir.to_path_buf()),
            name: None,
        })
    }

    #[test]
    fn init_makes_a_project_that_builds_later() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path()).expect("建得出");

        for path in [
            "mdpp.toml",
            "pages/index.mdpp",
            "pages/示例.mdpp",
            "templates/卡片.mdpp",
            "build",
            ".gitignore",
        ] {
            assert!(dir.path().join(path).exists(), "少了 {path}");
        }
        // 骨架真的能被读回来
        assert!(Config::load(dir.path()).is_ok(), "刚建的项目读不出来配置");
    }

    /// 建出来的样例页不该有「未知块」—— 那会让新用户的第一次 build 一上来就报错
    #[test]
    fn the_skeleton_pages_have_no_unknown_blocks() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path()).expect("建得出");
        let pages = crate::project::site::Site::collect(&dir.path().join("pages")).expect("扫得动");
        assert!(pages.pages().len() >= 2);

        let templates =
            crate::context::template_pages(&dir.path().join("templates")).expect("读得动");
        let context = mdpp::RenderContext::new().with_templates(templates);
        for page in pages.pages() {
            let source = std::fs::read_to_string(&page.source).expect("读得动");
            let html = mdpp::Mdpp::new().render_with(&source, &context);
            assert!(
                !html.contains("template--unknown"),
                "{} 里有未知块：\n{html}",
                page.key
            );
        }
    }

    /// 站点名写进两处：mdpp.toml 与首页
    #[test]
    fn the_site_name_lands_in_the_config_and_the_home_page() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        run(&Init {
            dir: Some(dir.path().to_path_buf()),
            name: Some("桥梁笔记".to_string()),
        })
        .expect("建得出");

        let config = Config::load(dir.path()).expect("读得动");
        assert_eq!(config.site_name, "桥梁笔记");
        let home = std::fs::read_to_string(dir.path().join("pages/index.mdpp")).expect("读得动");
        assert!(home.contains("# 桥梁笔记"), "{home}");
    }

    /// 名字里带引号不能让配置文件解析不了 —— 站点名是人写的
    #[test]
    fn a_quoted_site_name_still_yields_a_parsable_config() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        run(&Init {
            dir: Some(dir.path().to_path_buf()),
            name: Some(r#"甲"乙"#.to_string()),
        })
        .expect("建得出");
        assert_eq!(
            Config::load(dir.path()).expect("读得动").site_name,
            r#"甲"乙"#
        );
    }

    /// 重跑一次不动作者写过的文件
    #[test]
    fn running_init_again_keeps_what_the_author_wrote() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        init_at(dir.path()).expect("第一次建得出");
        std::fs::write(dir.path().join("pages/index.mdpp"), "# 我改过了").expect("改得下");

        init_at(dir.path()).expect("第二次也建得出");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("pages/index.mdpp")).expect("读得动"),
            "# 我改过了"
        );
    }

    /// 骨架模板与配置结构体脱节时，`init` 出来的项目当场就 build 不了 ——
    /// 那条 `the_skeleton_parses` 是在库里盯着，这里盯的是「换成真名字也没坏」
    #[test]
    fn the_skeleton_stays_parsable_after_the_name_is_substituted() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        run(&Init {
            dir: Some(dir.path().to_path_buf()),
            name: Some("甲".to_string()),
        })
        .expect("建得出");
        Config::load(dir.path()).expect("换过名字的配置该读得动");
    }

    /// 目录不存在时建出来 —— `mdpp init 新站点` 是最常见的一条
    #[test]
    fn a_missing_directory_is_created() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let fresh = dir.path().join("新站点");
        init_at(&fresh).expect("建得出");
        assert!(fresh.join("mdpp.toml").is_file());
    }
}