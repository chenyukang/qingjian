//! 查询模式：中文想法 → 对方语言的写法（缺省英语）。
//!
//! 分两段，判断都在这里：
//!
//! 1. **挑中文**（`lookup_source` 为空）：候选照旧是中文，让用户先确认「要查的是哪一句」——
//!    拼音重码多，长句尤其有语义差别。短词例外：头一个候选 ≥2 字、覆盖整段拼音、本地释义表
//!    里有它，就自动挑中它（「敷衍」一步到位，不必确认）。
//! 2. **查写法**（`lookup_source` 里有值）：候选换成这条中文的写法。数据来自随包释义表
//!    （中文词 → 学习语言释义）：`敷衍` 的 `perfunctory` 与 `to go through the motions`
//!    各成一条候选。本地查不到（整句、说法）返回 false，壳据此发云请求兜底。
//!
//! 候选都走 [`CandidateKind::English`]：上屏吃整段作用域、记个人英文词表、遗忘也照英文来，
//! 全都不用另写。词性 / 中文解释也不在这里填 —— [`Engine::annotate`] 按英文候选那条路补
//!（英→中释义表），表里没有时保留这里带的「源词 + 词性」（`v. 敷衍`）。

use super::*;
use crate::candidate::{Language, Sense, Translation};

/// 英文候选最多给几条（一页 9 格，够挑）。
const LOOKUP_LIMIT: usize = 9;

/// 自动挑中文的字数上限：到此为止算「短词」（敷衍、我的老师），再长就是句子，留给用户挑。
const LOOKUP_AUTO_CHARS: usize = 4;

impl Engine {
    /// 第一段：短词且本地释义表里有它，就自动挑中（返回是否挑中）。
    pub(in crate::engine) fn auto_pick_lookup_source(&self, items: &[Candidate]) -> bool {
        let Some(top) = items.first() else {
            return false;
        };
        // 单个字不当来源：「了」「的」这种查出来是语法说明（already / past tense marker），
        // 不是用户想表达的意思
        let chars = top.text.chars().count();
        if !(2..=LOOKUP_AUTO_CHARS).contains(&chars) {
            return false;
        }
        // 必须覆盖整段拼音：只对上尾巴的候选（`wode'l` 里的「了」）不是用户要查的那个词。
        // 作用域是原样敲的键（可能带分隔符），两边都去掉分隔符再比
        if top.syllables.join("") != strip_separators(self.composition.scope()) {
            return false;
        }
        if self.lookup_translation(&top.text).is_none() {
            return false;
        }
        tracing::info!(text = %top.text, "查询模式：短词自动挑中文");
        self.set_lookup_source(&top.text);
        true
    }

    /// 本地释义表里这条中文的释义（查询模式专用表优先，没装就退回学习语言那张）。
    pub(in crate::engine) fn lookup_translation(&self, chinese: &str) -> Option<Translation> {
        self.lookup_translator
            .as_deref()
            .and_then(|translator| translator.translate(chinese))
            .or_else(|| self.translator.translate(chinese))
    }

    /// 第二段：把中文候选换成这条中文的英文写法。返回是否换出了候选（没有就是本地查不到，壳去问云端）。
    pub(in crate::engine) fn expand_lookup_candidates(&self, items: &mut Vec<Candidate>) -> bool {
        let Some(chosen) = self.lookup_source() else {
            return false;
        };
        // 拼音沿用候选中那一条：上屏时照常吃掉这段拼音
        let syllables = items
            .iter()
            .find(|candidate| candidate.text == chosen)
            .map(|candidate| candidate.syllables.clone())
            .unwrap_or_default();
        let Some(translation) = self.lookup_translation(&chosen) else {
            tracing::info!(chinese = %chosen, "查询模式：本地释义表里没有对应的写法（等壳发云请求）");
            items.clear();
            return false;
        };
        let mut out: Vec<Candidate> = Vec::new();
        for sense in translation.senses() {
            let text = sense.text.trim();
            if text.is_empty() || out.iter().any(|c| c.text.eq_ignore_ascii_case(text)) {
                continue;
            }
            out.push(Candidate {
                text: text.to_owned(),
                kind: CandidateKind::English,
                syllables: syllables.clone(),
                reading: None,
                // 「源词 + 词性」当解释：英→中表里没有这个词时 `annotate` 会保留它
                translation: Some(Translation::new(
                    Language::Chinese,
                    vec![Sense {
                        part_of_speech: sense.part_of_speech,
                        text: chosen.clone(),
                        reading: None,
                        fresh: false,
                    }],
                )),
                aux_code: None,
            });
            if out.len() >= LOOKUP_LIMIT {
                break;
            }
        }
        // 查不到也把中文候选换掉：查询模式里冒出中文会让人看不懂自己在哪个模式，
        // 空表交给壳显示「查义中…」
        let found = !out.is_empty();
        *items = out;
        found
    }
}

/// 去掉拼音里的音节分隔符（`wo'de` → `wode`）。
fn strip_separators(pinyin: &str) -> String {
    pinyin.chars().filter(|c| *c != '\'').collect()
}
