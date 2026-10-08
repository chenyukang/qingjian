//! 文本差异：算出纠错结果里哪几段和原文不同，给弹窗里上色用（用户要一眼看出改了哪）。
//!
//! 只处理「同一段文字的小改」这种场景：按比较单位求最长公共子序列，落不进公共序列的单位就是改动。
//! 单位是：连续的西文/数字算一个词、连续空白算一段、汉字与标点各自一个 —— 汉字逐字比较（改一个字
//! 只标那个字），英文按词比较（少一个 `s` 标整词，比只标一个字母看得清）。

/// 比较单位的合并上限：单位数之积超过它就不算了（几千字的选区远到不了）。
const MAX_DIFF_CELLS: usize = 4_000_000;

/// 西文词里的字符（`'` 与 `-` 算词内：`don't`、`tx-pool` 各是一个词）。
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '\'' || c == '’' || c == '-'
}

/// 把文本切成比较单位，返回（单位, 每个单位的字符区间）。
fn units(text: &str) -> (Vec<String>, Vec<(usize, usize)>) {
    let chars: Vec<char> = text.chars().collect();
    let mut units = Vec::new();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if is_word_char(chars[i]) {
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
        } else if chars[i].is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
        } else {
            i += 1;
        }
        units.push(chars[start..i].iter().collect());
        spans.push((start, i));
    }
    (units, spans)
}

/// `changed` 里相对 `original` 改动的字符区间：左闭右开、按字符下标、相邻的已合并。
///
/// 两段完全一样返回空；一段为空时返回另一段的全部；差异太大（远长于选区上限）时不标。
pub fn changed_ranges(original: &str, changed: &str) -> Vec<(usize, usize)> {
    if original == changed {
        return Vec::new();
    }
    let (a, _) = units(original);
    let (b, spans) = units(changed);
    if a.is_empty() || b.is_empty() {
        return spans.last().map_or(Vec::new(), |(_, end)| vec![(0, *end)]);
    }
    if a.len().saturating_mul(b.len()) > MAX_DIFF_CELLS {
        tracing::debug!(left = a.len(), right = b.len(), "文本太长，不标改动");
        return Vec::new();
    }

    // dp[i][j] = a[i..] 与 b[j..] 的最长公共子序列长度
    let mut dp = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    // 回溯：b 里没被匹配上的单位就是改动
    let mut matched = vec![false; b.len()];
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            matched[j] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    // 相邻的改动合成一段
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (index, hit) in matched.iter().enumerate() {
        if *hit {
            continue;
        }
        let (start, end) = spans[index];
        match ranges.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_marks() {
        assert!(changed_ranges("今天天气不错", "今天天气不错").is_empty());
        assert!(changed_ranges("", "").is_empty());
    }

    #[test]
    fn marks_the_chinese_character_that_changed() {
        // 我今天很开心 → 我今天很伤心：只有第四个字（开→伤）改了
        assert_eq!(changed_ranges("我今天很开心", "我今天很伤心"), vec![(4, 5)]);
        // 一个字换成两个字：两个都标
        assert_eq!(changed_ranges("我今天很开心", "我今天很高兴"), vec![(4, 6)]);
    }

    #[test]
    fn marks_an_inserted_comma() {
        assert_eq!(changed_ranges("hello world", "hello, world"), vec![(5, 6)]);
    }

    #[test]
    fn marks_a_space_inserted_between_scripts() {
        // 提示词要求中英之间补空格，补上的那个空格要高亮出来
        assert_eq!(changed_ranges("你say", "你 say"), vec![(1, 2)]);
    }

    #[test]
    fn marks_a_whole_latin_word_when_letters_change() {
        assert_eq!(
            changed_ranges("he has two cat", "he has two cats"),
            vec![(11, 15)]
        );
        // 拼错的词整词标出来
        assert_eq!(changed_ranges("I recieve it", "I receive it"), vec![(2, 9)]);
    }

    #[test]
    fn marks_everything_when_nothing_shared() {
        assert_eq!(changed_ranges("abc", "xyz"), vec![(0, 3)]);
        assert_eq!(changed_ranges("", "xyz"), vec![(0, 3)]);
    }

    #[test]
    fn marks_multiple_separate_edits() {
        // 两处独立的改动不会合成一段（今→明、开→伤）
        assert_eq!(
            changed_ranges("我今天很开心", "我明天很伤心"),
            vec![(1, 2), (4, 5)]
        );
    }
}
