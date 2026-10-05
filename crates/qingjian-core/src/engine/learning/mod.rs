//! 学习与统计的挂钩：释义兜底回填、词汇曝光、输入统计、输入日志、删候选、定时落盘。

use super::Engine;
use super::input_log::{CommitEntry, InputLogEntry, InputLogger, InputSource, LOGGED_CANDIDATES};
use super::statistics::Usage;
use super::vocabulary::{FRESH_UNTIL, VocabularySummary};
use crate::candidate::{Candidate, CandidateKind, Translation};
use crate::sentence;

mod forgotten;
mod learner;
mod muted;
mod preference;

pub use forgotten::Forgotten;
pub use learner::{Learner, NoLearner};
pub(super) use muted::MutedLearner;
pub use preference::SortPreference;

impl Engine {
    /// 取回释义兜底写好的释义，记进译者（个人释义表）；返回学了几条。壳定时调，不阻塞。
    /// 结果的语言与当前学习语言对不上（中途切过语言）就丢。
    pub fn poll_glosses(&mut self) -> usize {
        let filled = self.gloss_filler.poll();
        let mut learned = 0;
        for gloss in filled {
            if gloss.translation.language == self.translator.language() {
                tracing::debug!(word = %gloss.word, "释义兜底写入个人释义表");
                self.translator.learn(&gloss.word, gloss.translation);
                learned += 1;
            }
        }
        learned
    }

    /// 当前学习语言的词汇汇总（偏好设置「统计」页）。
    pub fn vocabulary_summary(&self) -> VocabularySummary {
        self.vocabulary.summary(self.translator.language())
    }

