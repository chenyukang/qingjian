use super::cloud_word::CloudWord;

/// 联想结果。只是候选之外的补充展示，**不重排本地候选**：云端词补进第一页末尾几格。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prediction {
    /// 对应的请求序号。
    pub sequence: u64,

    /// 这段拼音可能对应的词（已按拼音校验）。
    pub words: Vec<CloudWord>,

    /// 组句中的整句补全，替换整段拼音。
    pub sentence: Option<String>,

    /// 手动任务（纠错）的结果里，哪几段和原文不同（字符区间，给弹窗上色用）。
    /// 翻译与普通联想为空；语义与算法见 [`crate::text_diff::changed_ranges`]。
    pub sentence_marks: Vec<(usize, usize)>,
}

impl Prediction {
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.sentence.is_none()
    }
}
