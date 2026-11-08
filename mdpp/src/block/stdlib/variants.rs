//! 变体组原语：`::variants`。
//!
//! 它是 `theme` / `tabs` 那一类块的底座：**N 份正文 + 一个选中状态**。状态机与
//! 资源作用域在这一层；DOM 的类名与标题文字由标准库模板给。
//!
//! ## 两种控制来源
//!
//! - `control=theme`：没有按钮栏，选中状态来自块外的主题（`data-mdpp-theme` /
//!   `prefers-color-scheme`）。所有面板都进 DOM，由样式挑显示哪一节；
//! - `control=choice`：块内生成一排按钮，切换由 `mdpp.js` 接上。非默认面板带
//!   `hidden`，`::css` / `::js` 绑到各自选项上，切换时才挂载。
//!
//! ## 参数
//!
//! - `control=`：`theme` / `choice`，必填；
//! - `group=`：组 ID 的前缀（`theme` / `tabs` …），默认 `variants`；
//! - `default=`：默认选项。`theme` 认 `light` / `dark`，`choice` 认下标；
//! - `class=` / `bar-class=` / `button-class=` / `button-on-class=` /
//!   `panel-class=`：各部分的类名，全由模板传入；
//! - `panel-variant-class=on`：给面板再追加 `--<选项值>` 的变体类
//!   （`theme__part--light` 就是它生成的）；
//! - `label=`：报错时的显示名（「选项卡」「深浅色主题」）。
//!
//! 所有分节与参数在**写出任何标签之前**验完。与 `::disclose` 一样，这是它留在
//! Rust 里的主要理由。

use std::collections::HashSet;

use markdown_it::{Node, Renderer};

use super::super::dispatch::{render_problem, sections};
use crate::registry::Block;
use crate::variants::{ALWAYS, Control, Group};

/// `::variants` 认的全部参数。
const PARAMS: &[&str] = &[
    "control",
    "group",
    "default",
    "class",
    "bar-class",
    "button-class",
    "button-on-class",
    "panel-class",
    "panel-variant-class",
    "label",
];

/// 解析后的参数。全部借用 `block`。
struct Params<'a> {
    control: Control,
    group: &'a str,
    default: Option<&'a str>,
    class: Option<&'a str>,
    bar_class: Option<&'a str>,
    button_class: Option<&'a str>,
    button_on_class: Option<&'a str>,
    panel_class: Option<&'a str>,
    panel_variant_class: bool,
    label: &'a str,
}

/// 一节 → 一个选项。
struct Part<'a> {
    /// 进 `data-mdpp-option` / `data-mdpp-panel` 的值
    value: String,
    /// 显示用的标签（按钮上那几个字 / 主题名）
    label: &'a str,
    /// 这一节的子节点
    content: &'a Node,
}

fn present(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn on(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "true" | "yes" | "on" | "1"
    )
}

fn parse(block: &Block) -> Result<Params<'_>, String> {
    let mut seen = HashSet::new();
    let mut control = None;
    let mut group = "variants";
    let mut default = None;
    let mut class = None;
    let mut bar_class = None;
    let mut button_class = None;
    let mut button_on_class = None;
    let mut panel_class = None;
    let mut panel_variant_class = false;
    let mut label = "变体";

    for (key, value) in &block.params {
        if !seen.insert(key.as_str()) {
            return Err(format!("`::variants` 的参数 `{key}` 写了两次"));
        }
        match key.as_str() {
            "control" => {
                control = Some(match value.trim().to_lowercase().as_str() {
                    "theme" => Control::Theme,
                    "choice" => Control::Choice,
                    other => {
                        return Err(format!(
                            "`::variants` 的 control= 只认 theme / choice，写成了 `{other}`"
                        ));
                    }
                });
            }
            "group" => group = present(value).unwrap_or("variants"),
            "default" => default = present(value),
            "class" => class = present(value),
            "bar-class" => bar_class = present(value),
            "button-class" => button_class = present(value),
            "button-on-class" => button_on_class = present(value),
            "panel-class" => panel_class = present(value),
            "panel-variant-class" => panel_variant_class = on(value),
            "label" => label = present(value).unwrap_or("变体"),
            other => {
                return Err(format!(
                    "`::variants` 不认参数 `{other}`：只允许 {}",
                    PARAMS.join(" / ")
                ));
            }
        }
    }

    let Some(control) = control else {
        return Err("`::variants` 缺少 control=：写 control=theme 或 control=choice".to_string());
    };

    Ok(Params {
        control,
        group,
        default,
        class,
        bar_class,
        button_class,
        button_on_class,
        panel_class,
        panel_variant_class,
        label,
    })
}