    /// 壳画完候选窗口后告知当前页上的候选：页上的译词在用户上屏那一刻记成「看到过」（[`super::vocabulary::VocabularyTracker`]）。
    /// 每次重画都换掉上一页，逐键刷新时一闪而过的候选不算；窗口收起时传空。
    pub fn note_displayed<'a>(&mut self, candidates: impl IntoIterator<Item = &'a Candidate>) {
        self.displayed.clear();
        for candidate in candidates {
            if candidate.kind == CandidateKind::English {
                continue;
            }
            let Some(translation) = &candidate.translation else {
                continue;
            };
            for sense in translation.senses() {
                let key = (translation.language, sense.text.clone());
                if !self.displayed.contains(&key) {
                    self.displayed.push(key);
                }
            }
        }
    }

    /// 上屏了：当前页上的译词都算看到过一轮。
    pub(super) fn record_exposures(&mut self) {
        if self.private {
            // 私密输入中的候选页不能污染词汇记录；同时丢掉暂存页，避免之后恢复普通输入时补记。
            self.displayed.clear();
            return;
        }
        for (language, word) in std::mem::take(&mut self.displayed) {
            self.vocabulary.record_exposure(language, &word);
        }
    }

    /// 按词汇记录给译词标生词：看到的轮次不到 [`FRESH_UNTIL`] 的算。
    pub(super) fn mark_fresh(&self, translation: &mut Translation) {
        let language = translation.language;
        for sense in translation.senses_mut() {
            sense.fresh = self.vocabulary.exposures(language, &sense.text) < FRESH_UNTIL;
        }
    }

    /// 往输入统计记一次上屏：汉字与英文词按文字数，中文词数按来源定（选一个词算一个，整句按语言模型切出来的词数，
    /// 切不了就按字数）。`english_word` 是原样上屏的字母串算不算一个英文词（拼音回车不算）。
    pub(super) fn meter_commit(&mut self, text: &str, source: InputSource, english_word: bool) {
        let mut usage = Usage::of_text(text);
        usage.words = match source {
            InputSource::Word | InputSource::Cloud => 1,
            InputSource::Sentence | InputSource::CloudSentence => {
                sentence::segment_text(text, &*self.language_model).map_or(usage.hanzi, |clauses| {
                    clauses.iter().map(|words| words.len() as u64).sum()
                })
            }
            InputSource::English
            | InputSource::Custom
            | InputSource::Shortcut
            | InputSource::Emoji
            | InputSource::Raw
            | InputSource::Translation => 0,
        };
        if source == InputSource::Raw && !english_word {
            usage.english_words = 0;
        }
        self.meter.record(usage);
    }

    /// 往输入日志记一次上屏。`keys` 是这次消耗掉的原始键，`index` 从上一次查询的候选里找。
    /// 攒着的直通字符先写出去（顺序要对）；这段组句里退格过且最终键串变了，紧跟着记一条 `retype`。
    pub(super) fn log_commit(&mut self, keys: &str, text: &str, source: InputSource) -> u64 {
        self.record_exposures();
        self.flush_passthrough();
        self.log_sequence += 1;
        let snapshot = self.last_query.borrow().clone().unwrap_or_default();
        let index = snapshot.candidates.iter().position(|c| c == text);
        let ms = self
            .composition_started
            .map_or(0, |started| started.elapsed().as_millis() as u64);
        self.logger.record(InputLogEntry::Commit(CommitEntry {
            id: self.log_sequence,
            scope: snapshot.scope,
            keys: keys.to_owned(),
            pinyin: snapshot.pinyin,
            corrected: snapshot.corrected,
            text: text.to_owned(),
            source,
            index,
            top: snapshot
                .candidates
                .into_iter()
                .take(LOGGED_CANDIDATES)
                .collect(),
            scheme: self.scheme_key(),
            english: self.english_mode,
            rescored: snapshot.rescored,
            app: self.application.clone(),
            pages: self.page_turns,
            ms,
        }));
        self.committed_since_break = true;
        self.page_turns = 0;
        if let Some(before) = self.retype_snapshot.take() {
            let after = self.composition.scope();
            if before != after {
                self.logger.record(InputLogEntry::Retype {
                    before,
                    after: after.to_owned(),
                    of: self.log_sequence,
                });
            }
        }
        self.log_sequence
    }

    /// 用户要求删掉一个候选（修饰键 + 数字）：中文词与云端词交给 Learner 删用户词、清学习；英文词删个人英文词；
    /// 整句、快捷候选、emoji 没什么可删。删完缓存作废，它也不再当下一个词的上文。
    pub fn forget(&mut self, candidate: &Candidate) -> Forgotten {
        self.sort_candidate(candidate, SortPreference::Down)
    }

    /// 用户要求这个候选以后别再出现（`⌃+数字`）：词库里的词记成「隐藏」，候选组装完就滤掉；
    /// 自己学过的词与 [`Self::forget`] 一样真的删掉。
    pub fn hide(&mut self, candidate: &Candidate) -> Forgotten {
        self.sort_candidate(candidate, SortPreference::Hidden)
    }

    /// 删候选 / 后置 / 隐藏：删得掉就删，删不掉（词库里的词）就切排序偏好。
    fn sort_candidate(&mut self, candidate: &Candidate, target: SortPreference) -> Forgotten {
        let mut candidate_owned = candidate.clone();
        if self.traditional
            && let Some(simp) = self.traditional_map.borrow().get(&candidate_owned.text)
        {
            candidate_owned.text = simp.clone();
        }
        let candidate = &candidate_owned;
        let cycleable = matches!(
            candidate.kind,
            CandidateKind::Chinese
                | CandidateKind::Code
                | CandidateKind::Cloud
                | CandidateKind::English
        );
        let forgotten = match candidate.kind {
            CandidateKind::Chinese | CandidateKind::Code | CandidateKind::Cloud => {
                self.learner.forget(&candidate.text)
            }
            CandidateKind::English => Forgotten {
                user_word: self.learner.forget_english(&candidate.text),
                learning: false,
                preference: None,
            },
            CandidateKind::Sentence
            | CandidateKind::Shortcut
            | CandidateKind::Custom(_)
            | CandidateKind::Emoji
            | CandidateKind::Generated => Forgotten::default(),
        };
        // 词库里的词删不掉（没有用户词、也没有学习记录），但用户按这个键的本意是「别再让它挡在前面」/
        // 「以后别出现」：改记排序偏好，再按一次恢复。整句 / 快捷 / emoji 没什么可记的，保持原来的提示
        let forgotten = if forgotten.is_nothing() && cycleable {
            Forgotten {
                preference: Some(self.learner.toggle_sort_preference(&candidate.text, target)),
                ..forgotten
            }
        } else {
            forgotten
        };
        if !forgotten.is_nothing() {
            self.forget_span_cache();
            *self.correction_cache.borrow_mut() = None;
            if self.chain.mentions(&candidate.text) {
                self.chain.reset();
            }
            self.recent_commits.retain(|c| c.text != candidate.text);
            tracing::debug!(text = %candidate.text, ?forgotten, "删除候选");
        }
        forgotten
    }

    /// 把某个词恢复成正常排序（设置页列表里那一行的按钮）。它不知道当前是哪一档，直接清掉。
    pub fn restore_sort_preference(&mut self, text: &str) -> SortPreference {
        let current = self.learner.sort_preference(text);
        if current != SortPreference::Normal {
            self.learner.toggle_sort_preference(text, current);
            self.forget_span_cache();
            *self.correction_cache.borrow_mut() = None;
        }
        self.learner.sort_preference(text)
    }

    /// 把所有标过「后置 / 隐藏」的词恢复成正常排序（设置页那个按钮）。返回恢复了几条。
    /// 后置的词沉在候选列表最末，翻页很难够到，所以需要一个不依赖候选窗的入口。
    pub fn restore_sort_preferences(&mut self) -> usize {
        let words: Vec<String> = self
            .learner
            .sort_preferences()
            .into_iter()
            .map(|(word, _)| word)
            .collect();
        for word in &words {
            self.restore_sort_preference(word);
        }
        words.len()
    }

    /// 把学习数据与输入日志落盘。壳在停用输入法时调，激活期间也可以定时调（进程被杀时少丢）：
    /// 落盘不改学习状态，所以不像 [`Self::learner_mut`] 那样作废格子缓存。
    pub fn flush_learning(&mut self) {
        self.learner.flush();
        self.logger.flush();
        self.meter.flush();
        self.vocabulary.flush();
        self.translator.flush();
    }

    /// 词库 / 用户词 / 学习数据变了，整句格子候选全部作废。
    pub(super) fn forget_span_cache(&self) {
        self.span_cache.borrow_mut().clear();
    }
}
