use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// `[status_bar]` 分节：桌面上常驻的悬浮中 / 英指示器。
/// Windows 侧是显示「中 / 英」文字的悬浮状态条（可拖动，位置记在这里）；
/// macOS 侧是同一份配置驱动的**圆点指示器**（颜色 / 形状 / 大小可调，不抢焦点、点击穿透）。
/// 与菜单栏的「中 / 英」状态项并存，各是一条。输入法不是青简时整个收起来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusBarConfig {
    /// 是否显示悬浮指示器。缺省关。
    pub enabled: bool,

    /// 记住的屏幕横坐标（内容左上角物理像素）；没拖动过为 `None`，首次按 [`Self::anchor`] 摆放。
    pub x: Option<i32>,

    /// 记住的屏幕纵坐标（内容左上角物理像素）。
    pub y: Option<i32>,

    /// 没拖动过（或还没实现拖动）时的摆放位置。缺省右下角。
    pub anchor: Anchor,

    /// 离 [`Self::anchor`] 指的那两条边各留多少点。
    pub offset_x: i32,
    pub offset_y: i32,

    /// 指示器的形状。
    pub shape: Shape,

    /// 指示器的大小（点：圆形的直径 / 方形的边长）。
    pub size: i32,

    /// 中文输入时的颜色。
    pub chinese_color: Color,

    /// 英文输入时的颜色。
    pub english_color: Color,

    /// 描一圈白色轮廓：桌面上背景颜色不定时（深色壁纸、浅色窗口）也看得清。缺省开。
    pub outline: bool,

    /// 菜单栏那个「中 / 英 ☁︎」状态项显示不显示（缺省显示）。关掉之后模式提示交给桌面圆点。
    ///
    /// 关掉时是 `setVisible(false)` 真的从菜单栏拿掉 —— 只收成零宽不行：这版系统上会留一个小黑块
    /// （2026-10-07 用户报的）。代价是再打开时位置可能被系统排到最左边；
    /// 而「输入源不是青简就收起」（`visibility`）那条仍走零宽 + 藏按钮，位置保留。
    pub menubar_item: bool,

    /// 什么时候显示。缺省 `follow`：**系统输入源不是青简就立刻收起**。
    /// 另外两档是可选项：`sticky`（离开后延迟 1.5 秒再收，切应用时不闪）、`always`（只看总开关）。
    pub visibility: Visibility,

    /// 切换中 / 英时在光标处提示一句（「英文输入」「中文输入」）。缺省开；
    /// 觉得啰嗦就关掉 —— 指示器的颜色本身已经说明了模式。
    pub notice: bool,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            x: None,
            y: None,
            anchor: Anchor::BottomRight,
            offset_x: 24,
            offset_y: 24,
            shape: Shape::Circle,
            size: 12,
            chinese_color: Color::CHINESE,
            english_color: Color::ENGLISH,
            outline: true,
            menubar_item: true,
            visibility: Visibility::Follow,
            notice: true,
        }
    }
}

/// 悬浮指示器什么时候显示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    /// 只跟着「系统当前输入源是不是青简」：不是就立刻收（最准，但切应用时会闪）。
    Follow,

    /// 跟随，但离开青简后**等一会儿**再收（可选）：切应用引起的瞬时跳动不再让圆点闪。
    Sticky,

    /// 只要指示器开着就一直显示（切到别的输入法也不收）：当装饰用，不再指示「能不能打中文」。
    Always,
}

impl Visibility {
    pub const ALL: [Self; 3] = [Self::Follow, Self::Sticky, Self::Always];

