use objc2_foundation::NSRange;

/// 一次「翻译选中文字」：从按下快捷键到用户接受或放弃。
#[derive(Debug, Clone)]
pub struct TranslationJob {
    /// 选区在应用里的范围，接受时用译文替换它。
    pub range: NSRange,

    /// 云端回来的译文 / 纠错结果；`None` 表示还在等。
    pub result: Option<String>,

    /// 交给云端之前选中的原文：结果与它一样时不必让用户按回车（等于什么都没改）。
    pub original: String,

    /// 结果与原文一样时显示的提示（翻译与纠错说法不同）。
    pub unchanged_notice: &'static str,
}
