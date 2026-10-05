//! 「指示器」页：桌面悬浮中 / 英指示器（`[status_bar]`）的开关、位置、形状、大小与两个颜色。
//!
//! 颜色用 AppKit 的取色器（`NSColorWell`，点一下弹系统调色板），位置除了九宫格还有 X / Y 偏移。

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSColorWell, NSPopUpButton, NSTextField};
use objc2_foundation::NSString;

use crate::indicator::native_color;
use crate::preferences::controls::{
    checkbox, color_well, note, row_checkbox, row_control, row_popup, select, set_checked,
    set_color, text_field,
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
    chinese_color: Retained<NSColorWell>,
    english_color: Retained<NSColorWell>,
    notice: Retained<NSButton>,
    offset_x: Retained<NSTextField>,
    offset_y: Retained<NSTextField>,
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
        let chinese_color = color_well(mtm, Setting::StatusBarChineseColor, target);
        row_control(layout, mtm, "中文颜色", &chinese_color);
        let english_color = color_well(mtm, Setting::StatusBarEnglishColor, target);
        row_control(layout, mtm, "英文颜色", &english_color);
        note(
            layout,
            mtm,
            "点色块弹系统调色板（选完立刻生效）。配置文件里对应 `#RRGGBB`，也认 red / green / blue / orange / white / black。",
        );
        let offset_x = text_field(mtm, Setting::StatusBarOffsetX, target);
        row_control(layout, mtm, "X 偏移", &offset_x);
        let offset_y = text_field(mtm, Setting::StatusBarOffsetY, target);
        row_control(layout, mtm, "Y 偏移", &offset_y);
        note(
            layout,
            mtm,
            "离「位置」那条边留多少点（按回车保存）；偏移是相对九宫格算的，中心那格也按它平移。",
        );
        let notice = checkbox(
            mtm,
            "切换中 / 英时在光标处提示一句（「英文输入」「中文输入」）",
            Setting::StatusBarNotice,
            target,
        );
        row_checkbox(layout, &notice);
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
            notice,
            offset_x,
            offset_y,
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
        set_color(&self.chinese_color, &native_color(bar.chinese_color));
        set_color(&self.english_color, &native_color(bar.english_color));
        set_checked(&self.notice, bar.notice);
        self.offset_x
            .setStringValue(&NSString::from_str(&bar.offset_x.to_string()));
        self.offset_y
            .setStringValue(&NSString::from_str(&bar.offset_y.to_string()));
        let on = bar.enabled;
        for control in [
            &self.anchor as &objc2_app_kit::NSControl,
            &self.shape,
            &self.size,
            &self.chinese_color,
            &self.english_color,
            &self.outline,
            &self.notice,
            &self.offset_x,
            &self.offset_y,
        ] {
            control.setEnabled(on);
        }
    }
}
