//! 偏好设置窗口本体：把各页（`pages/`）装进标签视图，底部一行状态；刷新时逐页同步。

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBox, NSBoxType, NSClipView, NSColor, NSFont, NSScreen, NSScrollView, NSTabView,
    NSTabViewItem, NSTabViewType, NSTextField, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use qingjian_core::{Language, UsageSummary, VocabularySummary};
use qingjian_platform::Config;

use super::controls::{language_label, small_label};
use super::layout::{Layout, PAGE_WIDTH};
use super::navigation::NavigationButton;
use super::pages::{
    AboutPage, AdvancedPage, CandidatesPage, CloudPage, DictionariesPage, FuzzyPage, GeneralPage,
    IndicatorPage, PhrasesPage, ShortcutsPage, SortPreferencesPage, UpdateStatus, UsagePage,
    build_about,
};
use super::panel::PreferencesPanel;
use super::setting::SELECT_PAGE_TAG_BASE;
use super::target::PreferencesTarget;
use crate::host::DictionaryInfo;
use objc2_foundation::NSInteger;

/// 每页顶部留白、页面最低高度（矮页也撑到这个高度，切页时窗口不跳）。
const PAGE_TOP: f64 = 18.0;
const MIN_PAGE_HEIGHT: f64 = 250.0;

/// 左侧栏宽度（与 winlane 的设置窗一致）。
const SIDEBAR_WIDTH: f64 = 220.0;
/// 右侧内容区左边距（分隔线之后）。
const CONTENT_MARGIN: f64 = 28.0;
/// 右侧顶部「大标题 + 说明」占的高度。
const HEADER_HEIGHT: f64 = 104.0;
/// 侧栏条目高（含条目间空隙）。
const NAV_ITEM_HEIGHT: f64 = 38.0;

/// 页面清单：**(标题, 一句话说明)**，顺序必须与上面 push 的顺序一致。
const PAGE_INFO: [(&str, &str); 12] = [
    ("通用", "中英切换、候选与显示这些日常开关。"),
    ("候选窗口", "候选窗字体、字号、行数、宽度与外观。"),
    ("快捷键", "上屏、翻页、删词、隐藏候选、翻译与纠错这些按键。"),
    ("自定义短语", "编码 → 短语，随打随换。"),
    ("模糊音", "前后鼻音、平翘舌这类容易混的音。"),
    ("词库", "内置词库、导入的词库与个人词表。"),
    ("指示器", "桌面悬浮的「中 / 英」圆点与菜单栏状态项。"),
    ("屏蔽词", "被后置 / 隐藏的候选，在这里逐条恢复。"),
    ("云服务", "云端补全、翻译与纠错的接口与模型。"),
    ("高级", "数据目录、日志这些不常碰的东西。"),
    ("统计", "用过的词、学习次数与输入量。"),
    ("关于", "版本、许可与项目信息。"),
];

/// 侧栏分组：`(从第几页开始, 分组标题)`。
const NAV_GROUPS: [(usize, &str); 3] = [(0, "常用"), (3, "词库"), (6, "其他")];

