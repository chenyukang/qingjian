//! 查词、切分、光标与上屏消耗。

use super::*;

#[test]
fn exact_word_first_then_longer_then_prefix_expansions_then_prefix_words() {
    assert_eq!(texts("kaifa"), ["开发", "开放", "开饭", "开发者", "开"]);
}

#[test]
fn partial_last_syllable_expands() {
    assert_eq!(texts("kaif"), ["开放", "开发", "开饭", "开发者", "开"]);
}

#[test]
fn initials_match_abbreviated_words_and_commit_consumes_letters() {
    let all = texts("kf");
    assert_eq!(all[0], "开放"); // 同为简拼命中，按词频
    assert!(all.contains(&"开发".to_owned()) && all.contains(&"咖啡".to_owned()));

    let mut engine = engine();
    engine.set_input("kfzhe");
    let kaifa = engine
        .query()
        .unwrap()
        .candidates
        .items
        .iter()
        .find(|c| c.text == "开发")
        .unwrap()
        .clone();
    engine.commit(&kaifa);
    assert_eq!(engine.composition().text(), "zhe");
}

#[test]
fn unparsable_tail_is_kept_aside() {
    let mut engine = engine();
    // kaiv 唯一的纠法是删掉刚敲的 v，那不算纠错：v 留作尾巴等下一键（kaifv 会被纠成 kaifa）
    engine.set_input("kaiv");
    let query = engine.query().unwrap();
    assert!(query.correction.is_none());
    assert_eq!(query.tail, "v");
    assert_eq!(query.marked_text(), "kai'v");
    assert_eq!(query.candidates.items[0].text, "开");

    let kaifa = query
        .candidates
        .items
        .iter()
        .find(|c| c.text == "开发")
        .unwrap()
        .clone();
    engine.commit(&kaifa);
    assert_eq!(engine.composition().text(), "v");
    // 剩下的 v 进表达式模式：没候选但也不报错
    assert!(engine.query().unwrap().candidates.items.is_empty());
}

#[test]
fn cursor_edits_requery_from_the_start_and_map_into_marked_text() {
    let mut engine = engine();
    engine.set_input("kaifa");
    engine.move_cursor_left();
    engine.move_cursor_left();
    let query = engine.query().unwrap();
    assert_eq!(query.marked_text(), "kai'fa");
    assert_eq!(query.marked_cursor(), 3); // kai|'fa

    engine.push('n');
    let query = engine.query().unwrap();
    assert_eq!(query.text, "kainfa");
    assert_eq!(query.marked_cursor(), 5); // kai'n|'fa

    engine.set_input("xi'an");
    engine.move_cursor_left();
    engine.move_cursor_left();
    let query = engine.query().unwrap();
    assert_eq!(query.marked_text(), "xi'an");
    assert_eq!(query.marked_cursor(), 3); // xi'|an，紧跟用户自己敲的 '
}

#[test]
fn punctuation_follows_committed_text() {
    let mut engine = engine();
    assert_eq!(engine.punctuate(','), Some("，"));
    engine.note_passthrough('3');
    assert_eq!(engine.punctuate('.'), None);
    engine.set_input("kaifa");
    let kaifa = engine.query().unwrap().candidates.items[0].clone();
    engine.commit(&kaifa);
    assert_eq!(engine.punctuate('.'), Some("。"));
}

#[test]
fn marked_text_joins_best_segmentation_with_apostrophes() {
    let mut engine = engine();
    engine.set_input("kaifa");
    assert_eq!(engine.query().unwrap().marked_text(), "kai'fa");
    engine.set_input("kf");
    assert_eq!(engine.query().unwrap().marked_text(), "k'f");
}

#[test]
fn prefix_words_appear_after_full_matches() {
    // 开放 / 开饭 来自 `kai f… a zhe` 这种切分的前缀 `kai f…`，排在覆盖更多字母的 开发 之后
    let all = texts("kaifazhe");
    assert_eq!(&all[..2], ["开发者", "开发"]);
    assert_eq!(all.last().map(String::as_str), Some("开"));
}

#[test]
fn ambiguous_segmentation_merges_results() {
    let all = texts("xian");
    assert_eq!(all[0], "先");
    assert!(all.contains(&"西安".to_owned()));
}

#[test]
fn complete_syllable_that_is_also_prefix_expands_after_exact() {
    // 下 是完整命中；先 / 想 来自 xia → xian / xiang 的前缀扩展；西安 来自 xi + a… 的次级切分
    assert_eq!(texts("xia"), ["下", "先", "想", "西安"]);
}

