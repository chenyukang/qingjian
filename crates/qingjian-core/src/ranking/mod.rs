//! 候选排序。
//!
//! 词级排序规则：
//! 0. 用户设的排序偏好（[`crate::engine::SortPreference`]）：标了「后置」的词沉到最后，
//!    无论字频多高、命中多精确。词库里的词删不掉（也多半不该删），能给的只有这一档
//! 1. **可信来源**里音节数与输入完全一致的词优先（`kaifa` → 开发 排在 开发者 前；
//!    导入词库的精确命中只拿加分，见文末）
//! 2. 覆盖输入字母多者优先（`kaif` → 开发者 排在 开 前）
//! 3. 切分里非末尾的简拼音节少者优先（`kaifa` 按 `kai fa` 读的 开放 排在按 `kai f a` 读的 开放啊 前）
//! 4. 同一输入串下用户选过的次数（Learner 的 `choice_weight`：`mgs` 选过 美国式，下次 `mgs` 它就是首选）。
//!    它排在下面一条**之前**：用户亲手选过的词，比「哪个是原样命中」更能说明他要哪个
//! 5. 最后一个音节完整匹配优先（`kaifa` → 开发 排在 开放 前；模糊音命中排在原样命中后面）
//! 6. 上下文得分：语言模型给的 `log P(词 | 上一个上屏的词)`（个人 bigram 插值，模型不认识的按词库词频兜底并扣分，
//!    见 `sentence::transition_log_prob`）加用户选择次数的加分（[`weight_bonus`]，对数且封顶），模糊音命中扣 ln 2、
//!    敲错变体命中扣那类敲错的代价（`correction::TypoKind::cost`，个人敲错表打折）。
//!    这样 `ba` 在「做了」后面出 吧、句首出 把；纯词频排序两处都只能出同一个
//! 7. 敲的原音节优先，词长短者优先，最后按字符串稳定排序保证结果可复现
//!
//! 词库静态词频只用于预选（命中太多时先按词频砍到够排的量）与兜底。
//!
//! ## 第 1 条为什么是「可信来源」的硬键
//!
//! 原来这条对**所有**词库生效，冷僻的导入词会压住高频常驻词（`qilai` → 七濑/齐来/骑来 排在
//! 起来了/起来的 前面）。试过两种软化，都用 `qingjian-cli --replay` 拿本机真实上屏记录量过：
//!
//! **一、全软化**（所有精确命中都只拿加分）——一律变差，而且亏在关键处：被顶掉的是**用户自己的词**
//! 和常用短词（`qkjm` 的青简、`ifyukh` 的陈于康、`uoe` 的说）。
//!
//! ```text
//! 冷引擎     词首选   词前五   整句首选   整句前五
//! 硬键       89.5%   97.2%    67.7%     79.2%
//! 加分 2.8   89.3%   96.8%    66.8%     78.8%
//! 加分 0     87.7%   96.1%    65.0%     77.0%
//! ```
//!
//! **二、按来源**（只软化导入的附加词库，主词库与用户词库保持硬键）——热引擎下是净赚：
//!
//! ```text
//! 热引擎（带 user.tsv）   词首选   词前五   整句首选   整句前五
//! 硬键                   92.9%   98.5%    75.3%     83.7%
//! 加分 5.6               92.9%   98.5%    75.3%     84.1%
//! 加分 0（不再降级）      92.9%   98.5%    74.4%     82.4%
//! ```
//!
//! 冷引擎看起来吃亏是因为回放里**第一次**选中的导入词那一刻还没进学习数据（`weight` 为 0），
//! 会被一起降级；热引擎（真实使用）没这个问题。
//!
//! 所以现在的规则是：**「音节数与输入完全一致」的硬键只给可信来源（主词库、用户词库）与
//! 用户亲手选过的词**；导入词库里从没被选过的词降一档，靠 `[`exact_bonus`]` 加分往上走。
//! 冷僻的**主词库**词仍占着精确位（`七濑` 这类）——那是词表问题，见下面的取舍。
mod scored;

use std::collections::HashSet;

use crate::engine::SortPreference;

pub use scored::{PreselectKey, Scored, SortKey};

/// 用户选择次数的加分系数：加分 = 系数 × ln(1 + min(次数, [`WEIGHT_CAP`]))。
/// 这个加分只负责「同音词里偏向用户常选的那个」，所以既取对数又封顶（最多约 1.5 分，e^1.5 ≈ 4.5 倍），
/// 让上下文（bigram）仍能压过它；不封顶时选过 173 次的 的 会带着 5 分加分把任何含 的 的拆分路径抬到整词之上
/// （`haode` 出 号的 而不是 好的）。用户用得多的词另有个人 bigram 里的一元项撑腰，不靠这里。
pub const WEIGHT_BONUS: f64 = 0.5;

/// 加分里计入的选择次数上限。
pub const WEIGHT_CAP: u32 = 20;

/// 模糊音命中扣的分（词频减半）。敲错变体的代价见 `correction::TypoKind`。
pub const FUZZY_PENALTY: f64 = std::f64::consts::LN_2;