/// 把分节认成选项；`theme` 还会把标签归一。
fn parts<'a>(
    control: Control,
    list: &[(&'a str, &'a Node)],
    params: &Params<'_>,
) -> Result<Vec<Part<'a>>, String> {
    if list.is_empty() {
        return Err(match control {
            Control::Theme => format!("{}是空的：每一节要以 [light] 或 [dark] 开头", params.label),
            _ => format!("{}是空的：每一节要以 [标签] 开头", params.label),
        });
    }

    match control {
        Control::Theme => {
            let mut parts = Vec::new();
            for (label, content) in list {
                let value = match label.to_lowercase().as_str() {
                    "both" | "all" => ALWAYS,
                    "light" => "light",
                    "dark" => "dark",
                    _ => {
                        return Err(format!(
                            "认不出这一节的标签 [{label}]：只认 [light] 与 [dark]；\
                             两边都显示写 [both]"
                        ));
                    }
                };
                parts.push(Part {
                    value: value.to_string(),
                    label,
                    content,
                });
            }
            Ok(parts)
        }
        Control::Choice => Ok(list
            .iter()
            .enumerate()
            .map(|(index, (label, content))| Part {
                value: index.to_string(),
                label,
                content,
            })
            .collect()),
        Control::Manual => Err("`::variants` 暂不支持 control=manual".to_string()),
    }
}

/// 默认值必须真的是这一个组里有的选项；写错了不要静悄悄落到第一个。
fn default_of<'a>(params: &Params<'a>, parts: &[Part<'_>]) -> Result<Option<String>, String> {
    let Some(default) = params.default else {
        return Ok(None);
    };
    match params.control {
        Control::Theme => {
            let value = default.to_lowercase();
            if matches!(value.as_str(), "light" | "dark") {
                Ok(Some(value))
            } else {
                Err("default= 只认 light / dark".to_string())
            }
        }
        Control::Choice => {
            if parts.iter().any(|part| part.value == default) {
                Ok(Some(default.to_string()))
            } else if parts.is_empty() {
                Err("default= 写了，但这一块一个选项也没有".to_string())
            } else {
                Err(format!(
                    "default= 只认这一块里有的选项下标 0..{}，写成了 `{default}`",
                    parts.len() - 1
                ))
            }
        }
        Control::Manual => unreachable!("parts() 已经拒绝了 manual"),
    }
}