#[test]
fn keyboard_u_umlaut_spelling_matches_canonical_dictionary_keys() {
    let dictionary =
        Dictionary::parse("策略\tce lve\t9000\n虐待\tnve dai\t8000\n学习\txue xi\t7000\n").unwrap();
    let mut engine = Engine::new(dictionary);

    engine.set_input("celue");
    let query = engine.query().unwrap();
    assert_eq!(query.marked_text(), "ce'lue");
    assert_eq!(query.candidates.items[0].text, "策略");
    assert_eq!(query.candidates.items[0].syllables, ["ce", "lve"]);
    let strategy = query.candidates.items[0].clone();
    assert_eq!(engine.commit(&strategy), "策略");
    assert!(engine.composition().is_empty());

    engine.set_input("nuedai");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "虐待");

    engine.set_input("xuexicelue");
    let sentence = &engine.query().unwrap().candidates.items[0];
    assert_eq!(sentence.text, "学习策略");
    assert_eq!(sentence.kind, CandidateKind::Sentence);
}

#[test]
fn empty_input_is_an_error() {
    assert_eq!(engine().query().unwrap_err(), ParseError::Empty);
}

#[test]
fn commit_consumes_only_the_candidate_syllables() {
    let mut engine = engine();
    engine.set_input("kaifazhe");
    let kaifa = engine
        .query()
        .unwrap()
        .candidates
        .items
        .iter()
        .find(|c| c.text == "开发")
        .unwrap()
        .clone();
    assert_eq!(engine.commit(&kaifa), "开发");
    assert_eq!(engine.composition().text(), "zhe");

    engine.set_input("kaif");
    assert_eq!(engine.commit(&kaifa), "开发");
    assert!(engine.composition().is_empty());

    engine.set_input("xi'an");
    let xian = engine
        .query()
        .unwrap()
        .candidates
        .items
        .iter()
        .find(|c| c.text == "西安")
        .unwrap()
        .clone();
    engine.commit(&xian);
    assert!(engine.composition().is_empty());
}

#[test]
fn take_raw_returns_pinyin_and_clears() {
    let mut engine = engine();
    engine.set_input("kaifa");
    assert_eq!(engine.take_raw(), "kaifa");
    assert!(engine.composition().is_empty());
}

#[test]
fn choice_key_strips_separators_and_clamps() {
    assert_eq!(choice_key("kai'fa", 6), "kaifa");
    assert_eq!(choice_key("kai'fa", 3), "kai");
    assert_eq!(choice_key("ba", 10), "ba");
}

#[test]
fn sentence_conversion_leads_when_input_spans_several_words() {
    // 想开发：SAMPLE 里没有整词，最优路径是 想 + 开发
    let all = texts("xiangkaifa");
    assert_eq!(all[0], "想开发");
    let mut engine = engine();
    engine.set_input("xiangkaifa");
    let query = engine.query().unwrap();
    let sentence = query.candidates.items[0].clone();
    assert_eq!(sentence.kind, CandidateKind::Sentence);
    assert_eq!(sentence.syllables, ["xiang", "kai", "fa"]);
    assert_eq!(engine.commit(&sentence), "想开发");
    assert!(engine.composition().is_empty());

    // 整段本身是一个词：不出整句
    let all = texts("kaifa");
    assert_eq!(all[0], "开发");
    assert_eq!(all.iter().filter(|t| *t == "开发").count(), 1);

    // 末尾只有一个字母时整句不算它：xiangkaif → 想开
    let all = texts("xiangkaif");
    assert_eq!(all[0], "想开");
}

#[test]
fn shortcuts_follow_the_first_local_candidate() {
    let all = texts("rq");
    let shortcut = all.iter().position(|t| t.ends_with('日')).unwrap();
    assert!(shortcut <= 1);
    assert!(all.iter().any(|t| t.contains('-')));

    let mut engine = engine();
    engine.set_input("xq");
    let query = engine.query().unwrap();
    let weekday = query
        .candidates
        .items
        .iter()
        .find(|c| c.kind == CandidateKind::Shortcut)
        .unwrap()
        .clone();
    assert!(weekday.text.starts_with("星期"));
    engine.commit(&weekday);
    assert!(engine.composition().is_empty());
}

