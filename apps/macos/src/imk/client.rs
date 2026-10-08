//! 对 IMK 客户端对象（实现 `IMKTextInput` 协议的代理）的薄封装。
//!
//! objc2-input-method-kit 没有为 IMKTextInput 生成绑定，这里用 `msg_send!` 直接发消息。

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSAttributedString, NSDictionary, NSNotFound, NSRange, NSRect, NSString};
use qingjian_core::SurroundingText;

/// `{NSNotFound, 0}`：不替换任何已有文本，插到当前位置。
const NO_REPLACEMENT: NSRange = NSRange::new(NSNotFound as usize, 0);

#[derive(Clone, Copy)]
pub struct TextClient<'a> {
    /// IMK 传进来的 `sender`。
    object: &'a AnyObject,
}

/// 读选区失败的原因。分开报：原来三种情况共用一句「没有选中的文字，或这个应用不支持读取选区」，
/// 选区超长的用户会以为是应用不支持，白折腾一圈。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionFailure {
    /// 没有选区，或只选了空白。
    Empty,

    /// 选区长度（UTF-16 单位，与 `NSRange.length` 一致）超过上限。
    TooLong(usize),

    /// 应用不给读选区（部分 Electron / 非标准控件）。
    Unreadable,
}

impl<'a> TextClient<'a> {
    pub fn new(object: &'a AnyObject) -> Self {
        Self { object }
    }

    /// 设置 marked text（带下划线的未上屏文本），光标放在第 `cursor` 个字符处。空串等于清除。
    pub fn set_marked_text(&self, text: &str, cursor: usize) {
        let string = NSString::from_str(text);
        let cursor = NSRange::new(cursor.min(text.chars().count()), 0);
        unsafe {
            let _: () = msg_send![
                self.object,
                setMarkedText: &*string,
                selectionRange: cursor,
                replacementRange: NO_REPLACEMENT
            ];
        }
    }

    /// 上屏。
    pub fn insert_text(&self, text: &str) {
        let string = NSString::from_str(text);
        unsafe {
            let _: () =
                msg_send![self.object, insertText: &*string, replacementRange: NO_REPLACEMENT];
        }
    }

    /// 应用里当前选中的文字与它的范围（翻译 / 纠错用）。
    ///
    /// 超过 `max_chars` 个单位直接返回 [`SelectionFailure::TooLong`]：读下来再丢更费内存，
    /// 调用方也要知道实际长度才能把话说清楚。
    pub fn selected_text(&self, max_chars: usize) -> Result<(String, NSRange), SelectionFailure> {
        let selected: NSRange = unsafe { msg_send![self.object, selectedRange] };
        if selected.location == NSNotFound as usize || selected.length == 0 {
            return Err(SelectionFailure::Empty);
        }
        if selected.length > max_chars {
            return Err(SelectionFailure::TooLong(selected.length));
        }
        let text: Option<Retained<NSAttributedString>> =
            unsafe { msg_send![self.object, attributedSubstringFromRange: selected] };
        let Some(text) = text else {
            return Err(SelectionFailure::Unreadable);
        };
        let text = text.string().to_string();
        if text.trim().is_empty() {
            return Err(SelectionFailure::Empty);
        }
        Ok((text, selected))
    }

    /// 用 `text` 替换应用里 `range` 那段文字（翻译结果替换选区）。
    /// 把应用里选中的 `range` 标成一段**合成文本**（marked text），内容先放原文 `text`。
    ///
    /// 翻译 / 纠错窗口一打开就调它，之后整段对话都在合成状态下进行：Chromium / Electron 在
    /// **有合成时会把按键先交给输入法**（实测：Obsidian 里打拼音时按回车不会多出空行，
    /// 而没有合成时按回车会被编辑器吃掉选区、再插一个换行）。原文先当合成内容，视觉上只是被划上
    /// 合成下划线；结果回来后在 [`Self::finish_review`] 里换成结果并提交。
    pub fn begin_review(&self, text: &str, range: NSRange) {
        let string = NSString::from_str(text);
        let end = NSRange::new(string.length(), 0);
        unsafe {
            let _: () = msg_send![
                self.object,
                setMarkedText: &*string,
                selectionRange: end,
                replacementRange: range
            ];
        }
    }

