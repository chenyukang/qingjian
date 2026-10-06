//! 面板「材质」：macOS 26 起用系统 Liquid Glass（`NSGlassEffectView`），更早的系统退回
//! `NSVisualEffectView` 毛玻璃。候选窗与偏好设置共用这一份。
//!
//! 参考 winlane 的 `ui/material.rs`（那边还多一档"纯色渐变"，这里先不需要）。
//! 用法：建一个 backdrop，把 [`PanelBackdrop::view`] 作为**容器最底层的 subview**，
//! 自己的 UI 再加在它后面（subview 顺序就是层级，别用 `NSGlassEffectView.setContentView`
//! —— 这版系统上材质会压在内容上方，候选文字会被盖住）。

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
};
use objc2_foundation::{NSPoint, NSRect};

/// 系统有没有 Liquid Glass API（macOS 26 起）。老系统上不能用 `NSGlassEffectView` 这个名字。
pub fn glass_available() -> bool {
    AnyClass::get(c"NSGlassEffectView").is_some()
}

/// 把视图裁成圆角：`wantsLayer` + `CALayer.cornerRadius` + `masksToBounds`。
///
/// 不引 `objc2-quartz-core` 的 feature，直接 `msg_send` 设——这两个属性名很稳定。
pub fn round_corners(view: &NSView, corner_radius: f64) {
    if corner_radius <= 0.0 {
        return;
    }
    view.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        if layer.is_null() {
            return;
        }
        let _: () = msg_send![layer, setCornerRadius: corner_radius];
        let _: () = msg_send![layer, setMasksToBounds: true];
    }
}

/// 面板圆角缺省值（偏好设置用；候选窗用主题里的 `corner_radius`）。
pub const DEFAULT_CORNER_RADIUS: f64 = 18.0;

enum Material {
    Glass(Retained<NSGlassEffectView>),

    /// 毛玻璃容器：`NSVisualEffectView` 是它的 subview，圆角裁在容器这层（见 [`round_corners`] 的说明）。
    Frosted(Retained<NSView>),
}

/// 一块面板底色：拿到 [`Self::view`]，放进容器最底层即可。
pub struct PanelBackdrop {
    material: Material,
}

/// 毛玻璃 + 一层包它的容器（圆角裁在容器上，返回的是容器）。
fn frosted_container(frame: NSRect, corner_radius: f64, mtm: MainThreadMarker) -> Retained<NSView> {
    let resize =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;
    let container = NSView::initWithFrame(NSView::alloc(mtm), frame);
    container.setAutoresizingMask(resize);
    let blur = NSVisualEffectView::initWithFrame(
        NSVisualEffectView::alloc(mtm),
        NSRect::new(NSPoint::ZERO, frame.size),
    );
    blur.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
    blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    blur.setState(NSVisualEffectState::Active);
    blur.setAutoresizingMask(resize);
    container.addSubview(&blur);
    // 圆角裁在容器这一层，**别裁材质自己**：给 `NSVisualEffectView` 设 `layer.masksToBounds`
    // 会让材质退回一块平色（毛玻璃整个没了，2026-10-06 踩过）；裁父视图则照常。
    round_corners(&container, corner_radius);
    container
}

impl PanelBackdrop {
    /// 按 `frame` 建一块材质（圆角 `corner_radius`）。
    pub fn new(frame: NSRect, corner_radius: f64, mtm: MainThreadMarker) -> Self {
        let material = if glass_available() {
            // Liquid Glass：内容由 Glass 视图托管（它负责把 content 画在材质之上）
            let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
            glass.setStyle(NSGlassEffectViewStyle::Regular);
            glass.setCornerRadius(corner_radius);
            Material::Glass(glass)
        } else {
            // 退路：毛玻璃
            Material::Frosted(frosted_container(frame, corner_radius, mtm))
        };
        Self { material }
    }

    /// 强制用 `NSVisualEffectView` 毛玻璃（不走 Liquid Glass）。
    ///
    /// 候选窗就走这条。材质取 `.UnderWindowBackground` 而不是 `.Popover`：`.Popover` 是为气泡的
    /// **可读性**调的，本身就偏实体、几乎不透明，叠上候选自己的底色就是一块平光的浅色板，
    /// 毛玻璃看不出来；`.UnderWindowBackground` 糊得重、自己几乎不上色，背后的字会被糊成一片。
    pub fn frosted(frame: NSRect, corner_radius: f64, mtm: MainThreadMarker) -> Self {
        Self {
            material: Material::Frosted(frosted_container(frame, corner_radius, mtm)),
        }
    }

    /// 挂到窗口上的视图：玻璃本身，或毛玻璃容器。
    pub fn view(&self) -> &NSView {
        match &self.material {
            Material::Glass(view) => view,
            Material::Frosted(view) => view,
        }
    }
}
