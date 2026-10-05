//! 附加候选：日期时间等快捷项、中英混输的英文词与补全、emoji。

use super::*;

impl Engine {
    /// 精确匹配自定义输入码时，数字键应选择候选。
    pub(super) fn has_custom_phrase(&self) -> bool {
        !self.english_mode
            && self
                .custom_phrases
                .iter()
                .any(|p| p.enabled && p.code == self.composition.scope())
    }

    /// 所有普通候选完成排序后按输入码固定位置。
    pub(super) fn insert_custom_phrases(&self, items: &mut Vec<Candidate>) {
        if self.english_mode {
            return;
        }
        let mut phrases: Vec<_> = self
            .custom_phrases
            .iter()
            .filter(|p| p.enabled && p.code == self.composition.scope())
            .collect();
        phrases.sort_by_key(|p| p.position);
        for phrase in phrases {
            items.insert(
                (phrase.position - 1).min(items.len()),
                Candidate {
                    text: phrase.text.clone(),
                    kind: CandidateKind::Custom(phrase.position),
                    syllables: Vec::new(),
                    reading: None,
                    translation: None,
                    aux_code: None,
                },
            );
        }
    }

    /// 日期 / 时间 / 星期这类快捷候选插在本地首选之后：`rq` 首选仍是词库里的词，快捷写法紧随其后。
    /// `pinyin` 是解出来的拼音：双拼（`uijm`）与注音下敲的键不是拼音本身，敲的键没命中就拿它再认一遍。
    pub(super) fn insert_shortcuts(
        &self,
        items: &mut Vec<Candidate>,
        scope: &str,
        pinyin: Option<String>,
    ) {
        let expression_char =
            if self.zhuyin && crate::zhuyin::layout::map_key(self.modes().expression).is_some() {
                '\0'
            } else {
                self.modes().expression
            };
        let now = jiff::Zoned::now();
        let mut shortcuts = shortcut::candidates(scope, expression_char, &now);
        if shortcuts.is_empty()
            && let Some(pinyin) = pinyin.as_deref()
            && pinyin != scope
        {
            shortcuts = shortcut::candidates(pinyin, expression_char, &now);
        }
        if shortcuts.is_empty() {
            return;
        }
        let position = items.len().min(1);
        items.splice(position..position, shortcuts);
    }

    /// 中英混输：整段输入是英文词就把它加进候选。
    /// 缺省作为拼音「不像话」（切不动、或除末尾外还有声母缩写 / 残缺音节）时排第一，否则排第二；
    /// 开了中文优先（`chinese_first`）整句 / 首个中文候选已经在前，英文词排第二。没有中文候选时总在第一。
    /// 两字母的全大写缩写（`mp` → MP、`bm` → BM）是个例外：整段太短、几乎总是在打中文（门票 / 编码），
    /// 这种让中文先；超过两个字母的正文英文（cargo / rust）照旧——按词频一刀切会把它们一起挤掉。
    /// 例外只管**没选过**的英文词：用户选过的照旧排第一（选过 OK，下次敲 `ok` 还是 OK 在前）。
    pub(super) fn insert_english(&self, items: &mut Vec<Candidate>, unlikely_pinyin: bool) {
        // `[general] english_in_pinyin`：中文模式下不要词库那份英文词（`ta'm` 全是 tam/Tampa/Tamil）；
        // **自己打过的英文词照旧给** —— 那是用户自己的词，打中文时也可能用（`winlane` 这种）
        let lists: Vec<&WordList> = if self.english_in_pinyin {
            self.english_lists()
        } else {
            self.learner.user_english().into_iter().collect()
        };
        if lists.is_empty() {
            return;
        }
        let text = self.composition.scope();
        if text.contains('\'') {
            return;
        }
        // `[general] english_min_letters`：输入字母太少时直接不给英文候选（这一段几乎总是在打中文）
        if text.bytes().filter(u8::is_ascii_alphabetic).count() < self.english_min_letters {
            return;
        }
        let english_candidate = |word: &str| Candidate {
            text: word.to_owned(),
            kind: CandidateKind::English,
            syllables: Vec::new(),
            reading: None,
            translation: None,
            aux_code: None,
        };
        let word = lists.iter().find_map(|words| words.get(text));
        // 这段字母下用户选中文词（`key` → 可以）比选英文词的次数多：中文词留在第一，英文让到后面；
        // 拼音再不像话也是他自己教的
        let chosen = items
            .first()
            .filter(|c| c.kind == CandidateKind::Chinese)
            .map_or(0, |c| self.learner.choice_weight(text, &c.text));
        let english_weight = word.map_or(0, |w| self.learner.weight(w));
        // 刚才直通进来的字母（`Shift+G` 打 `Google`）：这段必定是英文词，不再按「拼音像不像话」判
        let after_passthrough = self.passthrough_english_word(text).is_some();
        let unlikely_pinyin = unlikely_pinyin || after_passthrough;
        // 那个直通的大写字母被壳直接交给应用了（`Shift+G` 打 `Google`，缓冲区里只剩 `oogle`）：
        // 词表里不认识也把这段当原样候选摆到第一位，按空格收下就等于「教」给它了。
        // 候选文本只放这段本身——直通的 `G` 已经在应用里了，放整词会变成 `GGoogle`。
        // `shift_letter = "compose"` 的用户不需要这一手：大写留在缓冲区里，词表里有就出整词，
        // 没有就照常回车原样上屏（`take_raw` 会把大写还原回来）
        let literal = self.passthrough_english_word(text).map(|_| text.to_owned());
        // 两字母全大写缩写（mp → MP、bm → BM）让中文先：整段太短，几乎总是在打中文。
        // 但**不压过学习记录**：`ok` / `pc` / `ll` 同样满足「两个字母的全大写缩写」，一刀切会把它们一起
        // 翻成中文；而且选过 OK 的用户下次敲 `ok` 本该还是它排第一。所以这条只对用户**没选过**的英文词生效
        //（`record` 对英文候选也记次数，所以 `english_weight` 就是「选过没有」）。
        let short_acronym = english_weight == 0
            && text.len() <= 2
            && word.is_some_and(|word| word.chars().all(|c| c.is_ascii_uppercase()));
        let english_first =
            !self.chinese_first && unlikely_pinyin && chosen <= english_weight && !short_acronym;
        let mut position = if items.is_empty() || english_first {
            0
        } else {
            1
        };
        if let Some(word) = word {
            items.insert(position, english_candidate(word));
            position += 1;
        }
        if let Some(literal) = literal {
            items.retain(|c| {
                !(c.kind == CandidateKind::English && c.text.eq_ignore_ascii_case(&literal))
            });
            items.insert(0, english_candidate(&literal));
            // `retain` 可能把列表缩短（留下的都是从 0 开始的），插完之后把 `position` 夹到长度以内，
            // 否则后面按它插入会越界 panic（热引擎回放时踩到过：`position` 是 2、列表只剩 1 条）
            position = position.min(items.len());
        }
        // 英文补全：拼音不像话时（`compa` 切成 co'm'pa），整段多半是在打英文词的前面几个字母，补全紧跟在精确词之后；
        // 个人词表在前，两张表里都有的只出一次。
        if unlikely_pinyin && text.len() >= MIN_COMPLETION_LETTERS {
            let mut budget = ENGLISH_COMPLETIONS;
            for words in &lists {
                for word in words.complete(text, budget) {
                    if items.iter().any(|c| {
                        c.kind == CandidateKind::English && c.text.eq_ignore_ascii_case(word)
                    }) {
                        continue;
                    }
                    items.insert(position, english_candidate(word));
                    position += 1;
                    budget -= 1;
                }
                if budget == 0 {
                    break;
                }
            }
        }
    }