#[test]
fn shift_letters_join_the_buffer_only_when_configured() {
    let dictionary = Dictionary::parse("C盘\tc pan\t8000\n磁盘\tci pan\t249\n").unwrap();
    let mut engine = Engine::new(dictionary);
    let type_cpan = |engine: &mut Engine| {
        // 中文模式下按住 Shift 敲 C，再打 pan
        engine.push('C');
        for c in "pan".chars() {
            engine.push(c);
        }
    };

    // 缺省 `shift_letter = "passthrough"`：壳直接把大写字母交给应用，这一路本来就不会走到；
    // 万一走到也不该被当成拼音去匹配（所以「C盘」出不来）
    type_cpan(&mut engine);
    assert_ne!(engine.query().unwrap().candidates.items[0].text, "C盘");
    engine.clear();

    // 开了 compose：按小写参与匹配，拼音行按敲的样子显示，回车原样上屏时保留大写
    engine.set_shift_letter_compose(true);
    type_cpan(&mut engine);
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "C盘");
    assert_eq!(query.marked_text(), "C'pan");
    assert_eq!(engine.take_raw(), "Cpan");
    assert!(engine.composition().is_empty());
}

#[test]
fn a_fuzzy_hit_that_the_user_picked_counts_for_this_input() {
    // 双拼下 `cc` 解成 `cao`：「香草」是原样命中，「湘超」（xiang chao）靠模糊音命中。
    // 用户在这个输入串下亲手选过一次「湘超」，下次就该它排第一——不能再被「原样命中优先」压着，
    // 也不能因为命中词的读音比输入串多一个字母（xiangchao vs xiangcao）就把选择次数读成 0
    let rules = FuzzyRules {
        c_ch: true,
        ..FuzzyRules::default()
    };
    let dictionary = Dictionary::parse("香草\txiang cao\t9000\n湘超\txiang chao\t100\n").unwrap();
    let mut engine = Engine::new(dictionary)
        .with_learner(Box::new(CountingLearner(HashMap::new())))
        .with_fuzzy(rules);
    engine.set_shuangpin(Some(Scheme::Xiaohe));

    engine.set_input("xlcc");
    let first = engine.query().unwrap().candidates.items[0].text.clone();
    assert_eq!(first, "香草");

    let target = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "湘超")
        .expect("模糊音下也出湘超");
    engine.commit(&target);

    engine.set_input("xlcc");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "湘超");
}

#[test]
fn a_sentence_the_converter_did_not_rank_first_still_teaches() {
    // 用户选的整句常常不是 Viterbi 最优那条（「我喜欢湘超」输给「我喜欢想超」那种）。
    // 回推词序列时只比对最优路径的文本，等于「选了模型不偏爱的路径就什么都不记」——
    // 那条路径的个人 bigram 一直不涨，下次还是排后面。这里 开发 是最优路径，用户要 开阀
    let dictionary = Dictionary::parse("开发\tkai fa\t9000\n开阀\tkai fa\t1000\n").unwrap();
    let shared = Arc::new(Mutex::new((
        Vec::new(),
        crate::sentence::UserNgram::default(),
    )));
    let learner = WordLearner {
        shared: Arc::clone(&shared),
        ..WordLearner::default()
    };
    let mut engine = Engine::new(dictionary).with_learner(Box::new(learner));

    engine.set_input("kaifa");
    let best = engine.query().unwrap().candidates.items[0].text.clone();
    assert_eq!(best, "开发", "最优路径");

    let chosen = Candidate {
        text: "开阀".to_owned(),
        kind: CandidateKind::Sentence,
        syllables: vec!["kai".to_owned(), "fa".to_owned()],
        reading: None,
        translation: None,
        aux_code: None,
    };
    engine.commit(&chosen);
    let ngram = &shared.lock().unwrap().1;
    assert_eq!(
        ngram.pair(None, "开阀"),
        1,
        "整句上屏也要把词序列记进个人 n-gram"
    );
}

#[test]
fn a_han_latin_word_can_be_coined_by_repeating_it() {
    // `湘` + 尾巴原样上屏 `BA`：汉字+字母的自造词要能进用户词库。
    // 原来两道门都关着：原样上屏不记「接在上一个词后面」，自动造词又要求「字数 == 音节数」
    //（`湘ba` 1 汉字 2 音节）
    let shared = Arc::new(Mutex::new((
        Vec::new(),
        crate::sentence::UserNgram::default(),
    )));
    let learner = WordLearner {
        shared: Arc::clone(&shared),
        ..WordLearner::default()
    };
    let dictionary = Dictionary::parse("湘\txiang\t20000\n吧\tba\t3000\n").unwrap();
    let mut engine = Engine::new(dictionary).with_learner(Box::new(learner));

    for _ in 0..2 {
        engine.set_input("xiangba");
        let xiang = engine
            .query()
            .unwrap()
            .candidates
            .items
            .into_iter()
            .find(|c| c.text == "湘")
            .expect("湘");
        engine.commit(&xiang);
        assert_eq!(engine.take_raw(), "ba", "尾巴原样上屏");
    }

    engine.set_input("xiangba");
    let texts: Vec<String> = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert!(
        texts.iter().any(|text| text == "湘ba"),
        "自造词要出来：{texts:?}"
    );
}

