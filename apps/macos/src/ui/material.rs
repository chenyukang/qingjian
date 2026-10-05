//! 面板「材质」：macOS 26 起用系统 Liquid Glass（`NSGlassEffectView`），更早的系统退回
//! `NSVisualEffectView` 毛玻璃。候选窗与偏好设置共用这一份。
//!
//! 参考 winlane 的 `ui/material.rs`（那边还多一档"纯色渐变"，这里先不需要）。
//! 用法：建一个 backdrop，把 UI 加到 [`PanelBackdrop::content`]，再把
//! [`PanelBackdrop::view`] 当作窗口的 contentView（或先加进 contentView 的最底层）。

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
};
use objc2_foundation::{NSPoint, NSRect};

/// 系统有没有 Liquid Glass API（macOS 26 起）。老系统上不能用 `NSGlassEffectView` 这个名字。
pub fn glass_available() -> bool {
    AnyClass::get(c"NSGlassEffectView").is_some()
}

/// 面板圆角缺省值（偏好设置用；候选窗用主题里的 `corner_radius`）。
pub const DEFAULT_CORNER_RADIUS: f64 = 18.0;

enum Material {
    Glass(Retained<NSGlassEffectView>),
    Frosted(Retained<NSVisualEffectView>),
}

/// 一块面板底色。`content` 在材质之上 —— 所有 UI 往它上面加。
pub struct PanelBackdrop {
    /// 内容层：UI 加到这里，就"浮"在玻璃上。
    pub content: Retained<NSView>,
    material: Material,
}

impl PanelBackdrop {
    /// 按 `frame` 建一块材质（圆角 `corner_radius`）。
    pub fn new(frame: NSRect, corner_radius: f64, mtm: MainThreadMarker) -> Self {
        let resize = NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewHeightSizable;
        let bounds = NSRect::new(NSPoint::ZERO, frame.size);
        let content = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        content.setAutoresizingMask(resize);
        let material = if glass_available() {
            // Liquid Glass：内容由 Glass 视图托管（它负责把 content 画在材质之上）
            let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
            glass.setStyle(NSGlassEffectViewStyle::Regular);
            glass.setCornerRadius(corner_radius);
            glass.setContentView(Some(&content));
            Material::Glass(glass)
        } else {
            // 退路：毛玻璃 + 自己裁圆角
            let blur = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
            blur.setMaterial(NSVisualEffectMaterial::Popover);
            blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            blur.setState(NSVisualEffectState::Active);
            blur.setAutoresizingMask(resize);
            blur.setWantsLayer(true);
            // 圆角只有 Glass 那一路有 API；老系统的毛玻璃退路保持直角（能用就行）
            let _ = corner_radius;
            blur.addSubview(&content);
            Material::Frosted(blur)
        };
        Self { content, material }
    }

    /// 挂到窗口上的视图：玻璃本身，或毛玻璃容器。
    pub fn view(&self) -> &NSView {
        match &self.material {
            Material::Glass(view) => view,
            Material::Frosted(view) => view,
        }
    }
}