/// `::variants control=… group=… default=… class=…`
pub(super) fn render_variants(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let params = match parse(block) {
        Ok(params) => params,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let list = sections(node);
    let parts = match parts(params.control, &list, &params) {
        Ok(parts) => parts,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    let default = match default_of(&params, &parts) {
        Ok(default) => default,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    let mut group = Group::new(params.group, params.control);
    match params.control {
        Control::Theme => {
            group = group.option("light", "light").option("dark", "dark");
        }
        Control::Choice => {
            for part in &parts {
                group = group.option(part.value.clone(), part.label);
            }
        }
        Control::Manual => unreachable!("parts() 已经拒绝了 manual"),
    }
    if let Some(default) = default {
        group = group.default_to(default);
    }

    let mut container: Vec<(&str, String)> = Vec::new();
    if let Some(class) = params.class {
        container.push(("class", class.to_string()));
    }
    container.extend(group.container_attrs());
    fmt.cr();
    fmt.open("div", &container);
    fmt.cr();

    if params.control == Control::Choice {
        let mut bar: Vec<(&str, String)> = Vec::new();
        if let Some(class) = params.bar_class {
            bar.push(("class", class.to_string()));
        }
        bar.push(("role", "tablist".to_string()));
        fmt.open("div", &bar);
        for part in &parts {
            let selected = part.value == group.default_value();
            let mut class = String::new();
            if let Some(base) = params.button_class {
                class.push_str(base);
            }
            if selected && let Some(active) = params.button_on_class {
                if !class.is_empty() {
                    class.push(' ');
                }
                class.push_str(active);
            }
            let mut attrs: Vec<(&str, String)> = Vec::new();
            if !class.is_empty() {
                attrs.push(("class", class));
            }
            attrs.push(("type", "button".to_string()));
            attrs.push(("role", "tab".to_string()));
            // 旧版钩子；新代码认 `data-mdpp-option`。
            attrs.push(("data-tab", part.value.clone()));
            attrs.push((
                "aria-selected",
                if selected { "true" } else { "false" }.to_string(),
            ));
            attrs.extend(group.option_attrs(&part.value));
            fmt.open("button", &attrs);
            fmt.text(part.label);
            fmt.close("button");
        }
        fmt.cr();
        fmt.close("div");
        fmt.cr();
    }

    for part in &parts {
        let mut class = String::new();
        if let Some(base) = params.panel_class {
            class.push_str(base);
            if params.panel_variant_class && part.value != ALWAYS {
                // 变体类是**基类的最后一个**加后缀：`theme__part` 落成
                // `theme__part theme__part--light`，两个类都留着
                let variant = base.split_whitespace().last().unwrap_or(base);
                class.push(' ');
                class.push_str(variant);
                class.push_str("--");
                class.push_str(&part.value);
            }
        }
        let mut attrs: Vec<(&str, String)> = Vec::new();
        if !class.is_empty() {
            attrs.push(("class", class));
        }
        match params.control {
            Control::Choice => {
                attrs.push(("role", "tabpanel".to_string()));
                // 旧版钩子；新代码认 `data-mdpp-panel`。
                attrs.push(("data-panel", part.value.clone()));
                // 打印时每一节都要有标题（见 `mdpp.css` 的 `@media print`）
                attrs.push(("data-label", part.label.to_string()));
                attrs.extend(group.panel_attrs(&part.value));
            }
            Control::Theme => {
                attrs.push(("data-mdpp-value", part.value.clone()));
            }
            Control::Manual => unreachable!("parts() 已经拒绝了 manual"),
        }
        fmt.open("div", &attrs);
        fmt.cr();
        // 这一节里产生的 ::css / ::js 绑到当前选项上，切换时才挂载。
        let _scope = group.enter(&part.value);
        fmt.contents(&part.content.children);
        fmt.cr();
        fmt.close("div");
    }

    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// theme / tabs 现在走标准库模板；这里顺带钉住模板与 primitive 的接缝
    #[test]
    fn standard_library_templates_use_the_primitive() {
        let tabs = render("::tabs\n  [甲]\n    甲文\n  [乙]\n    乙文\n");
        assert!(tabs.contains(r#"class="tabs__bar""#), "{tabs}");
        assert!(tabs.contains(r#"class="tabs__panel""#), "{tabs}");
        assert!(tabs.contains("data-mdpp-option=\"0\""), "{tabs}");
        assert!(tabs.contains("甲文") && tabs.contains("乙文"), "{tabs}");

        let theme = render("::theme\n  [light]\n    浅色\n  [dark]\n    深色\n");
        assert!(theme.contains("theme__part--light"), "{theme}");
        assert!(theme.contains("theme__part--dark"), "{theme}");
        assert!(theme.contains("data-mdpp-value=\"light\""), "{theme}");
    }

    /// `control=theme`：不生成按钮栏，两节都在 DOM 里
    #[test]
    fn theme_has_no_control_bar_and_keeps_every_panel() {
        let html = render(
            "::variants control=theme group=theme class=theme panel-class=theme__part \
             panel-variant-class=on\n  [light]\n    甲\n  [dark]\n    乙\n",
        );
        assert!(html.contains(r#"class="theme""#), "{html}");
        assert!(html.contains(r#"data-mdpp-control="theme""#), "{html}");
        assert!(!html.contains("<button"), "{html}");
        assert!(html.contains("theme__part--light"), "{html}");
        assert!(html.contains("theme__part--dark"), "{html}");
        assert!(html.contains("data-mdpp-value=\"light\""), "{html}");
        assert!(html.contains("甲") && html.contains("乙"), "{html}");
    }

    /// `control=choice`：按钮、面板、默认值与旧 `data-*` 钩子
    #[test]
    fn choice_renders_buttons_panels_and_legacy_hooks() {
        let html = render(
            "::variants control=choice group=tabs class=tabs bar-class=tabs__bar \
             button-class=tabs__tab button-on-class=tabs__tab--on panel-class=tabs__panel\n  [甲]\n    甲文\n  [乙]\n    乙文\n",
        );
        assert!(html.contains(r#"data-mdpp-control="choice""#), "{html}");
        assert!(
            html.contains(r#"class="tabs__tab tabs__tab--on""#),
            "{html}"
        );
        assert!(html.contains(r#"aria-selected="true""#), "{html}");
        assert!(html.contains(r#"data-tab="0""#), "{html}");
        assert!(html.contains(r#"data-panel="1""#), "{html}");
        assert!(html.contains(r#"data-label="乙""#), "{html}");
        assert!(html.contains("hidden"), "{html}");
    }

    /// 标签是显示用的字，原样保留
    #[test]
    fn labels_keep_their_case() {
        let html = render("::variants control=choice\n  [API]\n    内容\n");
        assert!(html.contains(">API</button>"), "{html}");
    }

    /// 未知 `control=`、缺 `control=`、未知标签、坏 `default=` 都整块拒掉
    #[test]
    fn bad_headers_are_reported_before_anything_is_written() {
        for source in [
            "::variants\n  [甲]\n    甲\n",
            "::variants control=manual\n  [甲]\n    甲\n",
            "::variants control=theme\n  [blue]\n    甲\n",
            "::variants control=choice default=9\n  [甲]\n    甲\n",
            "::variants control=choice\n  没标签\n  [甲]\n    甲\n",
        ] {
            let html = render(source);
            assert!(html.contains("template--problem"), "{source:?}: {html}");
        }
    }

    /// 分节里的资源按选项延迟；`theme` 的前缀节属于 `*`
    #[test]
    fn assets_are_scoped_to_the_option() {
        let html = render(
            "::variants control=choice group=tabs class=tabs\n  [甲]\n    ::css\n      .a { }\n  [乙]\n    ::js\n      console.log(1);\n",
        );
        assert!(html.contains("data-mdpp-group=\"tabs-1\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"0\""), "{html}");
        assert!(html.contains("data-mdpp-asset=\"css\""), "{html}");
        assert!(html.contains("data-mdpp-asset=\"js\""), "{html}");

        let both = render(
            "::variants control=theme group=theme class=theme\n  [both]\n    两边都显示\n    ::css\n      .a { }\n  [dark]\n    深色\n",
        );
        assert!(both.contains("data-mdpp-value=\"*\""), "{both}");
    }

    /// 一节里还能再嵌一个选项卡（各节是各自解析的小文档）
    #[test]
    fn tabs_nest() {
        let nested =
            render("::tabs\n  [甲]\n    ::tabs\n      [内]\n        内文\n  [乙]\n    乙文\n");
        assert_eq!(nested.matches("class=\"tabs\"").count(), 2, "{nested}");
        assert!(
            nested.contains("内文") && nested.contains("乙文"),
            "{nested}"
        );
    }

    /// 未选中的面板先藏着，一节里的正文仍按 markdown 解析
    #[test]
    fn tabs_hide_the_other_panels_and_parse_markdown() {
        let html = render("::tabs\n  [一]\n    **加粗**\n  [二]\n    乙\n");
        assert_eq!(html.matches("class=\"tabs__tab").count(), 2, "{html}");
        assert!(html.contains("<strong>加粗</strong>"), "{html}");
        assert!(html.contains("hidden"), "{html}");
    }

    /// `theme` 的 `::css` / `::js` / `::html js` 都变成惰性资源
    #[test]
    fn theme_assets_are_deferred() {
        let html = render(
            "::theme\n  [light]\n    ::css\n      .a { color: red; }\n    ::js\n      console.log('light');\n    ::html js\n      <script>alert('light')</script>\n  [dark]\n    ::css\n      .b { color: blue; }\n",
        );
        assert!(html.contains("data-mdpp-asset=\"css\""), "{html}");
        assert!(html.contains("data-mdpp-asset=\"js\""), "{html}");
        assert!(html.contains("data-mdpp-group=\"theme-1\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"light\""), "{html}");
        assert!(html.contains("data-mdpp-value=\"dark\""), "{html}");
        assert!(!html.contains("<style>"), "{html}");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("console.log('light');"), "{html}");
        assert!(html.contains("alert('light')"), "{html}");
    }

    /// 块外的 `::css` 仍直接输出 `<style>`，不受变体作用域影响
    #[test]
    fn css_outside_a_group_still_outputs_style() {
        let html = render("::css\n  .a { color: red; }\n");
        assert!(html.contains("<style>"), "{html}");
        assert!(!html.contains("data-mdpp-asset"), "{html}");
    }

    /// 站点模式下 theme 的资源也进清单，页面里先给静态链接占位符
    #[test]
    fn theme_assets_are_collected_as_static_files() {
        use crate::{Mdpp, RenderContext};

        let rendered = Mdpp::new().render_with_assets(
            "::theme\n  [light]\n    ::css\n      .a { }\n    ::js\n      console.log(1);\n  [dark]\n    ::css\n      .b { }\n",
            &RenderContext::new(),
            "assets",
        );
        assert_eq!(rendered.assets.len(), 3, "{:?}", rendered.assets);
        assert!(
            rendered.html.contains("data-mdpp-asset=\"css\""),
            "{}",
            rendered.html
        );
        assert!(
            rendered.html.contains("data-mdpp-href=\"assets/mdpp-"),
            "{}",
            rendered.html
        );
        assert!(!rendered.html.contains("<link"), "{}", rendered.html);
        assert!(!rendered.html.contains("<script src="), "{}", rendered.html);
    }

    /// `::js once` 把一次性策略写进占位符
    #[test]
    fn tabs_can_mark_a_script_once() {
        let html = render("::tabs\n  [一]\n    ::js once\n      console.log(1);\n");
        assert!(html.contains("data-mdpp-once=\"once\""), "{html}");
    }
}
