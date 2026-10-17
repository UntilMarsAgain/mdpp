//! 一个十六进制颜色，与它在页面上要配的那几样。
//!
//! `mdpp.toml` 里的 `theme_color` 要拿去铺**顶栏那一整块底**，于是两件事得当场定下来：
//!
//! - **压在底上的字**：亮底配近黑、暗底配白。CSS 里没有可靠的亮度函数 —— 等浏览器
//!   算就晚了，也等于给每个浏览器出一道不同的题；
//! - **深色那一档**：亮块在暗页面上刺眼。`--mdpp-accent` 在深色下是 `#6ea8ff` 那种
//!   浅蓝（它是**前景色**，拿来铺底就露馅），所以深色下统一往黑里混一档，
//!   mkdocs-material 对 primary 干的也是这件事。
//!
//! 两样都在**构建期**算好，产物里只有几个十六进制串 —— 页面里没有一处算术。

/// 一个 sRGB 颜色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
}

/// 白：`--mdpp-bg` 那个 `#ffffff`
const WHITE: Color = Color {
    r: 0xff,
    g: 0xff,
    b: 0xff,
};

/// 近黑：`--mdpp-text` 那个 `#1a1c1f`。不取纯黑 —— 纯黑压在有色底上比正文的黑还硬
const NEAR_BLACK: Color = Color {
    r: 0x1a,
    g: 0x1c,
    b: 0x1f,
};

/// 深色那一档往黑里混多少。0.78 是「压暗一档」：色相不动，只暗下来
const DARKEN: f64 = 0.78;

/// WCAG 那个「白字与黑字哪个对比高」的分界（√(1.05×0.05) − 0.05 ≈ 0.179）
const BLACK_TEXT_BELOW: f64 = 0.179;

impl Color {
    /// 解析 `#rgb` 与 `#rrggbb`（大小写都认，前后空白不算数）。别的写法一律 `None` ——
    /// 名字（`red`）与函数（`rgb(…)`）不认：它们能塞进字符串的东西比颜色多，而这个值
    /// 要落进 `<style>` 与 `<meta>`，越窄越好
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().strip_prefix('#')?;
        // 先挡住 `#+ff` 这种：`from_str_radix` 认正负号，只看长度会把它当三个数位
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digits = u32::from_str_radix(hex, 16).ok()?;
        // `#rgb` 是每一份都写两遍（`a` → `aa`）
        let doubled = |part: u32| (part * 0x11) as u8;
        Some(match hex.len() {
            3 => Color {
                r: doubled((digits >> 8) & 0xf),
                g: doubled((digits >> 4) & 0xf),
                b: doubled(digits & 0xf),
            },
            6 => Color {
                r: (digits >> 16) as u8,
                g: (digits >> 8) as u8,
                b: digits as u8,
            },
            _ => return None,
        })
    }

    /// `#rrggbb`，小写。进 CSS 与 `<meta name="theme-color">` 都用它
    pub fn css(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// 深色页面上那一档：同一个颜色，往黑里混一档
    pub fn darkened(&self) -> Self {
        let down = |channel: u8| (f64::from(channel) * DARKEN) as u8;
        Color {
            r: down(self.r),
            g: down(self.g),
            b: down(self.b),
        }
    }

    /// 压在这块底上的字：白，或近黑。判据是相对亮度 —— 谁的对比高用谁
    pub fn on_color(&self) -> Self {
        if self.luminance() > BLACK_TEXT_BELOW {
            NEAR_BLACK
        } else {
            WHITE
        }
    }

    /// WCAG 的相对亮度：sRGB 每个通道先线性化，再按人眼那三个权重加权
    fn luminance(&self) -> f64 {
        fn linear(channel: u8) -> f64 {
            let c = f64::from(channel) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只认那两种十六进制写法 —— 这个值要落进 `<style>` 与 `<meta>`，
    /// 名字与函数写法能塞进去的东西比颜色多
    #[test]
    fn only_hex_colors_get_through() {
        assert_eq!(
            Color::parse("#2f6fd0").map(|c| c.css()),
            Some("#2f6fd0".into())
        );
        assert_eq!(
            Color::parse("#F0C").map(|c| c.css()),
            Some("#ff00cc".into())
        );
        assert_eq!(
            Color::parse("  #2F6FD0 ").map(|c| c.css()),
            Some("#2f6fd0".into())
        );
        for bad in [
            "red",
            "rgb(1,2,3)",
            "2f6fd0",
            "#12345",
            "#1234567",
            "#",
            "#fg0",
            "#+ff",
            "#12 34",
        ] {
            assert!(Color::parse(bad).is_none(), "「{bad}」本该被挡下");
        }
    }

    /// 压在底上的字按**亮度**挑：深底白字、浅底近黑字。
    /// 亮黄那种底上再压白字就看不清了，而那正是「深色下才好看、浅色下糊成一片」的反面
    #[test]
    fn the_text_on_a_color_is_picked_by_luminance() {
        for dark in ["#123456", "#2f6fd0", "#000000"] {
            let color = Color::parse(dark).expect("合法的十六进制");
            assert_eq!(color.on_color().css(), "#ffffff", "{dark} 上该压白字");
        }
        for light in ["#ffd166", "#ffffff", "#e9f6ee"] {
            let color = Color::parse(light).expect("合法的十六进制");
            assert_eq!(color.on_color().css(), "#1a1c1f", "{light} 上该压近黑字");
        }
    }

    /// 深色那一档是同一个颜色往黑里混一档 —— 不是换个色相，也不是一刀切地变灰
    #[test]
    fn the_dark_variant_is_the_same_color_a_shade_darker() {
        assert_eq!(
            Color::parse("#2f6fd0").expect("合法").darkened().css(),
            "#2456a2"
        );
        // 黑压不出更黑，白压下去也只是灰 —— 这两头保证「混黑」不会凭空造色
        assert_eq!(
            Color::parse("#000000").expect("合法").darkened().css(),
            "#000000"
        );
        assert_eq!(
            Color::parse("#ffffff").expect("合法").darkened().css(),
            "#c6c6c6"
        );
    }
}