    pub fn name(self) -> &'static str {
        match self {
            Self::Follow => "跟随输入源",
            Self::Sticky => "跟随，但延迟收起",
            Self::Always => "一直显示",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Follow => "follow",
            Self::Sticky => "sticky",
            Self::Always => "always",
        }
    }

    /// 离开青简之后等多久才收起（`Follow` 就是 0）。
    /// `always` 下「当前输入源不是青简」时的透明度：淡化表示它现在没在管事。
    /// 其余两档不需要——那时不显示才是正确状态。
    pub fn inactive_alpha(self) -> f64 {
        match self {
            Self::Always => 0.35,
            _ => 1.0,
        }
    }

    pub fn hide_delay(self) -> std::time::Duration {
        match self {
            Self::Follow => std::time::Duration::ZERO,
            Self::Sticky => std::time::Duration::from_millis(1500),
            Self::Always => std::time::Duration::MAX,
        }
    }
}

impl StatusBarConfig {
    /// 大小夹到能看的范围：太小看不见，太大像块膏药。
    pub fn size(&self) -> f64 {
        f64::from(self.size.clamp(6, 48))
    }
}

/// 指示器贴屏幕哪一角 / 哪条边。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// 设置界面里列出的顺序与名字。
    pub const ALL: [Self; 9] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::BottomLeft,
        Self::Bottom,
        Self::BottomRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "左上",
            Self::Top => "上中",
            Self::TopRight => "右上",
            Self::Left => "左中",
            Self::Center => "正中",
            Self::Right => "右中",
            Self::BottomLeft => "左下",
            Self::Bottom => "下中",
            Self::BottomRight => "右下",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::Top => "top",
            Self::TopRight => "top-right",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::BottomLeft => "bottom-left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom-right",
        }
    }

    /// 内容左上角在「可见区域」里的坐标。`visible` 是 `(x, y, 宽, 高)`（点，原点左下）。
    /// AppKit 原点在左下，所以「上」对应 `y + 高 - 偏移 - 尺寸`。
    pub fn origin(
        self,
        visible: (f64, f64, f64, f64),
        size: f64,
        offset: (f64, f64),
    ) -> (f64, f64) {
        let (x, y, width, height) = visible;
        let (ox, oy) = offset;
        let left = x + ox;
        let right = x + width - ox - size;
        let top = y + height - oy - size;
        let bottom = y + oy;
        let center_x = x + (width - size) / 2.0;
        let center_y = y + (height - size) / 2.0;
        match self {
            Self::TopLeft => (left, top),
            Self::Top => (center_x, top),
            Self::TopRight => (right, top),
            Self::Left => (left, center_y),
            Self::Center => (center_x, center_y),
            Self::Right => (right, center_y),
            Self::BottomLeft => (left, bottom),
            Self::Bottom => (center_x, bottom),
            Self::BottomRight => (right, bottom),
        }
    }
}

/// 指示器的形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    /// 圆点。
    Circle,

    /// 方块。
    Square,

    /// 圆角方块。
    Rounded,
}

impl Shape {
    pub const ALL: [Self; 3] = [Self::Circle, Self::Square, Self::Rounded];

    pub fn name(self) -> &'static str {
        match self {
            Self::Circle => "圆点",
            Self::Square => "方块",
            Self::Rounded => "圆角方块",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Square => "square",
            Self::Rounded => "rounded",
        }
    }

    /// 圆角半径；圆形就是边长的一半。
    pub fn corner_radius(self, size: f64) -> f64 {
        match self {
            Self::Circle => size / 2.0,
            Self::Square => 0.0,
            Self::Rounded => size / 4.0,
        }
    }
}

