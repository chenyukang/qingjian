//! 整句候选：几条读法、什么时候给备选。

use super::*;

/// 四个音节的输入给备选整句（门槛 [`ALTERNATE_MIN_SYLLABLES`]）：长句错一个字就得整句拆开重打。
#[test]
fn four_syllable_input_gets_sentence_alternates() {
    // 整段本身不是一个词，最优路径是两个词；另一条读法（开化书入）也要给
    let dictionary = Dictionary::parse(
        "开发\tkai fa\t20000\n输入\tshu ru\t8000\n开\tkai\t30000\n发\tfa\t9000\n\
         化\thua\t5000\n书\tshu\t7000\n入\tru\t4000\n",
    )
    .unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("kaifashuru");
    let items = engine.query().unwrap().candidates.items;
    let sentences: Vec<&str> = items
        .iter()
        .filter(|c| c.kind == CandidateKind::Sentence)
        .map(|c| c.text.as_str())
        .collect();
    let all: Vec<&str> = items.iter().map(|c| c.text.as_str()).collect();
    assert!(sentences.contains(&"开发输入"), "模型最优那条：{all:?}");
    assert!(sentences.len() >= 2, "四个音节要给备选整句：{all:?}");
}

/// 短输入本来不给备选整句，但末词是用户自己打过的接续时例外：他打过的搭配要能再出现
/// （两步打出 `纽约` + `队` 之后，整串 `niuyuedui` 就该能选出 纽约队）。
#[test]
fn short_input_keeps_the_alternate_the_user_typed_before() {
    let dictionary =
        Dictionary::parse("纽约\tniu yue\t20000\n对\tdui\t9000\n队\tdui\t800\n").unwrap();
    // 要真记 n-gram，得用带假学习器的引擎（默认那个只认词表）
    let mut engine = Engine::new(dictionary).with_learner(Box::new(WordLearner::default()));
    // 用户真实的打法：打 `niuyuedui`，先选「纽约」（只吃掉前两个音节，`dui` 留在手上），再选「队」。
    // 两次选择在同一段拼音里，个人二元记下 纽约 → 队
    engine.set_input("niuyuedui");
    let niuyue = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "纽约")
        .unwrap();
    engine.commit(&niuyue);
    let dui = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "队")
        .unwrap();
    engine.commit(&dui);

    engine.set_input("niuyuedui");
    let items = engine.query().unwrap().candidates.items;
    let all: Vec<&str> = items.iter().map(|c| c.text.as_str()).collect();
    assert!(
        items.iter().any(|c| c.text == "纽约队"),
        "自己打过「纽约+队」，整串输入就该能选出它：{all:?}"
    );
}

/// 三个音节不给备选整句：三音节输入词表里往往只有第一音节的单字，那几格留给词级候选。
#[test]
fn three_syllable_input_does_not_get_sentence_alternates() {
    let dictionary =
        Dictionary::parse("纽约\tniu yue\t20000\n对\tdui\t9000\n队\tdui\t800\n").unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("niuyuedui");
    let items = engine.query().unwrap().candidates.items;
    let all: Vec<&str> = items.iter().map(|c| c.text.as_str()).collect();
    assert!(
        items
            .iter()
            .any(|c| c.text == "纽约对" && c.kind == CandidateKind::Sentence),
        "{all:?}"
    );
    assert!(!all.contains(&"纽约队"), "三个音节不该给备选整句：{all:?}");
}
