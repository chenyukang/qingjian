//! 菜单栏里的「中 / 英」状态项。
//!
//! 输入源图标（Info.plist 的 tsInputMethodIconFileKey）没法动态换，所以自己放一个 NSStatusItem。
//! Caps Lock 的变化不会作为按键送到输入法，用一个定时器轮询系统状态刷新。
//!
//! 状态项一旦创建，可见性分两种处理：
//! - **配置关掉**（`[status_bar] menubar_item = false`）：`setVisible(false)` 真的从菜单栏拿掉。只收成零宽不行 ——
//!   这版系统上零宽状态项会留一个小黑块（2026-10-07 用户报的：「配置的是隐藏就应该真的不显示」）。
//!   代价是再打开时会被系统排到最左边（固定 autosave 名也保不住），但只在用户改开关时发生。
//! - **输入法停用**：收成零宽 + 藏起按钮（**位置要保住**）。焦点每进出一次输入框 IMK 就 deactivate / activate 一轮，
//!   所以延迟 [`COLLAPSE_DELAY`] 再收：焦点只是在输入框之间挪的话，半秒内就会再次激活，根本收不下去；
//!   真换到别的输入法才收起来，切回来再展开，位置一直在。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSMenu, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString, NSTimer, ns_string};

use crate::imk::modifiers;

/// 轮询间隔。Shift 的「轻拍」要靠它抓（IMK 不把修饰键变化送给输入法，只能轮询物理状态）：
/// 手指轻拍大概 80–150 毫秒，40 毫秒一轮能稳稳抓到按下与松开两个边沿。
const POLL_INTERVAL: f64 = 0.04;

/// 菜单栏标题与桌面指示器多久同步一次（`POLL_INTERVAL` 的多少轮）—— 它们的变化没那么急。
const SLOW_EVERY: u32 = 6;

/// 停用后隔多久才把状态项收起：焦点在输入框之间挪动时 deactivate 与下一次 activate 只隔几十毫秒。
const COLLAPSE_DELAY: f64 = 0.5;

pub struct ModeIndicator {
    /// 菜单栏状态项。
    item: Retained<NSStatusItem>,

    /// 轮询定时器；未激活时为 `None`。
    timer: Option<Retained<NSTimer>>,

    /// 停用后延迟收起的一次性定时器；再次激活时取消。
    collapse_timer: Option<Retained<NSTimer>>,

    /// 正展开着（输入法激活中）。收起时不刷新标题。
    shown: bool,

    /// 上次显示的状态（是否英文、是否查询模式），避免每次轮询都重设标题。
    state: Option<(bool, bool, bool)>,

    /// `[status_bar] menubar_item`：关掉就整个收成零宽（位置保留），也不再展开。
    enabled: bool,

    /// 云联想开着：标题带云朵，让用户一眼知道上下文会发出去。
    cloud: bool,

    mtm: MainThreadMarker,
}

