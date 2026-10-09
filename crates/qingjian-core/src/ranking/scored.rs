use std::cmp::Reverse;

use qingjian_dictionary::Match;

use crate::engine::SortPreference;

/// 一条待排序的词库命中。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scored<'a> {
    /// 词库命中，含词、拼音、词频与是否精确。
    pub hit: Match<'a>,

    /// 输入的最后一个音节是完整的，且与词的对应音节相同（而非前缀扩展）。
    pub full_last: bool,

    /// 词覆盖了输入开头多少个字母（不含 `'`）。按字母而不是音节算，不同切分之间才可比：
    /// `xian` 的 先（1 音节）和 西安（2 音节）覆盖同样 4 个字母。前缀词（`kaifazhe` → 开发）覆盖得少。
    pub coverage: usize,

    /// 切分里非末尾的简拼音节数（`kai f a` 是 1，`kai fa` 是 0）：越少越像用户敲的原话，
    /// 否则 `kaifa` 会因 `kai f a` 这种切法让 开放啊 排到 开放 前面。
    pub abbreviated: usize,

    /// 用户选择过的次数，来自 Learner。
    pub weight: u32,

    /// 这条「音节数与输入完全一致」的命中能不能用硬键：可信来源（主词库 / 用户词库），
    /// 或者用户亲手选过这个词（`weight > 0`）。导入词库里从没被选过的冷僻词降一档，
    /// 只拿得分里的加分，见 [`super::rank`]。
    pub hard_exact: bool,

    /// 靠模糊音 / 敲错变体命中的代价（某个音节与敲的不同；0 是敲的原样）：从上下文得分里扣掉，
    /// 预选时词频按 e^(-代价) 打折，同分排在原样命中后面。
    pub penalty: f64,
}

/// 预选键：结构项之后按用户选择次数与词库词频，命中太多时先用它砍到够排的量（不必算上下文得分）。
/// 打包成一个整数、越大越靠前：单字母简拼一键命中几万条，逐条比元组太慢。
/// 位从高到低：精确(1) 覆盖字母数(8) 简拼数的补(8) 末音节完整(1) 选择次数(32) 打折后词频(32) 原样命中(1) 字数的补(8)；
/// 与 [`SortKey`] 的前几项同序，只是不拿文本做最后的平手项（预选边界上的平手谁留下无所谓）。
pub type PreselectKey = u128;

/// 排序键，越小越靠前。元组的顺序即排序规则，见模块文档；**头一项是用户设的排序偏好**
/// （0 正常 / 1 后置，见 [`crate::engine::SortPreference`]）：后置的词无论字频多高、命中多精确，
/// 都排在没后置的候选之后 —— 但仍在候选里，翻页能找到。第二项是**可信来源**的
/// 「音节数与输入完全一致」，第三项是同输入串下的选择次数，第四项是原样命中，
/// 第五项是上下文得分（毫分，整数才能比较）。文本借自词库，键可以脱离 `Scored` 存放。
pub type SortKey<'a> = (
    i8,
    Reverse<bool>,
    Reverse<usize>,
    usize,
    Reverse<u32>,
    Reverse<bool>,
    Reverse<i64>,
    bool,
    usize,
    &'a str,
);

impl<'a> Scored<'a> {
    /// 命中不是敲的原样（模糊音或敲错变体）。
    pub fn altered(&self) -> bool {
        self.penalty > 0.0
    }

    pub(super) fn preselect_key(&self) -> PreselectKey {
        let frequency = if self.altered() {
            (f64::from(self.hit.frequency) * (-self.penalty).exp()) as u32
        } else {
            self.hit.frequency
        };
        let chars = u128::from(u8::try_from(self.hit.text.chars().count()).unwrap_or(u8::MAX));
        let coverage = u128::from(u8::try_from(self.coverage).unwrap_or(u8::MAX));
        let abbreviated = u128::from(u8::try_from(self.abbreviated).unwrap_or(u8::MAX));
        (u128::from(self.hard_exact) << 90)
            | (coverage << 82)
            | ((0xFF - abbreviated) << 74)
            | (u128::from(self.full_last) << 73)
            | (u128::from(self.weight) << 41)
            | (u128::from(frequency) << 9)
            | (u128::from(!self.altered()) << 8)
            | (0xFF - chars)
    }

    /// 单字候选：`yu` 下 于/雨/语/与 一串同音字，选过一次 雨 就把它顶到 于 前面太狠了
    ///（两者的字频差能有十几倍）。单字不吃「同输入串下选过」这条硬键，选择次数改在得分里加分
    ///（封顶，见 `rank` 里那条）慢慢往前挪；多字词照旧——那个输入串基本就一个词，选它就是选它。
    pub(crate) fn is_single_char(&self) -> bool {
        self.hit.text.chars().count() == 1
    }

    /// `choice` 是同输入串下的选择次数，`score` 是上下文得分（log 概率，已含用户加分与模糊音 / 敲错扣分）。
    pub(super) fn key(&self, choice: u32, score: f64, preference: SortPreference) -> SortKey<'a> {
        let hard_choice = if self.is_single_char() { 0 } else { choice };
        (
            preference.rank(),
            Reverse(self.hard_exact),
            Reverse(self.coverage),
            self.abbreviated,
            // 选择次数排在「原样命中」之前：用户在同一个输入串下亲手选过的词，比
            // 「哪个是原样命中、哪个是模糊音命中」更能说明他要哪个
            //（`xiangcao` 下「湘超」是模糊音命中，排在「香草」后面，选多少次都翻不过来；
            // 翻过来的唯一途径是总选择加分，那个封顶 20 次）
            Reverse(hard_choice),
            Reverse(self.full_last),
            Reverse((score * 1000.0).round() as i64),
            self.altered(),
            self.hit.text.chars().count(),
            self.hit.text,
        )
    }
}