#[test]
fn a_word_can_span_the_previous_commit() {
    // `村` 上屏后打 `ba`：词库里的 村BA 要凑得出来。整词候选只在当前这段拼音里查，
    // 已经上屏的「村」不在缓冲区里，靠把上一个词的音节接到切分前面才查得到
    let dictionary =
        Dictionary::parse("村\tcun\t20000\n村BA\tcun ba\t9000\n吧\tba\t3000\n").unwrap();
    let mut engine = Engine::new(dictionary);

    // 开关关着（缺省）：接不上
    let mut off = Engine::new(
        Dictionary::parse("村\tcun\t20000\n村BA\tcun ba\t9000\n吧\tba\t3000\n").unwrap(),
    );
    off.set_input("cun");
    let village = off
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "村")
        .expect("村");
    off.commit(&village);
    off.set_input("ba");
    assert!(
        off.query()
            .unwrap()
            .candidates
            .items
            .iter()
            .all(|c| c.text != "BA"),
        "缺省不跨上屏凑词"
    );

    engine.set_join_previous_word(true);
    engine.set_input("cun");
    let village = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "村")
        .expect("村");
    engine.commit(&village);
    engine.set_input("ba");
    let items = engine.query().unwrap().candidates.items;
    // 候选只放尾巴：「村」已经在文档里了
    assert_eq!(
        (items[0].text.as_str(), items[0].syllables.as_slice()),
        ("BA", &["ba".to_owned()][..])
    );
    assert_eq!(engine.commit(&items[0].clone()), "BA");
}

#[test]
fn shifted_keys_keep_the_capital_in_the_preedit() {
    // 双拼 + `shuangpin_raw_preedit`：行内拼音就是敲的键，大写得看得见（`DJu`）。
    // 匹配仍按小写算（`dan` + `sh`），所以候选照旧是「但是」；回车原样上屏也带大写
    let mut engine = Engine::new(Dictionary::parse("但是\tdan shi\t9000\n").unwrap());
    engine.set_shuangpin(Some(Scheme::Xiaohe));
    engine.set_shuangpin_raw_preedit(true);
    engine.set_shift_letter_compose(true);
    for c in "DJu".chars() {
        engine.push(c);
    }
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "但是");
    assert_eq!(query.marked_text(), "DJu");
    assert_eq!(
        query
            .marked_segments()
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        ["DJu"]
    );
    assert_eq!(engine.take_raw(), "DJu");
}

#[test]
fn expression_mode_skips_pinyin_and_evaluates() {
    let mut engine = self::engine();
    assert!(!engine.expression_mode());
    engine.set_input("v1+2");
    assert!(engine.expression_mode());
    let query = engine.query().unwrap();
    assert_eq!(query.marked_text(), "v1+2");
    assert_eq!(query.marked_cursor(), 4);
    assert_eq!(query.candidates.items[0].text, "3");
    assert_eq!(query.candidates.items[0].kind, CandidateKind::Shortcut);
    assert_eq!(query.candidates.items[1].text, "1+2=3");
    let result = query.candidates.items[0].clone();
    assert_eq!(engine.commit(&result), "3");
    assert!(engine.composition().is_empty());

    // 只有 v：候选为空但不报错，preedit 照显示
    engine.set_input("v");
    let query = engine.query().unwrap();
    assert!(query.candidates.items.is_empty());
    assert_eq!(query.marked_text(), "v");

    // v 开头的英文词仍能混输
    let words = WordList::parse("very\tvery\t4800\n").unwrap();
    let mut engine = self::engine().with_english(words);
    engine.set_input("very");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "very");
    assert_eq!(query.candidates.items[0].kind, CandidateKind::English);
}

/// 只认句首的 开发 与 开发 → 先 的假模型：让两词路径压过整段的词。
struct XianModel;

impl LanguageModel for XianModel {
    fn log_prob(&self, previous: Option<&str>, word: &str) -> Option<f64> {
        match (previous, word) {
            (None, "开发") => Some(-1.0),
            (Some("开发"), "先") => Some(-0.5),
            _ => None,
        }
    }
}

