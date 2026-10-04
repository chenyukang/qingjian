//! 中英之间的自动空格（`[general] mixed_space`）。

use super::*;

fn mixed(text: &str) -> Candidate {
    Candidate {
        text: text.into(),
        kind: CandidateKind::Chinese,
        syllables: Vec::new(),
        reading: None,
        translation: None,
        aux_code: None,
    }
}

fn mixed_engine() -> Engine {
    let mut engine = engine();
    engine.set_mixed_space(true);
    engine
}

#[test]
fn switched_off_leaves_text_alone() {
    let mut engine = engine();
    assert_eq!(engine.commit(&mixed("用Rust写的")), "用Rust写的");
}

#[test]
fn separate_commits_get_a_space_at_the_seam() {
    let mut engine = engine();
    engine.set_mixed_space(true);
    engine.set_input("kai");
    let chinese = engine.query().unwrap().candidates.items[0].clone();
    assert_eq!(chinese.text, "开");
    assert_eq!(engine.commit(&chinese), "开");
    // 中文之后接英文词：前面补一个空格
    assert_eq!(engine.commit(&mixed("Rust")), " Rust");
    // 英文之后接中文：也要补
    engine.set_input("kai");
    let chinese = engine.query().unwrap().candidates.items[0].clone();
    assert_eq!(engine.commit(&chinese), " 开");
}

#[test]
fn mixed_words_stay_in_one_piece() {
    let mut engine = engine();
    engine.set_mixed_space(true);
    // `B站` 这类成词的中英混排是一个词，内部不拆
    assert_eq!(engine.commit(&mixed("B站")), "B站");
    assert_eq!(engine.commit(&mixed("C盘")), " C盘");
}

#[test]
fn passthrough_after_chinese_asks_the_shell_for_one_space() {
    let mut engine = engine();
    engine.set_mixed_space(true);
    engine.set_input("kai");
    let chinese = engine.query().unwrap().candidates.items[0].clone();
    engine.commit(&chinese);
    assert!(engine.note_passthrough('3'), "汉字后面直通数字要空格");
    assert!(!engine.note_passthrough('2'), "数字后面接数字不再加");
}

#[test]
fn a_digit_after_han_asks_the_shell_for_a_space() {
    // `基本` 上屏后敲数字：数字没法组句，只能直通，所以前边的空格得靠
    // `note_passthrough` 告诉壳去插。壳对每个字符都是先 `punctuate` 再 `pass_through`，
    // 而 `punctuate` 原来会把「最近上屏的文本」清掉，于是这里就补不出空格了
    let mut engine = mixed_engine();
    assert_eq!(engine.commit(&mixed("基本")), "基本");
    // 壳问「9 转不转全角」——数字不转，但不能因此打断文本流
    assert_eq!(engine.punctuate('9'), None);
    assert!(engine.note_passthrough('9'), "数字前要插一个空格");
    // 数字后面那个空格由下一个词上屏时按接缝补
    assert_eq!(engine.commit(&mixed("点")), " 点");
    // 标点仍然算打断：`基本-` 之后不再补
    let mut engine = mixed_engine();
    assert_eq!(engine.commit(&mixed("基本")), "基本");
    assert_eq!(engine.punctuate('-'), None);
    assert!(!engine.note_passthrough('9'), "标点打断了文本流");
}
