//! **子命令**的端到端行为：走真的那个二进制（[`CARGO_BIN_EXE_mdpp`]）。
//!
//! 单元测试跑的是函数，这里跑的是**命令行**：退出码、消息去了哪条通道、文件落在哪。
//! 这三件事单元测试看不见，而它们恰恰是命令行最要紧的部分。
//!
//! 一个项目跑完整的一趟：`init` → `new` → `build`，不预先在测试里铺好目录 ——
//! 铺好的目录只会证明「文件已经在那儿」。

use std::path::Path;
use std::process::{Command, Output};

/// 跑一次 `mdpp`，子命令与参数都给全
fn mdpp(at: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mdpp"))
        .current_dir(at)
        .args(args)
        .output()
        .expect("跑得动自己")
}

/// 造一个项目根（临时目录）。里面**什么都没有**，由各用例自己 init
fn empty_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("造得出临时目录")
}

/// 建一个项目出来，返回那个项目根
fn init(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("站点");
    let out = mdpp(dir, &["init", "站点"]);
    assert!(out.status.success(), "init 失败：{out:?}");
    root
}

/// 子进程的 **stdout** —— 按规矩只有产物走这条通道，所以下面几条都拿它当判据
fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 子进程的 **stderr** —— 日志、报告、报错都在这儿（与「stdout 只出产物」是一条规矩）
fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// **一趟走完**：`init` → `new` → `build`，每一步的产物都该落在它该落的地方。
///
/// 三条分开写也能跑，但这条要验的恰恰是**接得起来**：某一步少交一样东西给下一步，
/// 分开的用例各自都还绿着，整条路却是断的
#[test]
fn init_new_build_is_a_whole_ride() {
    let dir = empty_dir();
    let root = init(dir.path());

    for path in [
        "mdpp.toml",
        "pages/index.mdpp",
        "templates/卡片.mdpp",
        ".gitignore",
    ] {
        assert!(root.join(path).exists(), "少了 {path}");
    }
    assert!(root.join("build").is_dir(), "少了 build/");

    // `new` 建一页
    let out = mdpp(&root, &["new", "指南/安装"]);
    assert!(out.status.success(), "new 失败：{}", stderr_of(&out));
    assert!(
        root.join("pages/指南/安装.mdpp").is_file(),
        "那一页没建出来"
    );

    // `build` 出站点
    let out = mdpp(&root, &["build"]);
    assert!(out.status.success(), "build 失败：{}", stderr_of(&out));
    assert!(root.join("build/index.html").is_file(), "首页没出");
    assert!(
        root.join("build/指南/安装/index.html").is_file(),
        "嵌套页没出"
    );
    // build 的消息一条都不该进 stdout —— 那条通道留给产物
    assert_eq!(stdout_of(&out), "", "build 不该往 stdout 写东西");
    assert!(stderr_of(&out).contains("build"), "{}", stderr_of(&out));
}

