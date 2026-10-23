//! 拼音以外的几种查询：表达式（`v`）、原样字母、英文模式、问字（`?`）。

use super::*;

impl Engine {
    /// 表达式模式（`v` 开头）：不解析拼音，候选是算式结果 / 中文数字，再加上整段是英文词的情况（`very`）。
    /// preedit 原样显示输入。
    pub(in crate::engine) fn query_expression(
        &self,
        scope: &str,
        rest: String,
        start: Instant,
    ) -> Query {
        let mut items = shortcut::candidates(scope, self.modes().expression, &jiff::Zoned::now());
        if let Some(word) = self.english.as_ref().and_then(|english| english.get(scope)) {
            items.push(Candidate {
                text: word.to_owned(),
                kind: CandidateKind::English,
                syllables: Vec::new(),
                reading: None,
                translation: None,
                aux_code: None,
            });
        }
        Query {
            segmentations: Vec::new(),
            candidates: CandidateList { items },
            tail: scope.to_owned(),
            text: self.composition.typed_text(),
            cursor: self.composition.cursor(),
            rest,
            decoded_keys: self.shuangpin.is_some() || self.zhuyin,
            shuangpin_raw_preedit: false,
            typed_display: None,
            correction: None,
            aux: None,
            timings: Timings {
                parse: Duration::ZERO,
                lookup: Duration::ZERO,
                rank: start.elapsed(),
            },
        }
    }

    /// 直输段的前缀补全。只对纯 ASCII 标识符做（`湘BA` 这种汉字混排不做）；
    /// 词表与拼音那条路同一个口径：开着随包英文词表时两张都用，只查用户词表时只查用户的。
    fn extend_raw_completions(&self, items: &mut Vec<Candidate>, scope: &str) {
        if scope.len() < crate::engine::MIN_COMPLETION_LETTERS
            || !scope.bytes().any(|b| b.is_ascii_alphabetic())
            || !scope
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return;
        }
        // 词表的编码是小写
        let prefix = scope.to_ascii_lowercase();
        let user = self.learner.user_english();
        let lists: Vec<&qingjian_dictionary::WordList> = if self.english_in_pinyin() {
            self.english_lists()
        } else {
            user.into_iter().collect()
        };
        let mut budget = crate::engine::ENGLISH_COMPLETIONS;
        for words in &lists {
            for word in words.complete(&prefix, budget) {
                if items.iter().any(|c| c.text.eq_ignore_ascii_case(word)) {
                    continue;
                }
                items.push(Candidate {
                    text: word.to_owned(),
                    kind: CandidateKind::English,
                    syllables: Vec::new(),
                    reading: None,
                    translation: None,
                    aux_code: None,
                });
                budget = budget.saturating_sub(1);
            }
            if budget == 0 {
                break;
            }
        }
    }

    /// 英文直输段：第一条候选就是原文（`no-way`），空格 / 回车都上屏它；preedit 原样显示。
    /// 后面再挂上前缀补全（`send_tr` → `send_transaction`）—— 原样那条永远第一，语义不变，
    /// 只是打半截标识符时不用退回去从头打。
    pub(in crate::engine) fn query_raw(&self, scope: &str, rest: String, start: Instant) -> Query {
        let mut items = vec![Candidate {
            text: scope.to_owned(),
            kind: CandidateKind::English,
            syllables: Vec::new(),
            reading: None,
            translation: None,
            aux_code: None,
        }];
        self.extend_raw_completions(&mut items, scope);
        Query {
            segmentations: Vec::new(),
            candidates: CandidateList { items },
            tail: scope.to_owned(),
            text: self.composition.typed_text(),
            cursor: self.composition.cursor(),
            rest,
            decoded_keys: self.shuangpin.is_some() || self.zhuyin,
            shuangpin_raw_preedit: false,
            typed_display: None,
            correction: None,
            aux: None,
            timings: Timings {
                parse: Duration::ZERO,
                lookup: Duration::ZERO,
                rank: start.elapsed(),
            },
        }
    }

    /// 英文模式：敲的字母原样显示，候选是英文词表的精确词、前缀补全与拼错纠正（见 [`english::suggest`]），
    /// 词表没装就没有候选。emoji 照配，但排在所有词后面：选词靠上下键，emoji 夹在词中间会挡路。
    pub(in crate::engine) fn query_english(
        &self,
        scope: &str,
        rest: String,
        start: Instant,
    ) -> Query {
        // `[general] english_mode_min_letters`：敲一两个字母先别弹一串补全（缺省 2 个字母起）
        let letters = scope.bytes().filter(u8::is_ascii_alphabetic).count();
        let mut items: Vec<Candidate> = if letters < self.english_mode_min_letters {
            Vec::new()
        } else {
            english::suggest(
                &self.english_lists(),
                scope,
                |text| self.learner.weight(text),
                ENGLISH_MODE_CANDIDATES,
            )
            .into_iter()
            .map(|text| Candidate {
                text,
                kind: CandidateKind::English,
                syllables: Vec::new(),
                reading: None,
                translation: None,
                aux_code: None,
            })
            .collect()
        };
        self.insert_emoji(&mut items);
        items.sort_by_key(|c| c.kind == CandidateKind::Emoji);
        Query {
            segmentations: Vec::new(),
            candidates: CandidateList { items },
            tail: scope.to_owned(),
            text: self.composition.typed_text(),
            cursor: self.composition.cursor(),
            rest,
            decoded_keys: self.shuangpin.is_some() || self.zhuyin,
            shuangpin_raw_preedit: false,
            typed_display: None,
            correction: None,
            aux: None,
            timings: Timings {
                parse: Duration::ZERO,
                lookup: Duration::ZERO,
                rank: start.elapsed(),
            },
        }
    }

    /// 问字模式（问字键或 `?` 开头）：拼音问题本地没有候选，preedit 显示前缀加切分好的问题拼音，答案等云端；
    /// 十六进制码点（`u4e00`、`u+1f600`）本地直接给出那个字符。
    pub(in crate::engine) fn query_question(
        &self,
        scope: &str,
        rest: String,
        start: Instant,
    ) -> Query {
        let body = self.modes().question_body(scope, self.zhuyin);
        let prefix = &scope[..scope.len() - body.len()];
        let (candidates, tail) = match shortcut::unicode_form(body) {
            Some(text) => (
                CandidateList {
                    items: vec![Candidate {
                        text,
                        kind: CandidateKind::Shortcut,
                        syllables: Vec::new(),
                        reading: None,
                        translation: None,
                        aux_code: None,
                    }],
                },
                scope.to_owned(),
            ),
            None => (
                CandidateList::default(),
                format!("{prefix}{}", self.marked_rest(body)),
            ),
        };
        Query {
            segmentations: Vec::new(),
            candidates,
            tail,
            text: self.composition.typed_text(),
            cursor: self.composition.cursor(),
            rest,
            decoded_keys: self.shuangpin.is_some() || self.zhuyin,
            shuangpin_raw_preedit: false,
            typed_display: None,
            correction: None,
            aux: None,
            timings: Timings {
                parse: start.elapsed(),
                lookup: Duration::ZERO,
                rank: Duration::ZERO,
            },
        }
    }
}