impl ModeIndicator {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(0.0);
        item.setAutosaveName(Some(ns_string!("QingjianModeIndicator")));
        item.setVisible(true);
        Self {
            item,
            timer: None,
            collapse_timer: None,
            shown: false,
            state: None,
            enabled: true,
            cloud: false,
            mtm,
        }
    }

    /// `[status_bar] menubar_item`：关掉就**真的从菜单栏拿掉**（`setVisible(false)`），打开则恢复。
    ///
    /// 只收成零宽不够 —— 这版系统上会留一个小黑块（见本文件开头的说明）。拿掉的代价是再打开时位置可能被
    /// 系统排到最左边，这只在用户主动改开关时发生；焦点来回挪动那种「停用收起」仍走零宽那条路（保位置）。
    pub fn set_enabled(&mut self, on: bool) {
        if self.enabled == on {
            return;
        }
        self.enabled = on;
        self.state = None;
        if on {
            self.item.setVisible(true);
            self.activate();
        } else {
            // 不等那半秒的延迟收起，立刻收，并整个拿掉
            self.collapse();
            self.item.setVisible(false);
        }
    }

    /// 输入法激活：展开状态项并开始轮询；停用时安排的收起取消。
    pub fn activate(&mut self) {
        if !self.enabled {
            self.collapse();
            return;
        }
        if let Some(timer) = self.collapse_timer.take() {
            timer.invalidate();
        }
        if !self.shown {
            self.shown = true;
            if let Some(button) = self.item.button(self.mtm) {
                button.setHidden(false);
            }
            self.item.setLength(NSVariableStatusItemLength);
        }
        self.state = None;
        self.update();
        if self.timer.is_none() {
            let target = ModeMonitor::new(self.mtm);
            let timer = unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    POLL_INTERVAL,
                    &target,
                    sel!(tick:),
                    None,
                    true,
                )
            };
            self.timer = Some(timer);
        }
    }

    /// 输入法停用：半秒后没再激活就收起。
    ///
    /// 轮询**不停**：它每轮顺带同步一次桌面指示器（输入源通知万一漏了，这里 0.25 秒内兜住）。
    pub fn deactivate(&mut self) {
        if self.collapse_timer.is_some() {
            return;
        }
        let target = ModeMonitor::new(self.mtm);
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                COLLAPSE_DELAY,
                &target,
                sel!(collapse:),
                None,
                false,
            )
        };
        self.collapse_timer = Some(timer);
    }

    /// 收成零宽、清空标题并藏起按钮；**位置保留**。
    ///
    /// 位置是要保的（焦点在输入框之间挪动不该让图标跳位置），所以这里不用 `setVisible(false)` ——
    /// 那会把它排到菜单栏最左边。真要「配置关掉」走 [`Self::set_enabled`]。
    pub fn collapse(&mut self) {
        self.collapse_timer = None;
        if !self.shown {
            return;
        }
        self.shown = false;
        self.state = None;
        if let Some(button) = self.item.button(self.mtm) {
            button.setTitle(ns_string!(""));
            button.setHidden(true);
        }
        self.item.setLength(0.0);
    }

    /// 点状态项弹出的菜单。
    pub fn set_menu(&self, menu: &NSMenu) {
        self.item.setMenu(Some(menu));
    }

    pub fn set_cloud(&mut self, cloud: bool) {
        self.cloud = cloud;
        self.state = None;
    }

    /// 按当前 Caps Lock 状态刷新标题；收起时不动。
    pub fn update(&mut self) {
        if !self.shown {
            return;
        }
        let english = modifiers::caps_lock_on()
            || crate::host::with(|h| h.english_mode_manual).unwrap_or(false);
        // 查询模式在这一栏常显：它是「模式之外」的状态，光看候选窗口看不出来
        let lookup = crate::host::with(|h| h.engine.lookup_mode()).unwrap_or(false);
        // 逐字模式同理由这一栏常显：候选变短了，但「为什么变短」得有个地方写着
        let word_by_word = crate::host::with(|h| h.engine.word_by_word()).unwrap_or(false);
        if self.state == Some((english, lookup, word_by_word)) {
            return;
        }
        self.state = Some((english, lookup, word_by_word));
        if let Some(button) = self.item.button(self.mtm) {
            let mode = if english { "英" } else { "中" };
            let lookup_mark = if lookup { " 查" } else { "" };
            let word_by_word_mark = if word_by_word { " 逐" } else { "" };
            let cloud_mark = if self.cloud { " ☁︎" } else { "" };
            let title = format!("{mode}{lookup_mark}{word_by_word_mark}{cloud_mark}");
            button.setTitle(&NSString::from_str(&title));
        }
    }
}

/// 菜单栏轮询器的状态：上一轮 Shift 是否按下（抓边沿）、慢同步的轮次。
struct MonitorState {
    shift: std::cell::Cell<bool>,
    round: std::cell::Cell<u32>,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            shift: std::cell::Cell::new(false),
            round: std::cell::Cell::new(0),
        }
    }
}

define_class!(
    // SAFETY: NSObject 没有子类化要求；没有实现 Drop。
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MonitorState]
    struct ModeMonitor;

    impl ModeMonitor {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: Option<&AnyObject>) {
            // 轻拍 Shift：按下 → 记时刻；松开 → 窗口内没敲过别的键就切换
            let shift = modifiers::shift_down();
            let was = self.ivars().shift.get();
            self.ivars().shift.set(shift);
            if shift && !was {
                crate::host::with(|h| {
                    h.shift_tap_armed = h.shift_tap_toggle.then(std::time::Instant::now);
                });
            } else if !shift && was {
                // 窗口长度是配置项（`[general] shift_tap_window_ms`，缺省 300）
                let tapped = crate::host::with(|h| {
                    h.shift_tap_armed
                        .take()
                        .is_some_and(|at| at.elapsed() <= h.shift_tap_window)
                })
                .unwrap_or(false);
                if tapped {
                    crate::host::with(|h| h.tap_toggle_english());
                }
            }
            // 标题与指示器慢一拍同步就够
            let round = self.ivars().round.get().wrapping_add(1);
            self.ivars().round.set(round);
            if !round.is_multiple_of(SLOW_EVERY) {
                return;
            }
            let english = modifiers::caps_lock_on();
            crate::host::with(|h| {
                h.indicator.update();
                h.sync_indicator_dot(english);
            });
        }

        #[unsafe(method(collapse:))]
        fn collapse(&self, _timer: Option<&AnyObject>) {
            crate::host::with(|h| h.indicator.collapse());
        }
    }

    unsafe impl NSObjectProtocol for ModeMonitor {}
);

impl ModeMonitor {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(MonitorState::new());
        unsafe { msg_send![super(this), init] }
    }
}
