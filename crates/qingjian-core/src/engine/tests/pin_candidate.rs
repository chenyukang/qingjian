//! 置顶候选（`⇧+数字`）：只改排序偏好，**不动学习数据**。

use std::sync::{Arc, Mutex};

use super::*;

/// 把每次 `forget` 记下来，并当作「删掉了一个用户词」回报 —— 用来验证置顶不会走到那条路。
struct UserWordLearner {
    forgotten: Arc<Mutex<Vec<String>>>,
    sort: HashMap<String, SortPreference>,
}

impl Learner for UserWordLearner {
    fn record(&mut self, _candidate: &Candidate) {}

    fn weight(&self, _text: &str) -> u32 {
        0
    }

    fn forget(&mut self, text: &str) -> Forgotten {
        self.forgotten.lock().unwrap().push(text.to_owned());
        Forgotten {
            user_word: true,
            ..Forgotten::default()
        }
    }

    fn toggle_sort_preference(&mut self, text: &str, target: SortPreference) -> SortPreference {
        let next = self
            .sort
            .get(text)
            .copied()
            .unwrap_or_default()
            .toggled(target);
        self.sort.insert(text.to_owned(), next);
        next
    }

    fn sort_preference(&self, text: &str) -> SortPreference {
        self.sort.get(text).copied().unwrap_or_default()
    }
}

fn learner() -> (UserWordLearner, Arc<Mutex<Vec<String>>>) {
    let forgotten = Arc::new(Mutex::new(Vec::new()));
    (
        UserWordLearner {
            forgotten: forgotten.clone(),
            sort: HashMap::new(),
        },
        forgotten,
    )
}

fn user_candidate(text: &str) -> Candidate {
    Candidate {
        text: text.to_owned(),
        kind: CandidateKind::Chinese,
        syllables: vec!["yi".to_owned(), "lin".to_owned()],
        reading: None,
        translation: None,
        aux_code: None,
    }
}

#[test]
fn pinning_a_user_word_does_not_delete_it() {
    let (learner, forgotten) = learner();
    let mut engine = engine().with_learner(Box::new(learner));
    let result = engine.pin(&user_candidate("轶琳"));
    assert!(
        forgotten.lock().unwrap().is_empty(),
        "置顶只是「排前面」，不该删用户词：{:?}",
        forgotten.lock().unwrap()
    );
    assert!(!result.user_word);
    assert_eq!(result.preference, Some(SortPreference::Top));
}

#[test]
fn hiding_a_user_word_still_deletes_it() {
    // 隐藏的本意就是「不要了」，对用户词等于删掉 —— 这条老行为不变
    let (learner, forgotten) = learner();
    let mut engine = engine().with_learner(Box::new(learner));
    let result = engine.hide(&user_candidate("轶琳"));
    assert_eq!(forgotten.lock().unwrap().as_slice(), ["轶琳"]);
    assert!(result.user_word);
}