#[test]
fn a_word_spelling_the_sentence_keeps_its_rank_unless_its_reading_differs() {
    // 词级排序里 开发线 在 开发先 前面；整句转换读出的是 开发 + 先，与词 开发先 同文本同读音：不重复插，词留在原位
    let sample = format!("{SAMPLE}开发线\tkai fa xian\t5000\n开发先\tkai fa xian\t1\n");
    let mut engine =
        Engine::new(Dictionary::parse(&sample).unwrap()).with_language_model(Box::new(XianModel));
    engine.set_input("kaifaxian");
    let all = texts_of(&engine);
    assert_eq!(&all[..2], ["开发线", "开发先"]);
    assert_eq!(all.iter().filter(|t| *t == "开发先").count(), 1);

    // 同文本的词是按别的读音（xiang，靠模糊音 an-ang 对上）收的：那条错读音的词让位，整句以正确读音排最前
    let sample = format!("{SAMPLE}开发线\tkai fa xian\t5000\n开发先\tkai fa xiang\t1\n");
    let mut engine =
        Engine::new(Dictionary::parse(&sample).unwrap()).with_language_model(Box::new(XianModel));
    engine.set_fuzzy(FuzzyRules {
        an_ang: true,
        ..FuzzyRules::default()
    });
    engine.set_input("kaifaxian");
    let items = engine.query().unwrap().candidates.items;
    assert_eq!(items[0].text, "开发先");
    assert_eq!(items[0].kind, CandidateKind::Sentence);
    assert_eq!(items[0].syllables, ["kai", "fa", "xian"]);
    assert_eq!(items.iter().filter(|c| c.text == "开发先").count(), 1);
}

#[test]
fn option_backspace_deletes_a_syllable_and_command_backspace_deletes_to_the_start() {
    let mut engine = engine();
    // 全拼：删最优切分的最后一个音节；`'` 连同前面的音节一起删；切不动的尾巴整个删
    engine.set_input("kaifaxian");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kaifa");
    engine.set_input("kai'fa'");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kai'");
    engine.set_input("kaifv");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kaif");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kai");
    // 光标停在中间：只动光标前的
    engine.set_input("kaifaxian");
    for _ in 0..4 {
        engine.move_cursor_left();
    }
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kaixian");
    assert_eq!(engine.composition().cursor(), 3);
    assert!(engine.delete_to_start());
    assert_eq!(engine.composition().text(), "xian");
    assert_eq!(engine.composition().cursor(), 0);
    assert!(!engine.delete_syllable_backward());
    assert!(!engine.delete_to_start());
    // 英文直输段：字母一段一段删，标点一次一个
    engine.set_input("hello,world");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "hello,");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "hello");
    // 双拼：两键一音节，落单的一键单删
    let mut engine = xiaohe();
    engine.set_input("kdfah");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kdfa");
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "kd");
}

#[test]
fn option_arrows_move_the_cursor_by_syllable() {
    let mut engine = engine();
    // 光标在末尾发现前面错了：⌥← 两下跳到第二个音节后面，⌥⌫ 删掉它，重敲，⌘→ 回末尾
    engine.set_input("kaifa'xian'xia");
    assert!(engine.move_cursor_syllable_left());
    assert_eq!(engine.composition().cursor(), "kaifa'xian'".len());
    assert!(engine.move_cursor_syllable_left());
    assert_eq!(engine.composition().cursor(), "kaifa'".len());
    assert!(engine.move_cursor_syllable_left());
    assert_eq!(engine.composition().cursor(), "kai".len());
    assert!(engine.delete_syllable_backward());
    assert_eq!(engine.composition().text(), "fa'xian'xia");
    engine.push('x');
    engine.push('i');
    assert_eq!(engine.composition().text(), "xifa'xian'xia");
    // ⌥→：从 xi| 起跳过一个音节到 xifa|，再跳先越过 `'` 再过一个音节
    assert!(engine.move_cursor_syllable_right());
    assert_eq!(engine.composition().cursor(), "xifa".len());
    assert!(engine.move_cursor_syllable_right());
    assert_eq!(engine.composition().cursor(), "xifa'xian".len());
    assert!(engine.move_cursor_syllable_right());
    assert_eq!(engine.composition().cursor(), "xifa'xian'xia".len());
    assert!(!engine.move_cursor_syllable_right());
    engine.move_cursor_home();
    assert!(!engine.move_cursor_syllable_left());
    // 直输段按字母段跳
    engine.set_input("hello,world");
    assert!(engine.move_cursor_syllable_left());
    assert_eq!(engine.composition().cursor(), "hello,".len());
    engine.move_cursor_home();
    assert!(engine.move_cursor_syllable_right());
    assert_eq!(engine.composition().cursor(), "hello".len());
}
