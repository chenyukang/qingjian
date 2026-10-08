//! 候选窗口的一行：序号、候选词、annotation 片段。

use super::tone::Tone;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 显示用序号文本，如 `1`。
    pub index: String,

    /// 候选词。
    pub text: String,

    /// 紧跟在候选词后面的辅码，如 `[kf]`：码是词本身的属性，不进右侧的 annotation 列。
    pub code: Option<String>,

    /// 右侧 annotation，按顺序绘制；没有译文时为空。
    pub annotation: Vec<(String, Tone)>,

    /// 来自云联想：词前画一个小云朵，与本地候选区分。
    pub cloud: bool,

    /// 需要标出来的字符区间（原文下标；纠错结果里相对原文改动的那几段）。
    /// 空就是不标。壳把 Core 的 `Candidate::marks` 搬过来。
    pub marks: Vec<(usize, usize)>,
}

impl Row {
    /// 只有序号和候选词的一行。
    pub fn plain(index: usize, text: impl Into<String>) -> Self {
        Self {
            index: (index + 1).to_string(),
            text: text.into(),
            code: None,
            annotation: Vec::new(),
            cloud: false,
            marks: Vec::new(),
        }
    }
}