    /// 结束合成：把合成内容换成 `text` 并提交（接受用结果、放弃用原文）。
    pub fn finish_review(&self, text: &str) {
        let string = NSString::from_str(text);
        let end = NSRange::new(string.length(), 0);
        unsafe {
            let _: () = msg_send![
                self.object,
                setMarkedText: &*string,
                selectionRange: end,
                replacementRange: NO_REPLACEMENT
            ];
            let _: () = msg_send![
                self.object,
                insertText: &*string,
                replacementRange: NO_REPLACEMENT
            ];
        }
    }

    pub fn surrounding_text(&self, before: usize, after: usize) -> Option<SurroundingText> {
        let (length, selected, marked): (usize, NSRange, NSRange) = unsafe {
            (
                msg_send![self.object, length],
                msg_send![self.object, selectedRange],
                msg_send![self.object, markedRange],
            )
        };
        if selected.location == NSNotFound as usize || length == 0 {
            return None;
        }
        // 组句中光标在 marked text 里；上下文以 marked text 为界
        let (start, end) = if marked.location == NSNotFound as usize {
            (selected.location, selected.location + selected.length)
        } else {
            (marked.location, marked.location + marked.length)
        };
        let start = start.min(length);
        let end = end.min(length);
        let before_range = NSRange::new(
            start.saturating_sub(before),
            start - start.saturating_sub(before),
        );
        let after_range = NSRange::new(end, after.min(length - end));
        let read = |range: NSRange| -> Option<String> {
            if range.length == 0 {
                return Some(String::new());
            }
            let text: Option<Retained<NSAttributedString>> =
                unsafe { msg_send![self.object, attributedSubstringFromRange: range] };
            text.map(|t| t.string().to_string())
        };
        Some(SurroundingText {
            before: read(before_range)?,
            after: read(after_range)?,
        })
    }

    /// 正在输入的应用的 bundle identifier（`com.apple.Terminal`），按应用改行为用；应用没给返回 `None`。
    pub fn bundle_identifier(&self) -> Option<String> {
        let bundle: Option<Retained<NSString>> =
            unsafe { msg_send![self.object, bundleIdentifier] };
        bundle.map(|b| b.to_string()).filter(|b| !b.is_empty())
    }

    /// **插入点（光标）**所在行在屏幕坐标系里的矩形 —— 提示类弹框贴光标用。
    ///
    /// 与 [`Self::caret_rect`] 的区别：那个问的是**第 0 个字符**，很多应用（Electron / 浏览器）
    /// 对第 0 个字符回的是文档开头，弹框会跳到屏幕角落（实测反馈）。
    /// 这里先问应用当前的选中范围（`selectedRange`），再拿插入点问行高矩形 —— 那才是光标所在处。
    /// 应用不支持时返回零矩形，调用方退回鼠标位置。
    pub fn insertion_rect(&self) -> NSRect {
        let mut rect = NSRect::ZERO;
        unsafe {
            let range: NSRange = msg_send![self.object, selectedRange];
            if range.location == NSNotFound as usize {
                return NSRect::ZERO;
            }
            let _: Option<Retained<NSDictionary>> = msg_send![
                self.object,
                attributesForCharacterIndex: range.location,
                lineHeightRectangle: &mut rect
            ];
        }
        rect
    }

    /// 光标（marked text 起点）所在行在屏幕坐标系里的矩形，用来定位候选窗口。
    /// 应用不支持时返回零矩形，窗口就会落在屏幕左下角，至少看得见。
    pub fn caret_rect(&self) -> NSRect {
        let mut rect = NSRect::ZERO;
        unsafe {
            let _: Option<Retained<NSDictionary>> = msg_send![self.object, attributesForCharacterIndex: 0usize, lineHeightRectangle: &mut rect];
        }
        rect
    }
}
