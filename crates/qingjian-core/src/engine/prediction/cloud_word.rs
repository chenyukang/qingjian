use crate::candidate::{Candidate, CandidateKind, Language, PartOfSpeech, Sense, Translation};

/// 云端给出的一个词候选：文本加全拼音节，音节用来校验它确实对得上用户敲的拼音，也用来记成用户词。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloudWord {
    /// 词。
    pub text: String,

    /// 全拼音节，如 `["zhang", "tao"]`。问字模式的答案不校验拼音，这里为空。
    pub syllables: Vec<String>,

    /// 显示用读音（问字模式答案的带声调拼音，如 `sēn`）。
    pub reading: Option<String>,

    /// 查询模式（中文 → 英文写法）里模型给的中文解释。
    pub gloss: Option<String>,

    /// 同上：词性缩写（`n.` / `v.` / `adj.` …）。
    pub part_of_speech: Option<String>,
}

impl CloudWord {
    /// 转成云端来源的候选（译文留给 `Engine::annotate` 补）。
    pub fn into_candidate(self) -> Candidate {
        Candidate {
            text: self.text,
            kind: CandidateKind::Cloud,
            syllables: self.syllables,
            reading: self.reading,
            translation: None,
            aux_code: None,
        }
    }

    /// 查询模式的结果转候选：走**英文候选**那条路（上屏吃整段作用域、记个人英文词表、遗忘也照英文来），
    /// 解释用云端给的中文（生僻说法在英→中表里查不到，这时就靠它）。
    pub fn into_lookup_candidate(self) -> Candidate {
        let translation = self
            .gloss
            .filter(|gloss| !gloss.trim().is_empty())
            .map(|gloss| {
                Translation::new(
                    Language::Chinese,
                    vec![Sense {
                        part_of_speech: self
                            .part_of_speech
                            .as_deref()
                            .and_then(|pos| pos.parse::<PartOfSpeech>().ok()),
                        text: gloss.trim().to_owned(),
                        reading: None,
                        fresh: false,
                    }],
                )
            });
        Candidate {
            text: self.text,
            kind: CandidateKind::English,
            syllables: self.syllables,
            reading: None,
            translation,
            aux_code: None,
        }
    }
}