    /// 给英文候选用的词表，个人的在前、随包的在后；一张都没有就是空。
    /// 整段作用域本身就是个英文词（`database`、`agent`）：用户多半在打那个词。
    pub(in crate::engine) fn scope_is_english_word(&self) -> bool {
        let scope = self.composition.scope();
        !scope.is_empty()
            && self
                .english_lists()
                .iter()
                .any(|words| words.get(scope).is_some())
    }

    pub(super) fn english_lists(&self) -> Vec<&WordList> {
        self.learner
            .user_english()
            .into_iter()
            .chain(self.english.as_ref())
            .collect()
    }

    /// emoji 候选：前几个中文候选里有配 emoji 的，emoji 紧跟在那个词后面，右侧标注它对应的词。
    /// 词后面紧挨着的英文词候选（中文优先时 `key` → 可以、key）不被 emoji 挤开，emoji 排在它之后。
    pub(super) fn insert_emoji(&self, items: &mut Vec<Candidate>) {
        if !self.emoji_on {
            return;
        }
        let Some(table) = &self.emoji else { return };
        let mut inserted = 0;
        let mut index = 0;
        let mut scanned = 0;
        while index < items.len() && scanned < EMOJI_SCAN && inserted < EMOJI_TOTAL {
            let item = &items[index];
            index += 1;
            if !matches!(
                item.kind,
                CandidateKind::Chinese | CandidateKind::Sentence | CandidateKind::English
            ) {
                continue;
            }
            scanned += 1;
            // 英文词按小写查英文表（smile → 😀）
            let key = if item.kind == CandidateKind::English {
                item.text.to_lowercase()
            } else {
                item.text.clone()
            };
            let emojis = table.lookup(&key);
            if emojis.is_empty() {
                continue;
            }
            let word = item.text.clone();
            let syllables = item.syllables.clone();
            if item.kind != CandidateKind::English {
                while items
                    .get(index)
                    .is_some_and(|next| next.kind == CandidateKind::English)
                {
                    index += 1;
                }
            }
            for emoji in emojis.iter().take(EMOJI_PER_WORD) {
                if inserted >= EMOJI_TOTAL {
                    break;
                }
                items.insert(
                    index,
                    Candidate {
                        text: emoji.clone(),
                        kind: CandidateKind::Emoji,
                        syllables: syllables.clone(),
                        reading: Some(word.clone()),
                        translation: None,
                        aux_code: None,
                    },
                );
                index += 1;
                inserted += 1;
            }
        }
    }
}