/// 站内链接**认得出**（出真 `href`），指不到的那条**保持红链**（不编一个地址出来）。
///
/// 这是库那套红链语义在**真实站点**上的落点：命令行手上有 `pages/`，「有没有这一页」
/// 在这里是能答的 —— 答得出，就该答；答不出的那条留在原地等人补
#[test]
fn a_page_links_to_another_one_and_a_missing_one_stays_red() {
    let dir = empty_dir();
    let root = init(dir.path());
    std::fs::write(
        root.join("pages/甲.mdpp"),
        "# 甲\n\n看 [[乙]]，也看 [[根本没有这一页]]。\n",
    )
    .expect("写得下");
    std::fs::write(root.join("pages/乙.mdpp"), "# 乙\n").expect("写得下");

    let out = mdpp(&root, &["build"]);
    assert!(out.status.success());
    let html = std::fs::read_to_string(root.join("build/甲/index.html")).expect("读得动");

    assert!(
        html.contains(r#"href="../乙/""#),
        "站内链接没指到那一页：{html}"
    );
    assert!(
        html.contains(r#"class="wikilink wikilink--missing""#),
        "指不到的那一页该是红链：{html}"
    );
    // 报告里逐条说清是哪一页的哪个目标
    let report = stderr_of(&out);
    assert!(report.contains("甲"), "{report}");
    assert!(report.contains("根本没有这一页"), "{report}");
}

/// 产物里那个 `.nojekyll` 不能少：GitHub Pages 少了它就跑 Jekyll，而 Jekyll 把 `_`
/// 开头的目录**静默丢掉** —— `pages/_草稿/` 出了页面，推上去却 404，**且不报错**
#[test]
fn the_build_stops_github_pages_from_running_jekyll() {
    let dir = empty_dir();
    let root = init(dir.path());
    let draft = root.join("pages/_草稿");
    std::fs::create_dir_all(&draft).expect("建得出");
    std::fs::write(draft.join("某页.mdpp"), "# 草稿\n").expect("写得下");

    let out = mdpp(&root, &["build"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        root.join("build/.nojekyll").exists(),
        "少了 .nojekyll：带 _ 的目录推上去会被丢掉"
    );
    // 而那一页确实出了（`.nojekyll` 是让服务端别丢，不是让 mdpp 别出）
    assert!(root.join("build/_草稿/某页/index.html").exists());
}

/// 配了 `site_url` 才有 canonical，而**页内链接仍然是相对的**
#[test]
fn site_url_gives_a_canonical_and_leaves_the_links_alone() {
    let dir = empty_dir();
    let root = init(dir.path());
    std::fs::write(
        root.join("pages/甲.mdpp"),
        "# 甲\n\n一句话说明\n\n看 [[乙]]。\n",
    )
    .expect("写得下");
    std::fs::write(root.join("pages/乙.mdpp"), "# 乙\n").expect("写得下");

    // 没配 `site_url`：没有 canonical
    assert!(mdpp(&root, &["build"]).status.success());
    let plain = std::fs::read_to_string(root.join("build/甲/index.html")).expect("读得动");
    assert!(!plain.contains("rel=\"canonical"), "{plain}");

    let config = root.join("mdpp.toml");
    let text = std::fs::read_to_string(&config).expect("读得动");
    std::fs::write(
        &config,
        text.replace(
            "site_description = \"\"",
            "site_description = \"一句话说明\"",
        )
        .replace(
            "site_name =",
            "site_url = \"https://x.github.io/repo\"\nsite_name =",
        ),
    )
    .expect("改得下");
    assert!(mdpp(&root, &["build"]).status.success());

    let html = std::fs::read_to_string(root.join("build/甲/index.html")).expect("读得动");
    assert!(
        html.contains(r#"<link rel="canonical" href="https://x.github.io/repo/甲/">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<meta name="description" content="一句话说明">"#),
        "{html}"
    );
    // 页内链接仍是相对的 —— 那才是整站能挂在子路径下的原因
    assert!(
        html.contains(r#"href="../乙/""#),
        "canonical 不该把相对链接也变成绝对的：{html}"
    );

    // 首页的 canonical 是站点根本身
    let home = std::fs::read_to_string(root.join("build/index.html")).expect("读得动");
    assert!(
        home.contains(r#"<link rel="canonical" href="https://x.github.io/repo/">"#),
        "{home}"
    );
}

/// 产物里的地址得真的指得到 —— 少退一层是「看着对、点不动」
#[test]
fn every_link_in_the_built_site_points_at_a_page_that_exists() {
    let dir = empty_dir();
    let root = init(dir.path());
    // 三层深浅各一页，外加根上那一页
    for (at, body) in [
        ("乙", "# 乙\n\n回 [[index]]。\n"),
        ("指南/入门", "# 入门\n\n看 [[手册]]，看 [[乙]]。\n"),
        ("指南/深入/安装", "# 安装\n\n回 [[入门]]。\n"),
        ("手册", "# 手册\n\n看 [[指南/深入/安装]]。\n"),
    ] {
        let path = root.join(format!("pages/{at}.mdpp"));
        std::fs::create_dir_all(path.parent().expect("有上一层")).expect("建得出");
        std::fs::write(&path, body).expect("写得下");
    }

    let out = mdpp(&root, &["build"]);
    assert!(out.status.success(), "{}", stderr_of(&out));

    // 把每个产物页面当成它在自己那个地址上打开，逐条解析它里面的链接
    for page in ["index", "乙", "指南/入门", "指南/深入/安装", "手册"] {
        // 首页落在站点根上（`build/index.html`），不在 `build/index/` 那个目录里
        let at = if page == "index" {
            "build/index.html".to_string()
        } else {
            format!("build/{page}/index.html")
        };
        let html = std::fs::read_to_string(root.join(&at)).expect("读得动");
        // 首页的地址就是站点根 `/`，其余每一页是 `/键/`
        let address = if page == "index" {
            String::from("/")
        } else {
            format!("/{page}/")
        };
        for target in links_in(&html) {
            let landed = normalize(&address, &target);
            // 样式与脚本落在 `assets/`，那是另一条管线验的
            if landed.starts_with("/assets/") {
                continue;
            }
            let exists = root.join(format!("build{landed}index.html")).exists();
            assert!(
                exists,
                "{address} 里的 {target} 落到了 {landed}，那儿没有页面"
            );
        }
    }
}

/// 页面里所有 `href="…"`（站内那些相对地址）
fn links_in(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("href=\"") {
        rest = &rest[at + "href=\"".len()..];
        let Some(end) = rest.find('"') else { break };
        out.push(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
    out
}

/// 一个相对地址从 `from`（一个以 `/` 开头、以 `/` 结尾的目录式地址）出发落到哪儿
///
/// 手写而不是引库：规则就「退一层吃掉一段、然后顺着拼」，十来行，而这一条
/// 恰恰是**最该有个独立实现来对着看**的
fn normalize(from: &str, target: &str) -> String {
    let mut stack: Vec<&str> = from.split('/').filter(|one| !one.is_empty()).collect();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    format!("/{}/", stack.join("/"))
}

/// `--strict` 把红链变成非零退出码 —— 这是给 CI 用的
#[test]
fn strict_fails_the_build_when_a_link_goes_nowhere() {
    let dir = empty_dir();
    let root = init(dir.path());
    std::fs::write(root.join("pages/甲.mdpp"), "# 甲\n\n[[还没写]]\n").expect("写得下");

    assert!(mdpp(&root, &["build"]).status.success(), "默认不该失败");

    let out = mdpp(&root, &["build", "--strict"]);
    assert!(!out.status.success(), "--strict 下红链该让 build 失败");
    assert!(stderr_of(&out).contains("红链"), "{}", stderr_of(&out));
}

/// 不是项目的目录里敲 build，要说清「这不是项目」并指向 init
#[test]
fn build_outside_a_project_says_so_and_points_at_init() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["build"]);

    assert!(!out.status.success());
    let message = stderr_of(&out);
    assert!(message.contains("不是一个 mdpp 项目"), "{message}");
    assert!(message.contains("mdpp init"), "{message}");
}

/// 页名不许往上跑：`../逃出` 要**当场拒绝**并说清是页名的问题 ——
/// 而不是老老实实建出一个 `pages/` 之外的文件（那已经不是「建一页」了）
#[test]
fn new_refuses_to_climb_out_of_pages() {
    let dir = empty_dir();
    let root = init(dir.path());

    let out = mdpp(&root, &["new", "../逃出"]);
    assert!(!out.status.success(), "页名不许跑出 pages/");
    assert!(stderr_of(&out).contains("页名"), "{}", stderr_of(&out));
}

/// 产物只有 HTML，`--stats` 与日志都不往 stdout 混
#[test]
fn render_writes_html_to_stdout_and_the_rest_to_stderr() {
    let dir = empty_dir();
    // 空的标准输入也算一份输入 —— 那不是「没有输入」
    let out = mdpp(dir.path(), &["render", "--stats"]);
    assert_eq!(stdout_of(&out), "", "空输入不该产出东西");
    assert!(stderr_of(&out).contains("1 份输入"), "{}", stderr_of(&out));

    let out = mdpp(dir.path(), &["render", "--full", "--title", "某篇"]);
    assert!(
        stdout_of(&out).starts_with("<!DOCTYPE html>"),
        "{}",
        stdout_of(&out)
    );
    assert_eq!(stderr_of(&out), "", "没有该报的东西就不该有输出");
}

/// 少了子命令时打印帮助并**成功**返回 —— 那是「还不知道该敲什么」，不是「用错了」。
/// 帮助走 stderr：`mdpp > 输出.html` 得到的该是一个空文件，而不是一页帮助。
#[test]
fn a_bare_invocation_prints_the_help() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &[]);

    assert!(out.status.success());
    assert_eq!(stdout_of(&out), "");
    assert!(stderr_of(&out).contains("子命令"), "{}", stderr_of(&out));
}

/// 敲错的子命令要报得像个人话，并带上「你是不是想写什么」
#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["buidl"]);

    assert_eq!(out.status.code(), Some(2), "用法错是退出码 2");
    assert!(stderr_of(&out).contains("buidl"), "{}", stderr_of(&out));
}

/// `serve` 在不是项目的目录里也要说清是哪一个 —— 它跟 build 一样先找项目
#[test]
fn serve_outside_a_project_says_so() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["serve"]);

    assert!(!out.status.success());
    assert!(
        stderr_of(&out).contains("不是一个 mdpp 项目"),
        "{}",
        stderr_of(&out)
    );
}

/// 覆盖层目录：build 后把 `override/` 里的东西原样拷到产物里，重复就覆盖
#[test]
fn the_override_directory_is_copied_after_build() {
    let dir = empty_dir();
    let root = init(dir.path());

    std::fs::create_dir_all(root.join("override")).expect("建得出");
    std::fs::write(
        root.join("override/CNAME"),
        "example.com
",
    )
    .expect("写得下");
    std::fs::write(
        root.join("override/robots.txt"),
        "User-agent: *
Disallow: /private/
",
    )
    .expect("写得下");
    std::fs::write(
        root.join("override/index.html"),
        "<h1>自定义首页</h1>
",
    )
    .expect("写得下");

    assert!(mdpp(&root, &["build"]).status.success());
    assert_eq!(
        std::fs::read_to_string(root.join("build/CNAME"))
            .expect("读得动")
            .trim(),
        "example.com"
    );
    assert!(
        std::fs::read_to_string(root.join("build/robots.txt"))
            .expect("读得动")
            .contains("Disallow: /private/")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("build/index.html"))
            .expect("读得动")
            .trim(),
        "<h1>自定义首页</h1>"
    );

    // 配个别的名字也能用
    let config = root.join("mdpp.toml");
    let text = std::fs::read_to_string(&config).expect("读得动");
    std::fs::write(
        &config,
        text.replace(
            "override_dir = \"override\"",
            "override_dir = \"我的覆盖层\"",
        ),
    )
    .expect("改得下");
    std::fs::create_dir_all(root.join("我的覆盖层")).expect("建得出");
    std::fs::write(
        root.join("我的覆盖层/test.txt"),
        "custom-override
",
    )
    .expect("写得下");

    assert!(mdpp(&root, &["build"]).status.success());
    assert_eq!(
        std::fs::read_to_string(root.join("build/test.txt"))
            .expect("读得动")
            .trim(),
        "custom-override"
    );
}

