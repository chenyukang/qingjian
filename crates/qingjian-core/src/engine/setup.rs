//! 注入与开关：词库、模糊音、双拼、翻译 / 学习 / 联想等 trait 实现的挂接，以及相应的只读访问。

use super::aux_code::is_valid_aux_code_key;
use super::*;
use crate::engine::decoded::EngineDecoded;

impl Engine {
    /// 设置中文模式的标点转换。
    pub fn set_full_width_punctuation(&mut self, enabled: bool) {
        self.full_width_punctuation = enabled;
    }

    /// 原子更新自定义短语，非法规则保持旧值。
    pub fn set_custom_phrases(&mut self, phrases: Vec<crate::CustomPhrase>) -> Result<(), String> {
        crate::custom_phrase::validate_phrases(&phrases)?;
        self.custom_phrases = phrases;
        Ok(())
    }

    /// 设双拼方案，`None` 回到全拼。纠错缓存按作用域记而作用域的含义变了，一并清掉。
    pub fn set_shuangpin(&mut self, scheme: Option<Scheme>) {
        self.shuangpin = scheme;
        *self.correction_cache.borrow_mut() = None;
    }

    pub fn shuangpin(&self) -> Option<Scheme> {
        self.shuangpin
    }
    /// 换双拼 preedit 显示形式：开着时显示原始按键（如 `ljse`），关着（缺省）展开成全拼（`lan'se`）。
    pub fn set_shuangpin_raw_preedit(&mut self, enabled: bool) {
        self.shuangpin_raw_preedit = enabled;
    }

    pub fn shuangpin_raw_preedit(&self) -> bool {
        self.shuangpin_raw_preedit
    }

    /// 挂上辅码码表（用户导入的与随包的笔画表）。照 [`Self::set_extra_dictionaries`] 的模式：壳按目录与配置装配。
    pub fn with_aux_codes(mut self, tables: Vec<Arc<dyn AuxCodeLookup>>) -> Self {
        self.aux_codes = tables;
        self
    }

    /// 运行时换码表（导入 / 移除 / 开关之后）；顺带退出辅码态，免得筛的是一张已经不在的表。
    pub fn set_aux_codes(&mut self, tables: Vec<Arc<dyn AuxCodeLookup>>) {
        self.aux_codes = tables;
        self.aux_code = None;
    }

    pub fn aux_codes(&self) -> &[Arc<dyn AuxCodeLookup>] {
        &self.aux_codes
    }

    /// 换辅码触发键（配置项 `[general] aux_code_key`）。非法的键（含当前翻页键 `page_keys`，
    /// 配置项 `[general] page_keys`）退回缺省 `;`。
    pub fn set_aux_code_key(&mut self, key: char, page_keys: (char, char)) {
        self.aux_code_key = if is_valid_aux_code_key(key, page_keys) {
            key
        } else {
            DEFAULT_AUX_CODE_KEY
        };
    }

    /// 换「码删空后留在辅码态」开关（配置项 `[general] aux_code_keep_empty`，缺省开）。
    pub fn set_aux_keep_empty(&mut self, keep: bool) {
        self.aux_keep_empty = keep;
    }

    /// 换辅码总开关（配置项 `[aux_code] enabled`，缺省关）。关掉时辅码整线关：触发键不进辅码态，
    /// 纯拼音态也不逐候查首条码；开着但没有码表（`aux_codes` 为空）同样不进辅码态。
    pub fn set_aux_enabled(&mut self, enabled: bool) {
        self.aux_enabled = enabled;
    }

    /// 换「候选上显示码」开关（配置项 `[general] aux_code_show`，缺省关）：纯拼音态挂不挂首条码。
    pub fn set_aux_show(&mut self, show: bool) {
        self.aux_show = show;
    }

    /// 設置是否啟用注音模式。開啟後鍵盤輸入按大千佈局解析。
    /// 学习开关（`[general] learning`）：关掉后不再记词频、用户词、个人 n-gram 与敲错表，已学的照常参与排序；
    /// 私密输入是另一个独立的开关（[`Self::set_private`]）。
    pub fn set_learning(&mut self, enabled: bool) {
        self.learner.set_disabled(!enabled);
    }

    pub fn set_zhuyin_mode(&mut self, on: bool) {
        self.zhuyin = on;
        self.forget_span_cache();
    }

