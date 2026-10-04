//! 词级查找：每个位置展开成多种写法，再按这些写法查主词库与用户词。

use super::*;

impl Engine {
    /// 每个位置的写法：敲的原样、模糊音，再加音节级敲错变体（`correction::typo`）当带代价的边，
    /// 代价按类别定、按个人敲错表打折。太短的输入（不到 [`correction::MIN_LETTERS`]）、双拼、非末尾带简拼的切分不加敲错变体：
    /// 短串一处编辑几乎总能凑出别的词，双拼敲错一键换掉的是整个声母 / 韵母。不完整的位置（简拼、前缀）本来就按前缀查，不加。
    pub(in crate::engine) fn expand_positions(
        &self,
        patterns: &[qingjian_dictionary::SyllablePattern<'_>],
        typos: bool,
    ) -> Expanded {
        let mut expanded = self.fuzzy.expand(patterns);
        if !typos {
            return expanded;
        }
        let letters: usize = patterns.iter().map(|p| p.text.len()).sum();
        // 非末尾有简拼 / 残缺音节的切分（`kai f a`）本来就不是用户敲的原话，不在它上面再猜敲错
        let inner_abbreviated = patterns
            .iter()
            .take(patterns.len().saturating_sub(1))
            .any(|p| !p.complete);
        if self.shuangpin.is_some() || letters < correction::MIN_LETTERS || inner_abbreviated {
            return expanded;
        }
        for (index, pattern) in patterns.iter().enumerate() {
            if !pattern.complete {
                continue;
            }
            for (text, kind) in typo::variants(pattern.text) {
                let accepted = self.learner.typo_count(pattern.text, text);
                expanded.push_alternative(index, text, self.typo_costs.typo_cost(*kind, accepted));
            }
        }
        expanded
    }

    /// 本地整句转换把最优切分转成的汉字，给云端当参考（问字模式里就是问题的汉字形式）；转不出或有占位音节为空。
    pub(in crate::engine) fn local_guess(&self, segmentations: &[Segmentation]) -> String {
        segmentations
            .first()
            .and_then(|best| self.convert_sentence(&best.patterns(), true))
            .filter(|conversion| !conversion.has_placeholder())
            .map(|conversion| conversion.text)
            .unwrap_or_default()
    }

    /// 主词库与用户词一起查（每个位置多种写法）。用户词是用户自己选过的（云联想接受的词等），排序上靠 weight 自然靠前。
    ///
    /// 每个命中带一个「可信来源」标记：主词库与用户词为真，导入的附加词库（CEDICT、雾凇那类）为假。
    /// 排序里只有可信来源的「音节数完全一致」才当硬键——冷僻的导入词不该顶掉用户自己的词和常用短词。
    pub(in crate::engine) fn lookup_all(
        &self,
        positions: &[Vec<qingjian_dictionary::SyllablePattern<'_>>],
    ) -> Vec<(Match<'_>, bool)> {
        let mut hits = Vec::new();
        for dictionary in self.lookup_dictionaries() {
            let trusted = self.trusted_dictionary(dictionary);
            hits.extend(
                dictionary
                    .lookup_pattern_alt(positions)
                    .into_iter()
                    .map(|hit| (hit, trusted)),
            );
        }
        hits
    }

    /// 只要音节数正好等于位置数的词，主词库与用户词一起查。可信标记同 [`Self::lookup_all`]。
    pub(in crate::engine) fn lookup_exact_all(
        &self,
        positions: &[Vec<qingjian_dictionary::SyllablePattern<'_>>],
    ) -> Vec<(Match<'_>, bool)> {
        let mut hits = Vec::new();
        for dictionary in self.lookup_dictionaries() {
            let trusted = self.trusted_dictionary(dictionary);
            hits.extend(
                dictionary
                    .lookup_exact_alt(positions)
                    .into_iter()
                    .map(|hit| (hit, trusted)),
            );
        }
        hits
    }

    /// 这本词库算不算「可信来源」：主词库与用户词库。
    /// 其余都是导入的附加词库——词表大、冷僻词多，只让它们的精确命中拿一个加分而不是硬键。
    fn trusted_dictionary(&self, dictionary: &Dictionary) -> bool {
        std::ptr::eq(dictionary, &self.dictionary)
            || self
                .learner
                .user_words()
                .is_some_and(|user| std::ptr::eq(dictionary, user))
    }
}