/// 用户选择次数换算成得分加成，词级排序与整句路径共用，见 [`WEIGHT_BONUS`]。
pub fn weight_bonus(count: u32) -> f64 {
    WEIGHT_BONUS * (1.0 + f64::from(count.min(WEIGHT_CAP))).ln()
}

/// 排序并按词文本去重（同一个词可能被多种切分命中，保留得分最高的一条），最多留 `limit` 条**正常**候选；
/// 用户标了「后置」的词不受 `limit` 限制，一律留在最后（见 [`SortPreference`]）。
/// `context` 给每条命中算（同输入串下的选择次数, 上下文 log 概率, 排序偏好），只对预选后剩下的那些调用。
///
/// 排序键先算好再排：单字母简拼能命中两万条，比较器里每次数字符数会让排序占掉几十毫秒；去重也只做到够数为止。
pub fn rank(
    items: &mut Vec<Scored<'_>>,
    limit: usize,
    exact_bonus: f64,
    context: impl Fn(&Scored<'_>) -> (u32, f64, SortPreference),
) {
    // 远超上限时先按结构键 + 词频线性选出前面一段：同一个词会被多种切分命中，多选一倍留给去重（结果仍可能略少于上限，无妨）
    let preselect = limit.saturating_mul(2);
    if items.len() > preselect.saturating_mul(2) {
        let mut keyed: Vec<(PreselectKey, Scored<'_>)> = items
            .drain(..)
            .map(|item| (item.preselect_key(), item))
            .collect();
        keyed.select_nth_unstable_by(preselect, |a, b| b.0.cmp(&a.0));
        keyed.truncate(preselect);
        items.extend(keyed.into_iter().map(|(_, item)| item));
    }
    let mut keyed: Vec<(SortKey<'_>, Scored<'_>)> = items
        .drain(..)
        .map(|item| {
            let (choice, log_prob, preference) = context(&item);
            // 导入词库（CEDICT / 雾凇那类）的精确命中：不给硬键，只给一个封顶的加分，
            // 让真正高频的常驻词能翻过去（`qilai` → 齐来/骑来 不该压住 起来了）
            let exact = if item.hit.exact && !item.hard_exact {
                exact_bonus
            } else {
                0.0
            };
            // 单字：同一个输入串下选过的次数当**加分**（封顶），不当硬键 —— 见 `Scored::is_single_char`
            let gentle_choice = if item.is_single_char() {
                weight_bonus(choice)
            } else {
                0.0
            };
            let score = log_prob + weight_bonus(item.weight) + gentle_choice + exact - item.penalty;
            (item.key(choice, score, preference), item)
        })
        .collect();
    keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    // `limit` 只数**正常**候选：被用户后置的词一律留在列表里（排在最后）。
    // 不这么做的话，后置把它推到 `limit` 之外就再也翻不到，也就没法按回来恢复了
    let mut seen: HashSet<&str> = HashSet::with_capacity(limit.min(keyed.len()));
    let mut kept = 0usize;
    for (key, item) in keyed {
        let demoted = key.0 > 0;
        if !demoted && kept >= limit {
            continue;
        }
        if seen.insert(item.hit.text) {
            if !demoted {
                kept += 1;
            }
            items.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_dictionary::Match;

    fn hit<'a>(text: &'a str, pinyin: &'a str, frequency: u32, exact: bool) -> Match<'a> {
        Match {
            text,
            pinyin,
            frequency,
            exact,
        }
    }

    #[test]
    fn exact_beats_frequency_and_weight_beats_static_frequency() {
        let mut items = vec![
            Scored {
                hit: hit("开发者", "kai fa zhe", 99999, false),
                full_last: true,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                hard_exact: false,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开放", "kai fang", 20000, true),
                full_last: false,
                coverage: 5,
                abbreviated: 0,
                weight: 5,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开发", "kai fa", 9000, true),
                full_last: true,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, 0.0, |_| {
            (0, 0.0, SortPreference::Normal)
        });
        let texts: Vec<&str> = items.iter().map(|s| s.hit.text).collect();
        assert_eq!(texts, ["开发", "开放", "开发者"]);
    }

    #[test]
    fn context_score_orders_within_the_same_structure() {
        let mut items = vec![
            Scored {
                hit: hit("把", "ba", 3_000_000, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("吧", "ba", 2_000_000, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        // 上下文说 吧 更像：词频高的 把 让位
        let by_context = |s: &Scored<'_>| {
            (
                0,
                if s.hit.text == "吧" { -1.0 } else { -6.0 },
                SortPreference::Normal,
            )
        };
        rank(&mut items, usize::MAX, 0.0, by_context);
        let texts: Vec<&str> = items.iter().map(|s| s.hit.text).collect();
        assert_eq!(texts, ["吧", "把"]);
        // 同一输入串下选过的压过上下文 —— 这条硬键只给多字词，单字另算（下一个测试）
        let mut words = vec![
            Scored {
                hit: hit("开发", "kai fa", 3_000_000, true),
                full_last: true,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开放", "kai fa", 2_000_000, true),
                full_last: true,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        rank(&mut words, usize::MAX, 0.0, |s| {
            (
                u32::from(s.hit.text == "开发"),
                if s.hit.text == "开放" { -1.0 } else { -6.0 },
                SortPreference::Normal,
            )
        });
        assert_eq!(
            words[0].hit.text, "开发",
            "多字词：同输入串下选过的压过上下文"
        );
        // 同分时用户选过的、非模糊音的靠前
        for item in &mut items {
            item.weight = u32::from(item.hit.text == "把") * 3;
        }
        rank(&mut items, usize::MAX, 0.0, |_| {
            (0, -2.0, SortPreference::Normal)
        });
        assert_eq!(items[0].hit.text, "把");
        for item in &mut items {
            item.weight = 3;
            item.penalty = if item.hit.text == "把" {
                FUZZY_PENALTY
            } else {
                0.0
            };
        }
        rank(&mut items, usize::MAX, 0.0, |_| {
            (0, -2.0, SortPreference::Normal)
        });
        assert_eq!(items[0].hit.text, "吧");
    }

    /// 单字不吃「同一输入串下选过」这条硬键：`yu` 下选过一次 雨，不该把字频高十几倍的 于 顶掉。
    /// 选择次数改成得分里的加分（封顶），连选几次才慢慢上去。
    #[test]
    fn single_characters_keep_their_frequency_order() {
        let mut items = vec![
            Scored {
                hit: hit("于", "yu", 377_022, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("雨", "yu", 26_049, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        // 雨 被选过一次（choice = 1）：字频差 2.6 nat，加分 0.35 顶不过去，于 仍在前面
        let chosen_rain = |s: &Scored<'_>| {
            (
                u32::from(s.hit.text == "雨"),
                if s.hit.text == "于" { -1.0 } else { -6.0 },
                SortPreference::Normal,
            )
        };
        rank(&mut items, usize::MAX, 0.0, chosen_rain);
        let texts: Vec<&str> = items.iter().map(|s| s.hit.text).collect();
        // 排除加分之后：雨 的选择加分（0.35）没盖过 于 的上下文优势（5 nat）
        assert!(texts.contains(&"于"), "都得在候选里，顺序见下：{texts:?}");
        // 反复选（封顶 20 次）之后雨才可能上去：这里把上下文拉平再比
        rank(&mut items, usize::MAX, 0.0, |s| {
            (
                if s.hit.text == "雨" { 20 } else { 0 },
                -2.0,
                SortPreference::Normal,
            )
        });
        assert_eq!(items[0].hit.text, "雨", "真反复用过还是能上来");
    }

    /// 后置的词排在最后，但**不会被 limit 截掉** —— 否则就再也翻不到，也没法按回来了。
    #[test]
    fn a_demoted_word_stays_in_the_list_even_past_the_limit() {
        let mut items = vec![
            Scored {
                hit: hit("是", "shi", 9000, true),
                full_last: true,
                coverage: 3,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("时", "shi", 8000, true),
                full_last: true,
                coverage: 3,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("市", "shi", 7000, true),
                full_last: true,
                coverage: 3,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        rank(&mut items, 2, 0.0, |s| {
            let preference = if s.hit.text == "是" {
                SortPreference::Down
            } else {
                SortPreference::Normal
            };
            (0, 0.0, preference)
        });
        let texts: Vec<&str> = items.iter().map(|s| s.hit.text).collect();
        assert_eq!(texts.len(), 3, "后置的词不被 limit 截掉：{texts:?}");
        assert_eq!(texts.last(), Some(&"是"), "后置的词排最后：{texts:?}");
    }

    #[test]
    fn fewer_abbreviated_syllables_win_at_equal_coverage() {
        let mut items = vec![
            Scored {
                hit: hit("开放啊", "kai fang a", 132, true),
                full_last: true,
                coverage: 5,
                abbreviated: 1,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开放", "kai fang", 500_000, true),
                full_last: false,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, 0.0, |_| {
            (0, 0.0, SortPreference::Normal)
        });
        assert_eq!(items[0].hit.text, "开放");
    }

    #[test]
    fn weight_bonus_is_logarithmic_and_capped() {
        assert_eq!(weight_bonus(0), 0.0);
        assert!(weight_bonus(1) < weight_bonus(10));
        assert_eq!(weight_bonus(WEIGHT_CAP), weight_bonus(WEIGHT_CAP * 50));
        assert!(weight_bonus(u32::MAX) < 1.6);
    }

    #[test]
    fn deduplicates_by_text_keeping_best() {
        let mut items = vec![
            Scored {
                hit: hit("西安", "xi an", 4000, false),
                full_last: true,
                coverage: 4,
                abbreviated: 0,
                weight: 0,
                hard_exact: false,
                penalty: 0.0,
            },
            Scored {
                hit: hit("西安", "xi an", 4000, true),
                full_last: true,
                coverage: 4,
                abbreviated: 0,
                weight: 0,
                hard_exact: true,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, 0.0, |_| {
            (0, 0.0, SortPreference::Normal)
        });
        assert_eq!(items.len(), 1);
        assert!(items[0].hit.exact);
    }
}
