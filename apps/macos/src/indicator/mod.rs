//! 桌面上的悬浮中 / 英指示器（`[status_bar]`）：一个不抢焦点、点击穿透的小圆点 / 方块。
//!
//! 有它就不必盯着菜单栏：中文一个颜色、英文另一个颜色。窗口参数照抄 winlane 那套
//! （`Borderless | NonactivatingPanel`、`ignoresMouseEvents`、`StatusWindowLevel + 1`、
//! `CanJoinAllSpaces | Stationary`、`orderFrontRegardless`）—— 少一个都可能抢焦点或掉到别的窗口后面。
//!
//! 输入法不是青简时**整个收起来**：不用去问系统当前输入源（青简自己就是输入法，
//! `deactivateServer` 就是"用户切走了"），比 Carbon TIS 那套可靠。
//!
//! 「设置界面」目前只做到配置文件（`[status_bar]` 一节，模板里有注释）；页面上那几个控件
//! （位置 / 形状 / 大小 / 两个颜色）等下一轮补。

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSPanel, NSScreen, NSStatusWindowLevel, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSDistributedNotificationCenter, NSObject, NSPoint, NSRect, NSSize};
use qingjian_platform::{Color, Shape, StatusBarConfig};

// 「选中的输入源变了」的观察者：切输入法时瞬时收到，立刻同步一次指示器。
//
// 为什么不只看 IMK 的 `deactivateServer`：切换输入法时那条回调**先**到，那一刻系统选中的
// 输入源还是青简，判不出"已经切走了"，之后就再没有回调 —— 圆点会一直挂着。
define_class!(
    // SAFETY: NSObject 没有子类化要求；没有实现 Drop。
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    struct SourceWatcher;

    impl SourceWatcher {
        #[unsafe(method(sourceChanged:))]
        fn source_changed(&self, _note: Option<&AnyObject>) {
            crate::host::with(|h| h.sync_indicator_dot(crate::imk::modifiers::caps_lock_on()));
        }
    }
);

/// 挂上「输入源变了」的观察者（进程生命周期内一直有效）。挂一次就够，重复调用无害。
pub fn watch_input_source_changes(mtm: MainThreadMarker) {
    let target = mtm.alloc::<SourceWatcher>().set_ivars(());
    let target: Retained<SourceWatcher> = unsafe { msg_send![super(target), init] };
    let name = crate::app::input_source::selection_changed_notification();
    unsafe {
        NSDistributedNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &target,
            sel!(sourceChanged:),
            Some(&name),
            None,
        );
    }
}

/// 画形状用的视图状态。ivars 本身拿不到 `&mut`（objc2 只在 alloc 时能塞），
/// 可变部分放 `RefCell` / `Cell` 里 —— 与候选窗视图那边一个写法。
struct DotIvars {
    /// 当前该画什么颜色。
    color: RefCell<Retained<NSColor>>,
    shape: Cell<Shape>,
    outline: Cell<bool>,
}

define_class!(
    // SAFETY: NSView 允许子类化；没有实现 Drop。
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DotIvars]
    struct DotView;

    impl DotView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let color = self.ivars().color.borrow();
            let shape = self.ivars().shape.get();
            let bounds = self.bounds();
            let path = match shape {
                Shape::Circle => NSBezierPath::bezierPathWithOvalInRect(bounds),
                Shape::Square => NSBezierPath::bezierPathWithRect(bounds),
                Shape::Rounded => {
                    let radius = shape.corner_radius(bounds.size.width);
                    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, radius, radius)
                }
            };
            color.setFill();
            path.fill();
            // 描一圈同色更深 / 更浅的边：深色壁纸上浅色点、浅色窗口上深色点都还能看清
            if self.ivars().outline.get() {
                NSColor::whiteColor().setStroke();
                path.setLineWidth(1.0);
                path.stroke();
            }
        }
    }
);

impl DotView {
    fn new(mtm: MainThreadMarker, config: &StatusBarConfig, english: bool) -> Retained<Self> {
        let size = config.size();
        let shape = config.shape;
        let color = native_color(if english {
            config.english_color
        } else {
            config.chinese_color
        });
        let this = mtm.alloc::<Self>().set_ivars(DotIvars {
            color: RefCell::new(color),
            shape: Cell::new(shape),
            outline: Cell::new(config.outline),
        });
        let view: Retained<Self> = unsafe {
            msg_send![
                super(this),
                initWithFrame: NSRect::new(NSPoint::ZERO, NSSize::new(size, size))
            ]
        };
        view.setWantsLayer(true);
        view
    }