    /// 設置是否啟用繁體輸出模式。
    pub fn set_traditional_mode(&mut self, on: bool) {
        self.traditional = on;
        if on && self.opencc.is_none() {
            match ferrous_opencc::OpenCC::from_config(ferrous_opencc::config::BuiltinConfig::S2tw) {
                Ok(opencc) => self.opencc = Some(opencc),
                Err(error) => tracing::warn!(%error, "繁体转换器初始化失败，候选仍是简体"),
            }
        }
    }

    /// 目前是否處於注音模式。
    pub fn is_zhuyin_mode(&self) -> bool {
        self.zhuyin
    }

    /// 换形码码表（五笔），`None` 回到拼音的诸方案。编码与拼音是两套键，纠错缓存一并清掉。
    pub fn set_code_table(&mut self, table: Option<CodeTable>) {
        self.code = table;
        *self.correction_cache.borrow_mut() = None;
        self.forget_span_cache();
    }

    /// 当前的形码码表；`None` 表示走拼音（全拼 / 双拼 / 注音）。
    pub fn code_table(&self) -> Option<&CodeTable> {
        self.code.as_ref()
    }

    /// 是否处在形码方案下。
    pub fn is_code_mode(&self) -> bool {
        self.code.is_some()
    }

    /// 拼音侧参不参与查询。形码开着时把它关掉就是「只用形码」（`[general] scheme = "none"`）；
    /// 两边都开是混输，见 [`Self::set_code_table`] 与 [`Self::query_mixed`]。
    pub fn set_phonetic(&mut self, on: bool) {
        self.phonetic = on;
        *self.correction_cache.borrow_mut() = None;
        self.forget_span_cache();
    }

    pub fn is_phonetic(&self) -> bool {
        self.phonetic
    }

    /// 判斷注音模式下目前是否還需要輸入聲調。
    /// 供殼（平台層）用來判斷空白鍵是應該進緩衝區作為聲調，還是直接用來選詞。
    pub fn zhuyin_needs_tone(&self) -> bool {
        if !self.zhuyin {
            return false;
        }
        let raw = self.composition.text();
        if raw.is_empty() {
            return false;
        }
        let decoded = crate::zhuyin::decode(raw);
        if let Some(last) = decoded.units().last() {
            !last.complete && last.pinyin != "'"
        } else {
            false
        }
    }

    /// 组句中敲 `;` 是否该进缓冲区：微软 / 搜狗双拼里它是 ing 的韵母键，只在末尾有落单的声母时收，
    /// 其他时候仍是标点。问字模式（`?x`）看的是前缀之后的部分。
    pub fn takes_semicolon(&self) -> bool {
        let body = self
            .modes()
            .question_body(self.composition.scope(), self.zhuyin);
        self.shuangpin
            .filter(|scheme| scheme.uses_semicolon())
            .is_some_and(|scheme| scheme.decode(body).pending_initial())
    }

    /// 只用形码：码表挂着、拼音侧关着。
    pub(super) fn code_only(&self) -> bool {
        self.code.is_some() && !self.phonetic
    }

    /// 混输：码表挂着、拼音侧也开着。
    pub(super) fn mixed(&self) -> bool {
        self.code.is_some() && self.phonetic
    }

    /// 有效的模式键：只用形码时所有字母都是字根键，只剩 `?` 开头的问字；
    /// 双拼与混输下小写字母各有用处，换成大写字母。
    pub(super) fn modes(&self) -> ModeKeys {
        if self.code_only() {
            self.modes.letterless()
        } else if self.shuangpin.is_some() || self.mixed() {
            self.modes.shifted()
        } else {
            self.modes
        }
    }

    /// 缓冲区为空时敲的大写字母该不该进表达式 / 问字模式：只在双拼或混输下、且是模式键的大写时。
    /// 壳只在中文模式、Caps 灭时问。
    pub fn takes_mode_letter(&self, c: char) -> bool {
        let modes = self.modes();
        (self.shuangpin.is_some() || self.mixed())
            && !self.zhuyin
            && (c == modes.expression || c == modes.question)
    }

    /// 缓冲区为空时敲 `?` 该不该进问字模式（配置 `[shortcut] question_mark`）：壳据此决定问号是入口还是标点。
    pub fn takes_question_mark(&self) -> bool {
        self.modes().question_mark
    }

