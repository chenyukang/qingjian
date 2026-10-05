//! 设置窗口左侧栏的导航按钮：自绘的圆角高亮 + 彩色圆角方块 + 白色符号，右边跟标题。
//!
//! 形状照 winlane 的 `SettingsNavigationButton`：选中（或点下去）时画一层 9% 的浅色圆角底，
//! 图标是 26×26 的彩色圆角方块里嵌一个白色 SF Symbol，标题用左边距让出图标的位置。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBezierPath, NSButton, NSButtonType, NSColor, NSCompositingOperation,
    NSControlStateValueOn, NSFont, NSImage, NSImageSymbolConfiguration, NSTextAlignment,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

/// 图标的颜色与符号。符号取不到（老系统 / 名字写错）时只画色块。
pub(super) struct Style {
    color: Retained<NSColor>,
    symbol: Option<Retained<NSImage>>,
}

define_class!(
    // SAFETY: NSButton 允许子类化；只在主线程画。
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Style]
    pub(super) struct NavigationButton;

    unsafe impl NSObjectProtocol for NavigationButton {}

    impl NavigationButton {
        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            let bounds = self.bounds();
            if self.state() == NSControlStateValueOn || self.isHighlighted() {
                NSColor::labelColor()
                    .colorWithAlphaComponent(0.09)
                    .setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, 9.0, 9.0).fill();
            }
            // SAFETY: 交给 NSButton 画标题与键盘焦点提示。
            unsafe {
                let _: () = msg_send![super(self), drawRect: dirty];
            }
            self.ivars().color.setFill();
            let icon = NSRect::new(
                NSPoint::new(10.0, (bounds.size.height - 26.0) / 2.0),
                NSSize::new(26.0, 26.0),
            );
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(icon, 6.0, 6.0).fill();
            if let Some(symbol) = &self.ivars().symbol {
                let glyph = NSRect::new(
                    NSPoint::new(icon.origin.x + 4.0, icon.origin.y + 4.0),
                    NSSize::new(18.0, 18.0),
                );
                // SAFETY: 没有 draw hints；坐标随按钮的翻转状态。
                unsafe {
                    symbol.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                        glyph,
                        NSRect::ZERO,
                        NSCompositingOperation::SourceOver,
                        1.0,
                        true,
                        None,
                    );
                }
            }
        }
    }
);

impl NavigationButton {
    /// `tag` 用调用方设（`Setting::SelectPage` 的编号），动作统一是 `changed:`。
    pub(super) fn new(
        title: &str,
        symbol_name: &str,
        color: Retained<NSColor>,
        frame: NSRect,
        target: &AnyObject,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol_name),
            None,
        )
        .and_then(|image| {
            image.imageWithSymbolConfiguration(
                &NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                    &NSColor::whiteColor(),
                ),
            )
        });
        let this = Self::alloc(mtm).set_ivars(Style { color, symbol });
        // SAFETY: 初始化一次，绘制状态都归主线程所有。
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // 标题前面留出图标的宽度（图标是自己画的，不走 NSButton 的 image 通道）
        this.setTitle(&NSString::from_str(&format!("           {title}")));
        this.setFont(Some(&NSFont::systemFontOfSize(14.0)));
        this.setAlignment(NSTextAlignment::Left);
        this.setBordered(false);
        this.setButtonType(NSButtonType::MomentaryChange);
        this.setAccessibilityLabel(Some(&NSString::from_str(title)));
        // SAFETY: target 是设置窗口的目标对象，活在主线程；动作走 tag 分发。
        unsafe {
            this.setTarget(Some(target));
            this.setAction(Some(sel!(changed:)));
        }
        this
    }
}