/// 侧栏条目的图标：SF Symbol 名 + 返回图标块颜色的构造函数。
type NavIcon = (&'static str, fn() -> Retained<NSColor>);

/// 侧栏每个条目的（SF Symbol, 图标块颜色）。
const NAV_ICONS: [NavIcon; 12] = [
    ("gearshape.fill", NSColor::systemGrayColor),
    ("macwindow", NSColor::systemPinkColor),
    ("keyboard", NSColor::systemPurpleColor),
    ("text.badge.plus", NSColor::systemTealColor),
    ("textformat.abc", NSColor::systemGreenColor),
    ("books.vertical.fill", NSColor::systemBlueColor),
    ("circle.fill", NSColor::systemCyanColor),
    ("eye.slash.fill", NSColor::systemOrangeColor),
    ("cloud.fill", NSColor::systemIndigoColor),
    ("slider.horizontal.3", NSColor::systemBrownColor),
    ("chart.bar.fill", NSColor::systemYellowColor),
    ("info.circle.fill", NSColor::systemRedColor),
];

/// 标签视图四周留白、底部状态行高度。
const TAB_MARGIN: f64 = 14.0;
const STATUS_HEIGHT: f64 = 18.0;

/// 窗口比屏幕可用高度至少矮这么多（标题栏 + 上下留一点边）；页面比窗口高时自己滚。
const SCREEN_MARGIN: f64 = 80.0;

/// 设置窗口与需要按配置刷新的各页。
pub struct PreferencesWindow {
    /// 窗口。
    panel: Retained<PreferencesPanel>,

    /// 右侧内容区的标签视图（无边框，页签由左侧栏代替）。
    tabs: Retained<NSTabView>,

    /// 左侧栏条目，按页面顺序。
    navigation: Vec<Retained<NavigationButton>>,

    /// 右侧的大标题与说明。
    page_title: Retained<NSTextField>,
    page_description: Retained<NSTextField>,

    /// 「通用」页。
    general: GeneralPage,

    /// 「候选窗口」页。
    candidates: CandidatesPage,

    /// 「快捷键」页。
    shortcuts: ShortcutsPage,

    /// 自定义短语编辑。
    phrases: PhrasesPage,

    /// 「模糊音」页。
    fuzzy: FuzzyPage,

    /// 「词库」页。
    dictionaries: DictionariesPage,
    sort_preferences: SortPreferencesPage,
    indicator: IndicatorPage,

    /// 「云服务」页。
    cloud: CloudPage,

    /// 「高级」页。
    advanced: AdvancedPage,

    /// 「统计」页的数字。
    usage: UsagePage,

    /// 「关于」页的检查更新控件。
    about: AboutPage,

    /// 底部状态行：配置文件解析失败时显示原因，也给临时提示用。
    status: Retained<NSTextField>,

    /// 所有控件的 target，要和窗口活得一样久。
    _target: Retained<PreferencesTarget>,
}

/// 一页：标题、布局器、承载视图。
type Page = (&'static str, Layout, Retained<NSView>);

impl PreferencesWindow {
    /// `languages` 是打进包里的释义表语言，`version` / `build` 显示在「关于」页。
    pub fn new(mtm: MainThreadMarker, languages: &[Language], version: &str, build: &str) -> Self {
        let target = PreferencesTarget::new(mtm);
        let new_layout = || Layout::new(PAGE_WIDTH, PAGE_TOP);
        let page = |title: &'static str, layout: Layout| -> Page {
            (
                title,
                layout,
                NSView::initWithFrame(mtm.alloc(), NSRect::ZERO),
            )
        };
        let mut pages: Vec<Page> = Vec::new();

        let mut layout = new_layout();
        let general = GeneralPage::build(&mut layout, mtm, &target, languages);
        pages.push(page("通用", layout));

        let mut layout = new_layout();
        let candidates = CandidatesPage::build(&mut layout, mtm, &target);
        pages.push(page("候选窗口", layout));

        let mut layout = new_layout();
        let shortcuts = ShortcutsPage::build(&mut layout, mtm, &target);
        pages.push(page("快捷键", layout));

        let mut layout = new_layout();
        let phrases = PhrasesPage::build(&mut layout, mtm, &target);
        pages.push(page("自定义短语", layout));

        let mut layout = new_layout();
        let fuzzy = FuzzyPage::build(&mut layout, mtm, &target);
        pages.push(page("模糊音", layout));

        let mut layout = new_layout();
        let dictionaries = DictionariesPage::build(&mut layout, mtm, &target);
        pages.push(page("词库", layout));

        let mut layout = new_layout();
        let indicator = IndicatorPage::build(&mut layout, mtm, &target);
        pages.push(page("指示器", layout));

        let mut layout = new_layout();
        let sort_preferences = SortPreferencesPage::build(&mut layout, mtm, &target);
        pages.push(page("屏蔽词", layout));

        let mut layout = new_layout();
        let cloud = CloudPage::build(&mut layout, mtm, &target);
        pages.push(page("云服务", layout));

        let mut layout = new_layout();
        let advanced = AdvancedPage::build(&mut layout, mtm, &target);
        pages.push(page("高级", layout));

        let mut layout = new_layout();
        let usage = UsagePage::build(&mut layout, mtm);
        pages.push(page("统计", layout));

        let mut layout = new_layout();
        let about = build_about(&mut layout, mtm, &target, version, build);
        pages.push(page("关于", layout));

        // 几何：左 220 侧栏 + 一条分隔线 + 右侧（大标题 / 说明 / 页面 / 状态行）
        let tallest = pages
            .iter()
            .map(|(_, layout, _)| layout.height() + PAGE_TOP)
            .fold(MIN_PAGE_HEIGHT, f64::max);
        // 设置项多了以后最高的一页会超出小屏幕：窗口封顶，超高的页放进滚动视图
        let page_height = tallest.min(max_page_height(mtm)).max(MIN_PAGE_HEIGHT);
        let text_x = SIDEBAR_WIDTH + 1.0 + CONTENT_MARGIN;
        let content_size = NSSize::new(
            text_x + PAGE_WIDTH + CONTENT_MARGIN,
            page_height + HEADER_HEIGHT + STATUS_HEIGHT + 16.0,
        );
        let content = NSView::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::ZERO, content_size));
        let backdrop = crate::ui::material::PanelBackdrop::new(
            NSRect::new(NSPoint::ZERO, content_size),
            crate::ui::material::DEFAULT_CORNER_RADIUS,
            mtm,
        );
        content.addSubview(backdrop.view());

        // 左侧栏：系统侧栏材质 + 品牌 + 分组条目（照 winlane 的设置窗）
        let sidebar = NSVisualEffectView::initWithFrame(
            NSVisualEffectView::alloc(mtm),
            NSRect::new(
                NSPoint::ZERO,
                NSSize::new(SIDEBAR_WIDTH, content_size.height),
            ),
        );
        sidebar.setMaterial(NSVisualEffectMaterial::Sidebar);
        sidebar.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
        crate::ui::material::round_corners(&sidebar, crate::ui::material::DEFAULT_CORNER_RADIUS);
        content.addSubview(&sidebar);

        let brand = small_label(mtm, "青简");
        brand.setFont(Some(&NSFont::boldSystemFontOfSize(21.0)));
        brand.setFrame(NSRect::new(
            NSPoint::new(24.0, content_size.height - 58.0),
            NSSize::new(170.0, 30.0),
        ));
        sidebar.addSubview(&brand);
        let version_label = small_label(mtm, &format!("{version}（{build}）"));
        version_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        version_label.setFrame(NSRect::new(
            NSPoint::new(25.0, content_size.height - 80.0),
            NSSize::new(180.0, 20.0),
        ));
        sidebar.addSubview(&version_label);

        let divider = NSBox::initWithFrame(
            NSBox::alloc(mtm),
            NSRect::new(
                NSPoint::new(SIDEBAR_WIDTH, 0.0),
                NSSize::new(1.0, content_size.height),
            ),
        );
        divider.setBoxType(NSBoxType::Separator);
        content.addSubview(&divider);

        let mut navigation: Vec<Retained<NavigationButton>> = Vec::new();
        let mut nav_y = content_size.height - 112.0;
        for (index, (title, _)) in PAGE_INFO.iter().enumerate() {
            if let Some((_, heading)) = NAV_GROUPS.iter().find(|(start, _)| *start == index) {
                let label = small_label(mtm, heading);
                label.setTextColor(Some(&NSColor::secondaryLabelColor()));
                label.setFrame(NSRect::new(
                    NSPoint::new(24.0, nav_y),
                    NSSize::new(170.0, 18.0),
                ));
                sidebar.addSubview(&label);
                nav_y -= 22.0;
            }
            let (symbol, color) = NAV_ICONS[index];
            let button = NavigationButton::new(
                title,
                symbol,
                color(),
                NSRect::new(
                    NSPoint::new(12.0, nav_y),
                    NSSize::new(SIDEBAR_WIDTH - 24.0, 32.0),
                ),
                &target,
                mtm,
            );
            button.setTag(SELECT_PAGE_TAG_BASE + index as NSInteger);
            sidebar.addSubview(&button);
            navigation.push(button);
            nav_y -= NAV_ITEM_HEIGHT;
        }

        // 右侧：大标题 + 一句话说明 + 无边框的页面区
        let page_title = small_label(mtm, PAGE_INFO[0].0);
        page_title.setFont(Some(&NSFont::boldSystemFontOfSize(25.0)));
        page_title.setFrame(NSRect::new(
            NSPoint::new(text_x, content_size.height - 62.0),
            NSSize::new(PAGE_WIDTH, 34.0),
        ));
        content.addSubview(&page_title);
        let page_description = small_label(mtm, PAGE_INFO[0].1);
        page_description.setTextColor(Some(&NSColor::secondaryLabelColor()));
        page_description.setFrame(NSRect::new(
            NSPoint::new(text_x, content_size.height - 96.0),
            NSSize::new(PAGE_WIDTH, 30.0),
        ));
        content.addSubview(&page_description);

        let tabs = NSTabView::initWithFrame(
            NSTabView::alloc(mtm),
            NSRect::new(
                NSPoint::new(text_x, STATUS_HEIGHT + 8.0),
                NSSize::new(PAGE_WIDTH, page_height),
            ),
        );
        tabs.setTabViewType(NSTabViewType::NoTabsNoBorder);
        tabs.setDrawsBackground(false);
        content.addSubview(&tabs);

        for (title, layout, view) in pages {
            let own_height = (layout.height() + PAGE_TOP).max(page_height);
            view.setFrame(NSRect::new(
                NSPoint::ZERO,
                NSSize::new(PAGE_WIDTH, own_height),
            ));
            layout.finish(&view, own_height);
            // SAFETY: identifier 允许为空；条目随 NSTabView 活着
            let item = unsafe { NSTabViewItem::initWithIdentifier(mtm.alloc(), None) };
            // 标签页自己的标签不再显示（左侧栏代替它），留着只为无障碍
            item.setLabel(&NSString::from_str(title));
            if own_height > page_height {
                item.setView(Some(&scrolling(mtm, &view, page_height, own_height)));
            } else {
                item.setView(Some(&view));
            }
            tabs.addTabViewItem(&item);
        }

        let status = small_label(mtm, "");
        status.setTextColor(Some(&NSColor::systemRedColor()));
        status.setFrame(NSRect::new(
            NSPoint::new(text_x, 6.0),
            NSSize::new(PAGE_WIDTH, STATUS_HEIGHT),
        ));
        content.addSubview(&status);

        let panel = PreferencesPanel::new(mtm, NSRect::new(NSPoint::ZERO, content_size));
        // 让材质能透到桌面：窗口自己不能画背景。**不动标题栏**（设了
        // titlebarAppearsTransparent + titleVisibility=Hidden 会把关闭/最小化那排按钮弄坏）
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setTitle(&NSString::from_str("青简偏好设置"));
        panel.setContentView(Some(&content));
        panel.center();

        let window = Self {
            panel,
            general,
            candidates,
            shortcuts,
            phrases,
            fuzzy,
            dictionaries,
            sort_preferences,
            indicator,
            cloud,
            advanced,
            usage,
            about,
            status,
            tabs,
            navigation,
            page_title,
            page_description,
            _target: target,
        };
        window.select_page(0);
        window
    }

    /// 切到第 `index` 页：内容区换页、右侧标题与说明跟着换、左侧栏高亮跟着挪。
    pub fn select_page(&self, index: usize) {
        if index >= self.navigation.len() {
            return;
        }
        self.tabs.selectTabViewItemAtIndex(index as NSInteger);
        if let Some((title, description)) = PAGE_INFO.get(index) {
            self.page_title.setStringValue(&NSString::from_str(title));
            self.page_description
                .setStringValue(&NSString::from_str(description));
        }
        for (position, button) in self.navigation.iter().enumerate() {
            button.setState(if position == index {
                objc2_app_kit::NSControlStateValueOn
            } else {
                objc2_app_kit::NSControlStateValueOff
            });
        }
    }

    pub fn select_phrase(&self, config: &Config, index: usize) {
        self.phrases.load(config, index);
    }
    pub fn selected_phrase(&self) -> Option<usize> {
        self.phrases.selected_row()
    }
    pub fn edit_phrase(&self, config: &Config, index: Option<usize>) {
        self.phrases.edit(config, index);
    }
    pub fn close_phrase_editor(&self) {
        self.phrases.close_editor();
    }
    pub fn set_phrase_error(&self, error: &str) {
        self.phrases.set_error(error);
    }
    pub fn phrase_draft(
        &self,
        config: &Config,
    ) -> Result<(Option<usize>, qingjian_core::CustomPhrase), String> {
        Ok((self.phrases.selected(config)?, self.phrases.draft()))
    }

    /// 打开（或带到最前）。
    pub fn show(&self) {
        self.panel.present();
    }

    /// 按配置刷新所有控件。`key_present` 是密钥已经有了（环境或配置里）；密钥框永远不回显值。
    pub fn sync(
        &self,
        config: &Config,
        key_present: bool,
        error: Option<&str>,
        dictionaries: &[DictionaryInfo],
        sort_preferences: &[(String, qingjian_core::SortPreference)],
        update: &UpdateStatus,
    ) {
        self.dictionaries.rebuild(dictionaries);
        self.sort_preferences.rebuild(sort_preferences);
        self.about.sync(config, update);
        self.general.sync(config);
        self.candidates.sync(config);
        self.indicator.sync(config);
        self.shortcuts.sync(config);
        self.phrases.sync(config);
        self.fuzzy.sync(config);
        self.cloud.sync(
            config,
            key_present,
            crate::app::paths::p2c_model_path().is_some()
                || crate::app::paths::model_path().is_some(),
        );
        self.advanced.sync(config);
        let status = error
            .map(|e| format!("配置文件有错误，已沿用上一份：{e}"))
            .unwrap_or_default();
        self.status.setTextColor(Some(&NSColor::systemRedColor()));
        self.status.setStringValue(&NSString::from_str(&status));
    }

    /// 检查更新的状态变了（查完了、查到新版），只刷「关于」页。
    pub fn sync_update(&self, config: &Config, update: &UpdateStatus) {
        self.about.sync(config, update);
    }

    /// 刷新「统计」页。打开窗口时调（数字随时在变，不跟配置一起同步）。
    pub fn sync_usage(
        &self,
        summary: &UsageSummary,
        vocabulary: &VocabularySummary,
        language: Option<Language>,
    ) {
        self.usage.show(
            summary,
            vocabulary,
            language.map_or("学习语言已关", language_label),
        );
    }

    /// 底部状态行临时显示一句提示（不是错误，灰字）；下次 `sync` 会被配置状态覆盖。
    pub fn set_status(&self, text: &str) {
        self.status
            .setTextColor(Some(&NSColor::secondaryLabelColor()));
        self.status.setStringValue(&NSString::from_str(text));
    }
}

/// 一页最高能多高：主屏可用高度减去标题栏、标签栏、状态行与留白。取不到屏幕就不封顶。
fn max_page_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::mainScreen(mtm).map_or(f64::MAX, |screen| {
        screen.visibleFrame().size.height - SCREEN_MARGIN - 2.0 * TAB_MARGIN - STATUS_HEIGHT
    })
}

/// 把比窗口高的一页放进滚动视图，开始时停在页顶。
fn scrolling(
    mtm: MainThreadMarker,
    page: &NSView,
    visible_height: f64,
    page_height: f64,
) -> Retained<NSScrollView> {
    let scroll = NSScrollView::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::ZERO, NSSize::new(PAGE_WIDTH, visible_height)),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setDocumentView(Some(page));
    // 页面视图没有翻转坐标，页顶在 y 最大处
    let clip: Retained<NSClipView> = scroll.contentView();
    clip.scrollToPoint(NSPoint::new(0.0, page_height - visible_height));
    scroll.reflectScrolledClipView(&clip);
    scroll
}