    /// 双拼开着时把一段键解成全拼；全拼下为 `None`，调用方原样用键。
    pub(super) fn decode(&self, keys: &str) -> Option<EngineDecoded> {
        if self.zhuyin {
            Some(EngineDecoded::Zhuyin(crate::zhuyin::decode(keys)))
        } else {
            self.shuangpin
                .map(|scheme| EngineDecoded::Shuangpin(scheme.decode(keys)))
        }
    }

    /// 光标后剩余拼音的显示形式：双拼先解码；能切就按音节用 `'` 连上，切不动就原样。
    /// 只用形码时剩余段是编码，原样显示。
    pub(super) fn marked_rest(&self, rest: &str) -> String {
        if self.code_only() || (self.shuangpin.is_some() && self.shuangpin_raw_preedit) {
            return rest.to_owned();
        }
        match self.decode(rest) {
            Some(decoded) => decoded.marked(),
            None => marked_rest(rest),
        }
    }

    pub fn with_emoji(mut self, table: EmojiTable) -> Self {
        self.emoji = Some(table);
        self
    }

    /// emoji 候选开关（`[general] emoji_candidates`）：关掉只是不再插 emoji 候选，表留着随时能再开。
    pub fn set_emoji(&mut self, on: bool) {
        self.emoji_on = on;
    }

    pub fn with_fuzzy(mut self, rules: FuzzyRules) -> Self {
        self.fuzzy = rules;
        self
    }

    /// 换模糊音规则：格子缓存里的代价随写法变，一起作废。
    pub fn set_fuzzy(&mut self, rules: FuzzyRules) {
        if self.fuzzy != rules {
            self.forget_span_cache();
        }
        self.fuzzy = rules;
    }

    pub fn fuzzy(&self) -> FuzzyRules {
        self.fuzzy
    }

    pub fn with_predictor(mut self, predictor: Box<dyn Predictor>) -> Self {
        self.predictor = predictor;
        self
    }

    /// 运行时换掉 Predictor（菜单开关云联想 / 配置热加载）；正在等的联想一并作废。
    pub fn set_predictor(&mut self, predictor: Box<dyn Predictor>) {
        self.cancel_prediction();
        self.predictor = predictor;
    }

    /// 挂上同步的整句重打分器（字级 Transformer，查询里当场打分，评测用）。`weight` 是神经分的权重 λ，
    /// `margin` 是参与重排的路径分门槛（nat），`context` 是给模型看的前文字符数；
    /// `None` 用缺省 [`NEURAL_WEIGHT`] / [`NEURAL_MARGIN`] / [`RESCORE_CONTEXT_CHARS`]。
    pub fn with_sentence_scorer(
        mut self,
        scorer: Box<dyn SentenceScorer>,
        weight: Option<f64>,
        margin: Option<f64>,
        context: Option<usize>,
    ) -> Self {
        self.sentence_scorer = Some(scorer);
        self.rescorer = None;
        self.set_neural_parameters(weight, margin, context);
        self
    }

    /// 挂上异步的整句重打分器：打分在后台线程，查询不等它，壳在停顿后 [`Self::request_rescoring`]、
    /// 结果到了 [`Self::poll_rescoring`] 后再查一次。参数同 [`Self::with_sentence_scorer`]。
    pub fn with_async_sentence_scorer(
        mut self,
        scorer: Box<dyn SentenceScorer>,
        weight: Option<f64>,
        margin: Option<f64>,
        context: Option<usize>,
    ) -> Self {
        self.set_async_sentence_scorer(Some(scorer));
        self.set_neural_parameters(weight, margin, context);
        self
    }

    /// 运行时换 / 卸异步重打分器（壳里模型在后台加载完才接上，配置关掉就卸）。
    pub fn set_async_sentence_scorer(&mut self, scorer: Option<Box<dyn SentenceScorer>>) {
        self.sentence_scorer = None;
        self.rescorer = scorer.map(super::rescoring::RescoreWorker::spawn);
        *self.neural_cache.borrow_mut() = super::rescoring::NeuralCache::default();
        self.forget_span_cache();
    }

