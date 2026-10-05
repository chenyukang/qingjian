//! 「指示器」页：桌面悬浮中 / 英指示器（`[status_bar]`）的开关、位置、形状、大小与两个颜色。
//!
//! 颜色用文本框（`#RRGGBB`，也认 red / green 这类名字）—— AppKit 的取色器控件要另接一套
//! 「值不是字符串」的设置通道，先不引入；颜色本身不影响功能。

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSPopUpButton, NSTextField};
use objc2_foundation::NSString;

use crate::preferences::controls::{
    checkbox, note, row_checkbox, row_control, row_popup, select, set_checked, text_field,
};
use crate::preferences::layout::{Layout, ROW_HEIGHT};
use crate::preferences::setting::Setting;
use crate::preferences::target::PreferencesTarget;
use qingjian_platform::{Anchor, Config, Shape};

/// 大小下拉里给的几档（点）。
const SIZES: [i32; 8] = [8, 10, 12, 14, 16, 20, 24, 30];

pub struct IndicatorPage {
    enabled: Retained<NSButton>,
    outline: Retained<NSButton>,
    anchor: Retained<NSPopUpButton>,
    shape: Retained<NSPopUpButton>,
    size: Retained<NSPopUpButton>,
    chinese_color: Retained<NSTextField>,
    english_color: Retained<NSTextField>,
}

impl IndicatorPage {
    pub fn build(
        layout: &mut Layout,
        mtm: MainThreadMarker,
        target: &Retained<PreferencesTarget>,
    ) -> Self {
        let enabled = checkbox(
            mtm,
            "显示桌面悬浮指示器（当前输入法不是青简时自动隐藏）",
            Setting::StatusBarEnabled,
            target,
        );
        row_checkbox(layout, &enabled);
        note(
            layout,
            mtm,
            "中文一个颜色、英文另一个颜色——不用盯菜单栏。它不抢焦点、点击穿透，压在普通窗口之上；按 ⌃⇧R 或 Caps Lock 切模式时跟着换色。",
        );
        let anchor = row_popup(
            layout,
            mtm,
            "位置",
            &Anchor::ALL.map(|a| a.name().to_owned()),
            Setting::StatusBarAnchor,
            target,
        );
        let shape = row_popup(
            layout,
            mtm,
            "形状",
            &Shape::ALL.map(|s| s.name().to_owned()),
            Setting::StatusBarShape,
            target,
        );
        let size = row_popup(
            layout,
            mtm,
            "大小",
            SIZES.map(|s| format!("{s} 点")).as_ref(),
            Setting::StatusBarSize,
            target,
        );
        let chinese_color = text_field(mtm, Setting::StatusBarChineseColor, target);
        row_control(layout, mtm, "中文颜色", &chinese_color);
        let english_color = text_field(mtm, Setting::StatusBarEnglishColor, target);
        row_control(layout, mtm, "英文颜色", &english_color);
        note(
            layout,
            mtm,
            "颜色写 #RRGGBB，也可以写 red / green / blue / orange / white / black；文本框按回车保存，写完立刻生效。",
        );
        let outline = checkbox(
            mtm,
            "描一圈白边（深色壁纸和浅色窗口上都看得清）",
            Setting::StatusBarOutline,
            target,
        );
        row_checkbox(layout, &outline);
        layout.space(ROW_HEIGHT / 2.0);
        note(
            layout,
            mtm,
            "「位置」是相对屏幕可见区域摆的九宫格；离边上留多少点写在配置文件 [status_bar] 的 offset_x / offset_y 里（缺省 24 / 24）。",
        );
        Self {
            enabled,
            outline,
            anchor,
            shape,
            size,
            chinese_color,
            english_color,
        }
    }

    pub fn sync(&self, config: &Config) {
        let bar = &config.status_bar;
        set_checked(&self.enabled, bar.enabled);
        set_checked(&self.outline, bar.outline);
        select(
            &self.anchor,
            Anchor::ALL.iter().position(|a| *a == bar.anchor),
        );
        select(&self.shape, Shape::ALL.iter().position(|s| *s == bar.shape));
        select(&self.size, SIZES.iter().position(|s| *s == bar.size));
        self.chinese_color
            .setStringValue(&NSString::from_str(&bar.chinese_color.hex()));
        self.english_color
            .setStringValue(&NSString::from_str(&bar.english_color.hex()));
        let on = bar.enabled;
        for control in [
            &self.anchor as &objc2_app_kit::NSControl,
            &self.shape,
            &self.size,
            &self.chinese_color,
            &self.english_color,
            &self.outline,
        ] {
            control.setEnabled(on);
        }
    }
}
