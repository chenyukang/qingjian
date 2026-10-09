//! 查询模式：中文照常组句，候选给的是「这个意思英文怎么说」的英文词。

use super::*;

#[test]
fn a_single_character_candidate_is_never_picked_automatically() {
    // 输入「kai」时头一个候选是单字「开」：不自动挑（查「开」只会得到语法/义项说明），
    // 留在第一段给中文候选
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kai");
    let query = engine.query().unwrap();
    assert!(engine.lookup_source().is_none());
    assert!(
        query
            .candidates
            .items
            .iter()
            .all(|c| c.kind != CandidateKind::English)
    );
}

#[test]
fn a_short_word_covering_the_whole_input_jumps_straight_to_english() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kaifa");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "develop");
}

#[test]
fn a_partial_tail_stops_the_automatic_pick() {
    // 「我的」只覆盖整段拼音的一半（后面还有个没打完的 l）：不自动挑，让用户先看中文
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kaifa'k");
    assert!(engine.lookup_source().is_none());
}

#[test]
fn a_word_without_a_local_gloss_stays_in_the_first_phase() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kafei");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "咖啡");
    assert!(engine.lookup_source().is_none());
}

#[test]
fn escape_lets_the_user_pick_the_chinese_again() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kaifa");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "develop");
    engine.clear_lookup_source();
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "开发");
}

#[test]
fn lookup_mode_turns_chinese_candidates_into_english_words() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_lookup_source("开发");
    engine.set_input("kaifa");
    let query = engine.query().unwrap();
    let items = &query.candidates.items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].text, "develop");
    assert_eq!(items[0].kind, CandidateKind::English);
    assert_eq!(items[0].syllables, vec!["kai".to_owned(), "fa".to_owned()]);
}

#[test]
fn lookup_candidates_keep_the_source_word_as_the_explanation() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_lookup_source("开发");
    engine.set_input("kaifa");
    let mut query = engine.query().unwrap();
    engine.annotate(&mut query.candidates);
    let translation = query.candidates.items[0].translation.as_ref().unwrap();
    assert_eq!(translation.senses()[0].text, "开发");
    assert_eq!(
        translation.senses()[0].part_of_speech,
        Some(PartOfSpeech::Verb)
    );
}

#[test]
fn chinese_candidates_come_back_when_lookup_mode_is_off() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_input("kaifa");
    let query = engine.query().unwrap();
    assert!(
        query
            .candidates
            .items
            .iter()
            .all(|c| c.kind != CandidateKind::English)
    );
    assert!(query.candidates.items.iter().any(|c| c.text == "开发"));
}

#[test]
fn turning_the_mode_off_restores_chinese_candidates() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kaifa");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "develop");
    engine.set_lookup_mode(false);
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "开发");
}

#[test]
fn clearing_the_source_goes_back_to_the_first_phase() {
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kaifa");
    engine.set_lookup_source("开发");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "develop");
    engine.clear_lookup_source();
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "开发");
}

#[test]
fn lookup_works_even_when_the_display_translator_is_absent() {
    let mut engine = engine().with_lookup_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_lookup_source("开发");
    engine.set_input("kaifa");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "develop");
}

#[test]
fn typing_on_makes_the_previous_choice_expire() {
    // 挑中「开发」看英文之后继续打字（拼音变了）：旧的选择作废，不再拿它挡着新输入
    let mut engine = engine().with_translator(Box::new(FixedTranslator));
    engine.set_lookup_mode(true);
    engine.set_input("kaifa");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "develop");
    engine.set_input("kaifan");
    // 作废发生在下一次查询时（拼音在这时才知道变了）
    let query = engine.query().unwrap();
    assert!(engine.lookup_source().is_none());
    assert!(
        query
            .candidates
            .items
            .iter()
            .all(|c| c.kind != CandidateKind::English)
    );
}