/// 标签页上那个小图标：`favicon = "…"` 配了就出 `<link rel="icon">`，并把那个文件
/// 拷进 `assets/`。**二进制格式（`.png` / `.ico` / `.gif`）是这里的要紧处** ——
/// 那正是 favicon 最常见的形态，而按文本读它会因为「不是合法 UTF-8」直接把 build
/// 弄失败，于是最常见的用法反而用不了
#[test]
fn a_binary_favicon_is_copied_and_linked() {
    let dir = empty_dir();
    let root = init(dir.path());

    // 一个 1×1 的真 PNG：内容是**二进制**，按 UTF-8 读必然失败
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    std::fs::create_dir_all(root.join("assets")).expect("建得出");
    std::fs::write(root.join("assets/icon.png"), PNG).expect("写得下");

    let config = root.join("mdpp.toml");
    let text = std::fs::read_to_string(&config).expect("读得动");
    std::fs::write(
        &config,
        format!("{text}\nfavicon = \"assets/icon.png\"\n"),
    )
    .expect("改得下");

    // 以前这一条会挂在「stream did not contain valid UTF-8」上
    let out = mdpp(&root, &["build"]);
    assert!(
        out.status.success(),
        "二进制 favicon 该能 build：{}",
        stderr_of(&out)
    );

    // 文件**逐字节**进了 `assets/`
    let copied = std::fs::read(root.join("build/assets/icon.png")).expect("读得动");
    assert_eq!(copied, PNG, "拷出来的字节该与源文件一致");

    // 每一页都带上那个 `<link>`，且地址按各自深度数对
    let home = std::fs::read_to_string(root.join("build/index.html")).expect("读得动");
    assert!(
        home.contains(r#"<link rel="icon" href="assets/icon.png">"#),
        "{home}"
    );

    // 深层页：地址得退够层数（少退一层图标就 404，而页面照样出得来、不报错）
    assert!(mdpp(&root, &["new", "指南/安装"]).status.success());
    assert!(mdpp(&root, &["build"]).status.success());
    let deep =
        std::fs::read_to_string(root.join("build/指南/安装/index.html")).expect("读得动");
    assert!(
        deep.contains(r#"<link rel="icon" href="../../assets/icon.png">"#),
        "深层页的图标地址要数对层数：{deep}"
    );
}

/// 不配 `favicon` 就没有那个 `<link>` —— 浏览器自己画一个，而不是空图标
#[test]
fn without_a_favicon_there_is_no_icon_link() {
    let dir = empty_dir();
    let root = init(dir.path());
    assert!(mdpp(&root, &["build"]).status.success());
    let home = std::fs::read_to_string(root.join("build/index.html")).expect("读得动");
    assert!(!home.contains("rel=\"icon\""), "{home}");
}

/// 配了却读不到就**报错** —— 那是一个笔误，而一个读不到的小图标只会静悄悄地让
/// 浏览器画一个默认的
#[test]
fn a_favicon_that_cannot_be_read_is_refused() {
    let dir = empty_dir();
    let root = init(dir.path());
    let config = root.join("mdpp.toml");
    let text = std::fs::read_to_string(&config).expect("读得动");
    std::fs::write(
        &config,
        format!("{text}\nfavicon = \"assets/没有这个.png\"\n"),
    )
    .expect("改得下");

    let out = mdpp(&root, &["build"]);
    assert!(!out.status.success(), "读不到的 favicon 该报错");
    let message = stderr_of(&out);
    assert!(message.contains("favicon"), "{message}");
}

/// `-C, --dir`：**站在项目外头也能干活** —— 找项目、建页、出产物都以它为起点，
/// 而不是以进程此刻所在的目录为起点
#[test]
fn the_workdir_flag_lets_you_work_from_outside_the_project() {
    let dir = empty_dir();
    let root = init(dir.path());
    // 进程故意待在项目外面：不给 `-C` 的话，这里连 `mdpp.toml` 都找不着
    let elsewhere = dir.path().join("别处");
    std::fs::create_dir(&elsewhere).expect("造得出别处");
    let at = root.display().to_string();

    let out = mdpp(&elsewhere, &["-C", &at, "new", "甲"]);
    assert!(out.status.success(), "new 失败：{}", stderr_of(&out));
    assert!(root.join("pages/甲.mdpp").is_file(), "那一页该落在项目里");
    assert!(!elsewhere.join("pages").exists(), "不该落到进程当前目录下");

    let out = mdpp(&elsewhere, &["-C", &at, "build"]);
    assert!(out.status.success(), "build 失败：{}", stderr_of(&out));
    assert!(root.join("build/index.html").is_file(), "产物该落在项目里");
    assert!(!elsewhere.join("build").exists(), "产物不该长在别处");
}

/// `-C` 也管 `render` 的**输入文件**：相对路径从它算 —— 于是
/// `mdpp -C 某处 render 某页.md` 在哪儿敲都一样
#[test]
fn the_workdir_flag_resolves_render_inputs_for_you() {
    let dir = empty_dir();
    std::fs::write(dir.path().join("甲.md"), "# 标题一行\n").expect("写得下");
    let elsewhere = dir.path().join("别处");
    std::fs::create_dir(&elsewhere).expect("造得出别处");
    let at = dir.path().display().to_string();

    let out = mdpp(&elsewhere, &["-C", &at, "render", "甲.md"]);
    assert!(out.status.success(), "render 失败：{}", stderr_of(&out));
    let html = stdout_of(&out);
    assert!(html.contains("<h1"), "{html}");
    assert!(html.contains("标题一行"), "{html}");
}

/// 目录进不去就**当场**说清是哪个目录 —— 不是让后面每一条路都去报「找不到 mdpp.toml」
#[test]
fn the_workdir_flag_says_so_when_the_directory_is_not_there() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["-C", "压根没有的目录", "build"]);
    assert!(!out.status.success(), "进不去就该失败");
    let message = stderr_of(&out);
    assert!(message.contains("压根没有的目录"), "{message}");
    assert!(
        !message.contains("mdpp.toml"),
        "还没走到找项目那一步：{message}"
    );
}

/// `-C` 与 `init` 后面那个 `[目录]` 是**两件事**：一个说站在哪儿、一个说建到哪儿，
/// 后者相对前者算 —— `mdpp -C 某处 init 子站` 就是 `某处/子站`
#[test]
fn init_builds_into_the_workdir_rather_than_the_process_dir() {
    let dir = empty_dir();
    let base = dir.path().join("某处");
    std::fs::create_dir(&base).expect("造得出某处");
    let elsewhere = dir.path().join("别处");
    std::fs::create_dir(&elsewhere).expect("造得出别处");

    let at = base.display().to_string();
    let out = mdpp(&elsewhere, &["-C", &at, "init", "子站"]);
    assert!(out.status.success(), "init 失败：{}", stderr_of(&out));
    assert!(
        base.join("子站/mdpp.toml").is_file(),
        "该建在 -C 那个目录里"
    );
    assert!(!elsewhere.join("子站").exists(), "不该长在进程当前目录下");
}