    /// 换一组个人 n-gram 插值参数（回放调参用）；整句格子缓存作废。
    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
        self.forget_span_cache();
    }

    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// 换一组敲错纠正代价（回放调参用）；整句格子缓存与纠错缓存作废。
    pub fn set_typo_costs(&mut self, costs: TypoCosts) {
        self.typo_costs = costs;
        *self.correction_cache.borrow_mut() = None;
        self.forget_span_cache();
    }

    pub fn typo_costs(&self) -> TypoCosts {
        self.typo_costs
    }

    /// 整句转换与词级排序用的个人部分：学习器的个人 n-gram 配上当前插值参数。
    pub(super) fn personal(&self) -> Personal<'_> {
        Personal {
            ngram: self.learner.user_ngram(),
            interpolation: self.interpolation,
        }
    }

    /// 神经分的权重 λ（0 到 1）。
    pub fn set_neural_weight(&mut self, weight: f64) {
        self.neural_weight = weight.clamp(0.0, 1.0);
        self.forget_span_cache();
    }

    fn set_neural_parameters(
        &mut self,
        weight: Option<f64>,
        margin: Option<f64>,
        context: Option<usize>,
    ) {
        self.neural_weight = weight.unwrap_or(NEURAL_WEIGHT).clamp(0.0, 1.0);
        self.neural_margin = margin.unwrap_or(NEURAL_MARGIN).max(0.0);
        self.neural_context = context.unwrap_or(RESCORE_CONTEXT_CHARS);
        self.forget_span_cache();
    }

    pub fn with_language_model(mut self, model: Box<dyn LanguageModel>) -> Self {
        self.language_model = model;
        self
    }

    /// 静态语言模型（没接就是 [`NoLanguageModel`]）：评测工具拿它按 [`crate::sentence::segment_text`] 切汉字文本。
    pub fn language_model(&self) -> &dyn LanguageModel {
        &*self.language_model
    }

    pub fn history(&self) -> &InputHistory {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut InputHistory {
        &mut self.history
    }

    /// 进入 / 离开英文模式。英文模式下 [`Self::query`] 只给英文词表的候选，回车与空格仍由壳原样上屏敲的字母，
    /// 不发云联想，也不把原样上屏记成「不纠这个串」。
    pub fn set_english_mode(&mut self, on: bool) {
        self.english_mode = on;
    }

    pub fn english_mode(&self) -> bool {
        self.english_mode
    }

    pub fn with_english(mut self, words: WordList) -> Self {
        self.english = Some(words);
        self
    }

    pub fn with_translator(mut self, translator: Box<dyn Translator>) -> Self {
        self.translator = translator;
        self
    }

    /// 运行时换学习语言的释义表。
    /// 接英文候选用的释义表（英→中）。
    pub fn with_english_translator(mut self, translator: Box<dyn Translator>) -> Self {
        self.english_translator = translator;
        self
    }

    pub fn set_translator(&mut self, translator: Box<dyn Translator>) {
        self.translator = translator;
    }

    pub fn with_mode_keys(mut self, keys: ModeKeys) -> Self {
        self.modes = keys.sanitized();
        self
    }

    /// 非法组合（相同、或不是 v / u / i）整个退回缺省。
    pub fn set_mode_keys(&mut self, keys: ModeKeys) {
        self.modes = keys.sanitized();
    }

    pub fn mode_keys(&self) -> ModeKeys {
        self.modes
    }

    /// 中英混输里中文候选是否总排在英文词前面（配置 `[general] chinese_first`，缺省关）。
    /// 关着时拼音「不像话」的输入英文词排第一（`hello` 先英文再 荷兰咯）；开了英文词固定第二。
    pub fn set_chinese_first(&mut self, on: bool) {
        self.chinese_first = on;
    }

    pub fn chinese_first(&self) -> bool {
        self.chinese_first
    }

    /// 中文模式下 Shift+字母是否进组句缓冲区（配置 `[general] shift_letter`，缺省关）。
    /// 开着时大写按小写参与匹配、原样上屏时还原，`Cpan` 与 `cpan` 一样出「C盘」；
    /// 关着时壳直接把大写字母交给应用，进这里的字母就按它自己的样子匹配。
    pub fn set_shift_letter_compose(&mut self, on: bool) {
        self.shift_letter_compose = on;
    }

    pub fn shift_letter_compose(&self) -> bool {
        self.shift_letter_compose
    }

    pub fn with_learner(mut self, learner: Box<dyn Learner>) -> Self {
        self.learner.replace(learner);
        self.forget_span_cache();
        self
    }

    pub fn with_input_logger(mut self, logger: Box<dyn InputLogger>) -> Self {
        self.logger.replace(logger);
        self
    }

    /// 运行时换输入日志的落盘方（开关、清空之后）。旧的先 flush。
    pub fn set_input_logger(&mut self, logger: Box<dyn InputLogger>) {
        self.logger.flush();
        self.logger.replace(logger);
    }

    pub fn input_logger_mut(&mut self) -> &mut dyn InputLogger {
        self.logger.inner_mut()
    }

    pub fn with_usage_meter(mut self, meter: Box<dyn UsageMeter>) -> Self {
        self.meter = meter;
        self
    }

    /// 输入统计的汇总（偏好设置「统计」页）。
    pub fn usage_summary(&self) -> UsageSummary {
        self.meter.summary()
    }

    pub fn with_vocabulary_tracker(mut self, tracker: Box<dyn VocabularyTracker>) -> Self {
        self.vocabulary = tracker;
        self
    }

    pub fn with_gloss_filler(mut self, filler: Box<dyn GlossFiller>) -> Self {
        self.gloss_filler = filler;
        self
    }

    /// 运行时换释义兜底（随云联想开关）。
    pub fn set_gloss_filler(&mut self, filler: Box<dyn GlossFiller>) {
        self.gloss_filler = filler;
    }

    /// 汉字与相邻 ASCII 字母 / 数字之间补一个空格（`[general] mixed_space`）。
    pub fn set_mixed_space(&mut self, on: bool) {
        self.mixed_space = on;
    }

    /// 导入词库里「音节数与输入完全一致」的加分（`--tune exact-bonus=N`）。
    pub fn set_exact_bonus(&mut self, bonus: f64) {
        self.exact_bonus = bonus;
    }

    /// 设置「大而杂」的导入词库（用户在 `dicts/` 下的那些）。见 [`Engine::bulk_dictionaries`]。
    pub fn set_bulk_dictionaries(&mut self, dictionaries: Vec<Dictionary>) {
        self.bulk_dictionaries = dictionaries;
    }

    /// 词级查询要查哪些词库。第一键（作用域只有一个字符）跳过导入的大词库，
    /// 从第二个键起全都查。整句、词频统计那些地方仍用 [`Self::all_dictionaries`]。
    pub(super) fn lookup_dictionaries(&self) -> Vec<&Dictionary> {
        let first_key = self.composition.scope().chars().count() <= 1;
        let mut all = self.all_dictionaries();
        if first_key {
            all.retain(|dictionary| {
                !self
                    .bulk_dictionaries
                    .iter()
                    .any(|bulk| std::ptr::eq(*dictionary, bulk))
            });
        }
        all
    }

    /// 接上一个已上屏的词凑整词（`[general] join_previous_word`）：`村` 上屏后打 `ba` 出「BA」。
    pub fn set_join_previous_word(&mut self, on: bool) {
        self.join_previous_word = on;
    }

    /// 输入字母少于这个数时不给英文候选（`[general] english_min_letters`）：短输入（`o`、`en`）几乎都在打中文，
    /// 这个门槛把那些字母候选挡掉。只影响中文模式下的英文词，英文模式（Caps Lock）与英文尾段不受影响。
    pub fn set_english_min_letters(&mut self, letters: usize) {
        self.english_min_letters = letters.max(1);
    }

    /// 按 `[general] mixed_space` 决定上屏文本与上文之间要不要补空格：上一次上屏以汉字结尾、
    /// 这段以字母 / 数字开头（或反过来）时补一个，标点与空格两侧都不补。
    /// 学习、日志、译词都按原文走，只有插入的文本与撤销计数走这里。
    /// 刚才有字母是壳直接交给应用的（`Shift+G` 打 `Google` 时那个 `G` 走了直通）：缓冲区里剩的这段
    /// 是在打英文词，不是拼音。返回接上直通字母的整词（`Google`）。
    ///
    /// 为什么不能只看缓冲区：`oogle` 单看能解成 `o'guang'e`（像话），照「能解成拼音就不是英文」判会漏掉它。
    /// 直通进来的大写字母才是判据。整段本身就是个完整音节时（`G` + `hao`，多半是手滑按了 Shift）返回 `None`。
    pub(super) fn passthrough_english_word(&self, text: &str) -> Option<String> {
        let pending: String = self
            .passthrough_pending
            .chars()
            .filter(char::is_ascii_alphabetic)
            .collect();
        if pending.is_empty()
            || pending.len() != self.passthrough_pending.chars().count()
            || text.is_empty()
            || !text.bytes().all(|b| b.is_ascii_lowercase())
        {
            return None;
        }
        // 整段就是个完整音节（`G` + `hao`、`N` + `i`）：那是手滑按了 Shift，别当英文词。
        // 双拼下敲的是键，要按解出来的拼音判；全拼（`decode` 为 `None`）直接问语法表——
        // 早先只查 `decode`，全拼下这条保护整个失效，`Ni` 会被当成英文词
        let single_syllable = match self.decode(text) {
            Some(decoded) => decoded.is_complete() && !decoded.marked().contains('\''),
            None => crate::parser::is_syllable(text),
        };
        if single_syllable {
            return None;
        }
        Some(format!("{pending}{text}"))
    }

    /// 学一个英文词：把刚才直通的字母接上再学。
    ///
    /// 大写开头的自造词（`Winlane`）里 `W` 是 Shift+字母，壳直接交给应用、没进缓冲区，
    /// 不接上的话学到的是 `inlane`，下次打 `win` 补不出来。`pending` 由调用方给（`take_raw` 要先取，
    /// 因为 log_commit 会清空 passthrough_pending）；只有整段都是字母时才接。
    pub(super) fn learn_english_word(&mut self, text: &str, pending: &str) {
        let letters: String = pending.chars().filter(char::is_ascii_alphabetic).collect();
        let joinable = !letters.is_empty()
            && letters.len() == pending.chars().count()
            && text.bytes().all(|b| b.is_ascii_alphabetic());
        let word = if joinable {
            format!("{letters}{text}")
        } else {
            text.to_owned()
        };
        self.learner.learn_english(&word);
    }

    pub(super) fn glued(&self, text: &str) -> String {
        if !self.mixed_space {
            return text.to_owned();
        }
        let previous = self
            .recent_commits
            .last()
            .and_then(|commit| commit.text.chars().next_back());
        match (previous, text.chars().next()) {
            (Some(previous), Some(first)) if crate::mixed_space::needed(previous, first) => {
                format!(" {text}")
            }
            _ => text.to_owned(),
        }
    }

    /// 整句上屏：先按词缝补空格（词内部的 `B站` / `C盘` 不动），再按上文补前导那个空格。
    /// `word_chars` 是各词的字符数，见 [`crate::mixed_space::insert_at_seams`]。
    pub(super) fn glued_sentence(&self, text: &str, word_chars: &[usize]) -> String {
        if !self.mixed_space {
            return text.to_owned();
        }
        self.glued(&crate::mixed_space::insert_at_seams(text, word_chars))
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// 换掉全部附加词库（导入、移除、开关之后）。格子缓存随之作废。
    pub fn set_extra_dictionaries(&mut self, dictionaries: Vec<Dictionary>) {
        self.extra_dictionaries = dictionaries;
        self.forget_span_cache();
    }

    pub fn extra_dictionaries(&self) -> &[Dictionary] {
        &self.extra_dictionaries
    }

    /// 查词用的全部词库：主词库、附加词库、用户词。
    pub(super) fn all_dictionaries(&self) -> Vec<&Dictionary> {
        let mut all = Vec::with_capacity(self.extra_dictionaries.len() + 2);
        all.push(&self.dictionary);
        all.extend(self.extra_dictionaries.iter());
        // 导入的「大而杂」词库也算全部词库的一部分（词频归一化、整句词图都要用它们）；
        // 只有词级查询的第一键把它们剔掉，见 [`Self::lookup_dictionaries`]
        all.extend(self.bulk_dictionaries.iter());
        if let Some(user) = self.learner.user_words() {
            all.push(user);
        }
        all
    }

    /// 全部词库的词频之和，词频归一化成概率时用。
    pub(super) fn total_frequency(&self) -> u64 {
        self.all_dictionaries()
            .iter()
            .map(|d| d.total_frequency())
            .sum()
    }

    pub fn learner(&self) -> &dyn Learner {
        self.learner.inner()
    }

    /// 拿到可变的 Learner 就当它要改：格子缓存一起作废。
    pub fn learner_mut(&mut self) -> &mut dyn Learner {
        self.forget_span_cache();
        self.learner.inner_mut()
    }

    pub fn learning_language(&self) -> Language {
        self.translator.language()
    }
}
