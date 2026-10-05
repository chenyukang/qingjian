//! 整句候选：几条读法、什么时候给备选。

use super::*;

/// 三个音节的输入给备选整句（门槛 [`ALTERNATE_MIN_SYLLABLES`]）：三音节输入词表里往往只有第一音节的单字，
/// 末字换一种读法这时候比那些单字有用（`niuyuedui` → 纽约对 / 纽约队）。
#[test]
fn three_syllable_input_gets_sentence_alternates() {
    let dictionary =
        Dictionary::parse("纽约\tniu yue\t20000\n对\tdui\t9000\n队\tdui\t800\n").unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("niuyuedui");
    let items = engine.query().unwrap().candidates.items;
    let all: Vec<&str> = items.iter().map(|c| c.text.as_str()).collect();
    let sentence = |text: &str| {
        items
            .iter()
            .any(|c| c.text == text && c.kind == CandidateKind::Sentence)
    };
    assert!(sentence("纽约对"), "模型最优那条照常出：{all:?}");
    assert!(sentence("纽约队"), "末字的另一种读法也要给：{all:?}");
}

/// 两个音节不给备选整句：那时候词级候选比另一种读法有用。
#[test]
fn two_syllable_input_does_not_get_sentence_alternates() {
    let dictionary = Dictionary::parse("你\tni\t9000\n好\thao\t8000\n号\thao\t500\n").unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("nihao");
    let items = engine.query().unwrap().candidates.items;
    let all: Vec<&str> = items.iter().map(|c| c.text.as_str()).collect();
    assert!(
        items
            .iter()
            .any(|c| c.text == "你好" && c.kind == CandidateKind::Sentence),
        "{all:?}"
    );
    assert!(!all.contains(&"你号"), "两个音节不该给备选整句：{all:?}");
}
