//! 逐字模式（`⌃⇧Z`）：候选只留只吃一个音节的（单字与单音节词）。
//!
//! 用户的场景：拼音还没上屏，这次就想一个字一个字挑，而 `bini` 下 比你 / 比尼亚德尔马
//! 这类多字词把单字挤到十几页之后。这个模式是**显式**切换 —— 不猜意图，不改默认排序。

use super::*;

fn query(engine: &mut Engine, input: &str) -> Vec<Candidate> {
    engine.set_input(input);
    engine.query().unwrap().candidates.items
}

#[test]
fn keeps_only_one_syllable_candidates() {
    // 常态：多字词（开发）与单字（开）混在一起，多字词在单字前面（覆盖率优先）
    let mut zone = super::engine();
    let all = query(&mut zone, "kaifa");
    let all_texts: Vec<&str> = all.iter().map(|c| c.text.as_str()).collect();
    assert!(all_texts.contains(&"开发"), "{all_texts:?}");
    assert!(all_texts.contains(&"开"), "{all_texts:?}");
    let first_word = all_texts.iter().position(|t| *t == "开发");
    let first_char = all_texts.iter().position(|t| *t == "开");
    assert!(
        first_word < first_char,
        "多字词本来就在单字前面：{all_texts:?}"
    );

    // 逐字：只剩只吃一个音节的候选，单字就在最前
    let mut zone = super::engine();
    zone.set_word_by_word(true);
    let words = query(&mut zone, "kaifa");
    let texts: Vec<&str> = words.iter().map(|c| c.text.as_str()).collect();
    assert!(!texts.is_empty(), "应当还有单字候选");
    assert!(
        words.iter().all(|c| c.syllables.len() == 1),
        "只留只吃一个音节的：{texts:?}"
    );
    assert!(texts.contains(&"开"), "{texts:?}");
    assert!(!texts.contains(&"开发"), "多字词让位：{texts:?}");
    assert_eq!(texts[0], "开", "单字排到最前：{texts:?}");
}

#[test]
fn exits_when_no_single_syllable_candidate() {
    // 词库里只有双音节词：逐字模式一个候选都留不下，应当自动退出，而不是给一个空窗
    let mut zone = Engine::new(Dictionary::parse("开发\tkai fa\t9000\n").unwrap());
    zone.set_word_by_word(true);
    let _ = query(&mut zone, "kaifa");
    assert!(!zone.word_by_word(), "没有单音节候选时自动退出");
    assert!(
        zone.query()
            .unwrap()
            .candidates
            .items
            .iter()
            .any(|c| c.text == "开发")
    );
}

#[test]
fn stays_on_while_there_is_still_pinyin_left() {
    // 你选一个字一个字挑：选完第一个字，剩下的拼音还要继续挑，模式不能自己关掉
    let mut zone = super::engine();
    zone.set_word_by_word(true);
    zone.set_input("kaifa");
    let first = zone
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "开" && c.kind == CandidateKind::Chinese)
        .expect("逐字模式下第一屏就有 开");
    assert_eq!(zone.commit(&first), "开");
    assert!(zone.word_by_word(), "还剩 fa 没挑，模式留着");
    assert!(!zone.composition().is_empty(), "拼音还剩一部分");
}

#[test]
fn clearing_the_composition_leaves_the_mode() {
    // 「这一句我要一个字一个字挑」——拼音上屏 / 清空后回到常态
    let mut zone = super::engine();
    zone.set_word_by_word(true);
    let _ = query(&mut zone, "kaifa");
    assert!(zone.word_by_word());
    zone.clear();
    assert!(!zone.word_by_word());
}

#[test]
fn keeps_single_syllable_candidates_of_a_long_input() {
    // 用户的原始例子：多字词被挡掉之后，同音单字仍然挑得到
    let mut zone = super::engine();
    zone.set_word_by_word(true);
    let words = query(&mut zone, "kaifazhe");
    let texts: Vec<&str> = words.iter().map(|c| c.text.as_str()).collect();
    assert!(words.iter().all(|c| c.syllables.len() == 1), "{texts:?}");
    assert!(texts.contains(&"开"), "{texts:?}");
}
