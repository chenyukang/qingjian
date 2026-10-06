use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSEvent, NSWindow,
    NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSObjectProtocol, NSRect};

/// Esc 的键码。
const ESCAPE_KEY: u16 = 53;

define_class!(
    // SAFETY: NSWindow 允许子类化；没有实现 Drop。
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    /// 设置窗口的 NSWindow：关窗时把激活权还给上一个应用，让输入法回到不抢焦点的后台形态。
    pub struct PreferencesPanel;

    impl PreferencesPanel {
        /// Esc 关窗（`cancelOperation:`）：文本框与普通控件敲 Esc，由键绑定系统把它沿响应链
        /// 送上来；设置窗没有「取消」按钮，这条就是它的取消。
        /// 录制快捷键时 Esc 被录制控件自己吃掉（取消录制），到不了这里。
        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            self.dismiss();
        }

        /// 没有第一响应者（或响应者是窗口本身）时 Esc 不会走 `cancelOperation:`，按键直接到窗口，
        /// 这里再认一次；其余键照旧交给父类（Tab 换焦点、回车按默认按钮都靠它）。
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == ESCAPE_KEY {
                self.dismiss();
                return;
            }
            let _: () = unsafe { msg_send![super(self), keyDown: event] };
        }

        /// 关窗时把激活权还给上一个应用（用户的输入框）：`deactivate` 就够了。
        /// 激活策略留在 Accessory（`LSUIElement`，与另两家输入法一致），不再切回 `Prohibited`。
        #[unsafe(method(close))]
        fn close(&self) {
            let mtm = MainThreadMarker::from(self);
            NSApplication::sharedApplication(mtm).deactivate();
            let _: () = unsafe { msg_send![super(self), close] };
        }
    }

    unsafe impl NSObjectProtocol for PreferencesPanel {}
);

impl PreferencesPanel {
    /// 关窗：走 ObjC 分发，进的是上面那个 `close` 覆写（把激活权还给上一个应用在那里做）。
    /// `define_class!` 生成的方法带一个 `Sel` 参数，方法体里不能直接 `self.close()`。
    fn dismiss(&self) {
        let _: () = unsafe { msg_send![self, close] };
    }

    pub fn new(mtm: MainThreadMarker, content: NSRect) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(());
        let this: Retained<Self> = unsafe {
            msg_send![
                super(this),
                initWithContentRect: content,
                styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        // 面板材质要透到桌面，窗口就不能画背景；标题栏那一条因此也得由内容区自己铺满，
        // 否则顶部一条是裸桌面，关闭 / 最小化那排按钮看着像和窗口脱开。
        // `FullSizeContentView` + 透明标题栏 + 隐藏标题必须一起设：只设 `titlebarAppearsTransparent`
        // 会把那排按钮弄坏（上一版踩过）。winlane 的设置窗是同一套。
        this.setStyleMask(this.styleMask() | NSWindowStyleMask::FullSizeContentView);
        this.setTitlebarAppearsTransparent(true);
        this.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        // 程序建的 NSWindow 默认关窗即释放，我们还握着 Retained，必须关掉
        unsafe { this.setReleasedWhenClosed(false) };
        this
    }

    /// 切到 Accessory（有窗口、无 Dock 图标）并把窗口带到最前，文本框才拿得到键盘焦点。
    pub fn present(&self) {
        let mtm = MainThreadMarker::from(self);
        let app = NSApplication::sharedApplication(mtm);
        super::edit_menu::install(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        self.makeKeyAndOrderFront(None);
    }
}