/// 配置里的颜色写法：`#RRGGBB`。也可以用 `red` / `green` / `blue` / `white` / `black` 这类名字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    /// 中文：偏红。
    pub const CHINESE: Self = Self {
        red: 0xE5,
        green: 0x48,
        blue: 0x4D,
    };

    /// 英文：偏绿。
    pub const ENGLISH: Self = Self {
        red: 0x46,
        green: 0xA7,
        blue: 0x58,
    };

    /// 设置界面里做出来的几个快捷色。
    pub const PRESETS: [(&'static str, Self); 6] = [
        ("红", Self::CHINESE),
        ("绿", Self::ENGLISH),
        (
            "蓝",
            Self {
                red: 0x3E,
                green: 0x63,
                blue: 0xF6,
            },
        ),
        (
            "橙",
            Self {
                red: 0xF7,
                green: 0x6B,
                blue: 0x26,
            },
        ),
        (
            "白",
            Self {
                red: 0xFF,
                green: 0xFF,
                blue: 0xFF,
            },
        ),
        (
            "黑",
            Self {
                red: 0x18,
                green: 0x18,
                blue: 0x1B,
            },
        ),
    ];

    /// `#RRGGBB`。
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::CHINESE
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

impl FromStr for Color {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();
        let lower = trimmed.to_ascii_lowercase();
        // 名字：常用的几个，省得记十六进制
        let named = match lower.as_str() {
            "red" | "红" => Some(Self::CHINESE),
            "green" | "绿" => Some(Self::ENGLISH),
            "blue" | "蓝" => Some(Self::PRESETS[2].1),
            "orange" | "橙" => Some(Self::PRESETS[3].1),
            "white" | "白" => Some(Self::PRESETS[4].1),
            "black" | "黑" => Some(Self::PRESETS[5].1),
            _ => None,
        };
        if let Some(color) = named {
            return Ok(color);
        }
        let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "颜色要写成 #RRGGBB（或 red / green / blue …），收到 {trimmed:?}"
            ));
        }
        let byte = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).unwrap_or(0);
        Ok(Self {
            red: byte(0),
            green: byte(2),
            blue: byte(4),
        })
    }
}

impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod inactive_alpha_tests {
    use super::Visibility;

    /// 只有 `always` 需要淡化：其余两档"不显示"才是正确状态。
    #[test]
    fn only_always_dims_when_not_ours() {
        assert_eq!(Visibility::Always.inactive_alpha(), 0.35);
        assert_eq!(Visibility::Follow.inactive_alpha(), 1.0);
        assert_eq!(Visibility::Sticky.inactive_alpha(), 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_round_trip_between_hex_and_names() {
        let color: Color = "#e5484d".parse().unwrap();
        assert_eq!(color, Color::CHINESE);
        assert_eq!(color.hex(), "#E5484D");
        assert_eq!("red".parse::<Color>().unwrap(), Color::CHINESE);
        assert_eq!("绿".parse::<Color>().unwrap(), Color::ENGLISH);
        assert!("nope".parse::<Color>().is_err());
        assert!("#12345".parse::<Color>().is_err());
        // 配置里写的是十六进制（字符串），读回来是颜色
        let parsed: StatusBarConfig = toml::from_str("chinese_color = \"#112233\"\n").unwrap();
        assert_eq!(parsed.chinese_color.hex(), "#112233");
        assert_eq!(parsed.english_color, Color::ENGLISH, "没写的用缺省");
    }

    #[test]
    fn anchors_sit_inside_the_visible_area() {
        let visible = (0.0, 0.0, 1000.0, 800.0);
        let size = 12.0;
        let offset = (24.0, 24.0);
        // 右下角：右边留 24、下边留 24
        assert_eq!(
            Anchor::BottomRight.origin(visible, size, offset),
            (1000.0 - 24.0 - 12.0, 24.0)
        );
        // 左上角：左边 24、上边 24（AppKit 原点在左下）
        assert_eq!(
            Anchor::TopLeft.origin(visible, size, offset),
            (24.0, 800.0 - 24.0 - 12.0)
        );
        // 正中：居中
        assert_eq!(Anchor::Center.origin(visible, size, offset), (494.0, 394.0));
    }

    #[test]
    fn size_is_clamped() {
        let config = StatusBarConfig {
            size: 1000,
            ..StatusBarConfig::default()
        };
        assert_eq!(config.size(), 48.0);
        let config = StatusBarConfig {
            size: 1,
            ..StatusBarConfig::default()
        };
        assert_eq!(config.size(), 6.0);
    }
}
