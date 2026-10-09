//! 「屏蔽词」页：候选窗里标过「后置 / 隐藏」的词，一行一个 + 「恢复」按钮。
//!
//! 单独一页而不是塞进「词库」页：一个是那一页的列表已经占满（`Layout` 一页只支持一个撑满页底的控件），
//! 一个是标过的词可能几十个，得有个能滚的地方。

use std::cell::RefCell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSScrollView, NSTextField, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use qingjian_core::SortPreference;

use crate::preferences::controls::{GROUP_GAP, NOTE_HEIGHT, button, note_full, small_label};
use crate::preferences::layout::{Layout, PAGE_PADDING, PAGE_WIDTH, ROW_HEIGHT};
use crate::preferences::setting::Setting;
use crate::preferences::target::PreferencesTarget;

/// 一行的高度。
const ROW: f64 = ROW_HEIGHT + 6.0;

/// 列表区至少这么高（约 8 行），窗口更高时撑到页底。
const LIST_HEIGHT: f64 = 8.0 * ROW;

pub struct SortPreferencesPage {
    /// 列表的文档视图，行都加在它上面。
    list: Retained<NSView>,

    /// 装列表的滚动视图。
    scroll: Retained<NSScrollView>,

    /// 当前的行控件，重建时先移除。
    rows: RefCell<Vec<Retained<NSView>>>,

    /// 一个都没标过时的提示。
    empty: Retained<NSTextField>,

    /// 行控件的 target；建行时用。
    target: Retained<PreferencesTarget>,

    mtm: MainThreadMarker,
}

impl SortPreferencesPage {
    pub fn build(
        layout: &mut Layout,
        mtm: MainThreadMarker,
        target: &Retained<PreferencesTarget>,
    ) -> Self {
        note_full(
            layout,
            mtm,
            "候选窗里按 Shift+数字 把这一格「后置」（还看得见，只沉到最后）、按 ⌃+数字 把它「隐藏」（不再出现）。\
             词库里的词删不掉，能做的就这两档；标过的词在这里逐条列出，点「恢复」放回正常排序。",
        );
        let restore_all = button(mtm, "恢复全部", Setting::RestoreSortPreferences, target);
        layout.place(&restore_all, PAGE_PADDING, 120.0, ROW_HEIGHT + 4.0);
        layout.next_row(ROW_HEIGHT + 4.0);
        layout.space(GROUP_GAP);
        let list = NSView::initWithFrame(mtm.alloc(), NSRect::ZERO);
        let scroll = NSScrollView::initWithFrame(mtm.alloc(), NSRect::ZERO);
        scroll.setHasVerticalScroller(true);
        scroll.setDrawsBackground(false);
        scroll.setDocumentView(Some(&list));
        layout.place_fill(&scroll, PAGE_PADDING, layout.inner_width(), LIST_HEIGHT);
        layout.next_row(LIST_HEIGHT);
        let empty = small_label(mtm, "没有被后置或隐藏的词。");
        list.addSubview(&empty);
        empty.setFrame(NSRect::new(
            NSPoint::new(0.0, LIST_HEIGHT - NOTE_HEIGHT),
            NSSize::new(PAGE_WIDTH - 2.0 * PAGE_PADDING, NOTE_HEIGHT),
        ));
        Self {
            list,
            scroll,
            rows: RefCell::new(Vec::new()),
            empty,
            target: target.clone(),
            mtm,
        }
    }

    /// 重建列表：一行一个词（词 · 状态）+「恢复」按钮。
    pub fn rebuild(&self, words: &[(String, SortPreference)]) {
        let mtm = self.mtm;
        for view in self.rows.borrow_mut().drain(..) {
            view.removeFromSuperview();
        }
        self.empty.setHidden(!words.is_empty());
        let width = PAGE_WIDTH - 2.0 * PAGE_PADDING;
        let visible_height = self.scroll.contentSize().height.max(LIST_HEIGHT);
        let document_height = (ROW * words.len() as f64).max(visible_height);
        let content_width = self.scroll.contentSize().width.min(width);
        self.list.setFrame(NSRect::new(
            NSPoint::ZERO,
            NSSize::new(content_width, document_height),
        ));
        self.empty.setFrame(NSRect::new(
            NSPoint::new(0.0, document_height - NOTE_HEIGHT),
            NSSize::new(content_width, NOTE_HEIGHT),
        ));
        let mut rows = self.rows.borrow_mut();
        for (index, (word, preference)) in words.iter().enumerate() {
            let y = document_height - ROW * (index as f64 + 1.0);
            let state = match preference {
                SortPreference::Top => "置顶",
                SortPreference::Down => "后置",
                SortPreference::Hidden => "隐藏",
                SortPreference::Normal => "正常",
            };
            let label = small_label(mtm, &format!("{word} · {state}"));
            label.setFrame(NSRect::new(
                NSPoint::new(0.0, y + 2.0),
                NSSize::new(content_width - 72.0, ROW - 4.0),
            ));
            self.list.addSubview(&label);
            rows.push(Retained::into_super(Retained::into_super(label)));
            let restore = button(
                mtm,
                "恢复",
                Setting::RestoreSortPreference(index),
                &self.target,
            );
            restore.setFrame(NSRect::new(
                NSPoint::new(content_width - 72.0, y + 1.0),
                NSSize::new(72.0, ROW - 2.0),
            ));
            self.list.addSubview(&restore);
            rows.push(Retained::into_super(Retained::into_super(restore)));
        }
        // 非翻转坐标系的文档视图默认停在底部，滚回顶部让第一条在最上面
        let clip = self.scroll.contentView();
        clip.scrollToPoint(NSPoint::new(
            0.0,
            document_height - clip.bounds().size.height,
        ));
        self.scroll.reflectScrolledClipView(&clip);
    }
}