    fn update(&self, config: &StatusBarConfig, english: bool) {
        let size = config.size();
        let ivars = self.ivars();
        *ivars.color.borrow_mut() = native_color(if english {
            config.english_color
        } else {
            config.chinese_color
        });
        ivars.shape.set(config.shape);
        ivars.outline.set(config.outline);
        let frame = self.frame();
        if (frame.size.width - size).abs() > f64::EPSILON {
            self.setFrame(NSRect::new(frame.origin, NSSize::new(size, size)));
        }
        self.setNeedsDisplay(true);
    }
}

/// `qingjian_platform` 的 RGB 到 AppKit 的颜色。
fn native_color(color: Color) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(color.red) / 255.0,
        f64::from(color.green) / 255.0,
        f64::from(color.blue) / 255.0,
        1.0,
    )
}

/// 悬浮指示器：一个面板 + 里面那个形状。
pub struct Indicator {
    panel: Option<Retained<NSPanel>>,
    view: Option<Retained<DotView>>,

    /// 当前是否显示（输入法激活中且开关开着）。
    visible: bool,
    mtm: MainThreadMarker,
}

impl Indicator {
    pub fn new(mtm: MainThreadMarker) -> Self {
        Self {
            panel: None,
            view: None,
            visible: false,
            mtm,
        }
    }

    /// 按配置与当前模式同步：开关关了就收起来，开着就摆好位置、换好颜色、显示出来。
    /// `english` 是**生效**的英文模式（Caps Lock 或 `⌃⇧R` 切过）。
    pub fn sync(&mut self, config: &StatusBarConfig, english: bool) {
        if !config.enabled {
            self.hide_now();
            return;
        }
        let view = match self.view.take() {
            Some(view) => view,
            None => DotView::new(self.mtm, config, english),
        };
        view.update(config, english);
        if let Some(panel) = &self.panel {
            panel.setContentView(Some(&view));
        } else {
            let panel = self.make_panel(&view, config);
            self.panel = Some(panel);
        }
        self.view = Some(view);
        self.place(config);
        // 显示与否只看「系统当前输入源是不是青简」：是就露出来（含焦点在输入框之间挪动），
        // 不是就立刻收（切到别的输入法 / 别的输入源）
        if crate::app::input_source::current_source_is_ours() {
            self.show();
        } else {
            self.hide_now();
        }
    }

    /// 摆到配置里说的位置：拖过（`x` / `y`）就按记下的，否则按「贴哪一角 + 偏移」算。
    fn place(&self, config: &StatusBarConfig) {
        let (Some(panel), Some(screen)) = (&self.panel, NSScreen::mainScreen(self.mtm)) else {
            return;
        };
        let visible = screen.visibleFrame();
        let size = config.size();
        let (x, y) = match (config.x, config.y) {
            (Some(x), Some(y)) => (f64::from(x), f64::from(y)),
            _ => config.anchor.origin(
                (
                    visible.origin.x,
                    visible.origin.y,
                    visible.size.width,
                    visible.size.height,
                ),
                size,
                (f64::from(config.offset_x), f64::from(config.offset_y)),
            ),
        };
        panel.setFrame_display(
            NSRect::new(NSPoint::new(x, y), NSSize::new(size, size)),
            false,
        );
    }

    fn make_panel(&self, view: &DotView, config: &StatusBarConfig) -> Retained<NSPanel> {
        let size = config.size();
        let rect = NSRect::new(NSPoint::ZERO, NSSize::new(size, size));
        let panel: Retained<NSPanel> = unsafe {
            msg_send![
                NSPanel::alloc(self.mtm),
                initWithContentRect: rect,
                styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setOpaque(false);
        panel.setHasShadow(false);
        // 点击穿透：鼠标事件直接落到下面的应用，指示器不挡事
        panel.setIgnoresMouseEvents(true);
        panel.setHidesOnDeactivate(false);
        panel.setExcludedFromWindowsMenu(true);
        panel.setLevel(NSStatusWindowLevel + 1);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setContentView(Some(view));
        panel
    }

    /// 露出来。
    pub fn show(&mut self) {
        let Some(panel) = &self.panel else {
            return;
        };
        if self.visible {
            return;
        }
        self.visible = true;
        panel.orderFrontRegardless();
    }

    /// 收起来。
    pub fn hide_now(&mut self) {
        if !self.visible {
            return;
        }
        self.visible = false;
        if let Some(panel) = &self.panel {
            panel.orderOut(None);
        }
    }
}
