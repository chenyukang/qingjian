/// 一次联想请求要的是什么。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PredictionKind {
    /// 组句中：这段拼音对应的词，可能还要整句补全。
    #[default]
    Compose,

    /// 问字模式（`?` 开头）：用拼音问「三个木是什么字」之类的问题，要答案（字或短答案）与读音。
    Question,

    /// 翻译：把应用里选中的一段文字译成学习语言（壳里快捷键触发），译文放在结果的 `sentence` 里。
    Translate,

    /// 纠错：把应用里选中的一段文字（中文、英文或中英混排）改正错别字、标点与语法（壳里快捷键触发），
    /// 改好的文本放在结果的 `sentence` 里。
    Correct,
}
