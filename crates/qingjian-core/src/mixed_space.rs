//! 汉字与相邻 ASCII 字母 / 数字之间的自动空格（配置 `[general] mixed_space`，缺省关）。
//!
//! 只在**词与词之间**补：一个候选词内部（`B站`、`C盘`、`T恤`）不动，`hello, world`、`https://x.com`、
//! `C++` 这类字母数字彼此相邻的地方也不动。平台层不做这个变换，只把 Core 返回的文本原样插入。

/// `before` 后面紧跟 `after` 时要不要插一个空格。
pub fn needed(before: char, after: char) -> bool {
    (is_han(before) && is_latin(after)) || (is_latin(before) && is_han(after))
}

/// 按词切分给整段文本补缝上的空格。`word_chars` 是每个词的字符数：
/// 词数不足两个、或总数与 `text` 的字符数对不上（繁体转换之类改过长度）时原样返回，宁可不补也不错位。
pub fn insert_at_seams(text: &str, word_chars: &[usize]) -> String {
    if word_chars.len() < 2 || word_chars.iter().sum::<usize>() != text.chars().count() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + word_chars.len());
    let mut chars = text.chars();
    let mut previous: Option<char> = None;
    for (index, count) in word_chars.iter().enumerate() {
        for position in 0..*count {
            let Some(c) = chars.next() else { break };
            if index > 0 && position == 0 && previous.is_some_and(|previous| needed(previous, c)) {
                out.push(' ');
            }
            out.push(c);
            previous = Some(c);
        }
    }
    out.extend(chars);
    out
}

/// 汉字：基本区、扩展 A、兼容区与扩展 B 之后的高位平面。
fn is_han(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x2fa1f
    )
}

/// 需要与汉字隔开的字符：ASCII 字母与数字。标点、空格、emoji 都不算。
fn is_latin(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::{insert_at_seams, needed};

    #[test]
    fn boundary_is_one_sided() {
        assert!(needed('我', 'a'));
        assert!(needed('9', '章'));
        assert!(!needed('a', 'b'));
        assert!(!needed('我', '，'));
        assert!(!needed(' ', 'a'));
    }

    #[test]
    fn seams_between_words_get_a_space() {
        // 我 / 在 / B站：只在 在↔B 这道缝补，词内部的 B站 不动
        assert_eq!(insert_at_seams("我在B站", &[1, 1, 2]), "我在 B站");
        // 我 / 用 / Rust / 写的
        assert_eq!(
            insert_at_seams("我用Rust写的", &[1, 1, 4, 2]),
            "我用 Rust 写的"
        );
        // 汉字之间、字母数字之间都不动
        assert_eq!(insert_at_seams("我们去吧", &[1, 1, 1, 1]), "我们去吧");
        assert_eq!(insert_at_seams("Rustlang", &[4, 4]), "Rustlang");
    }

    #[test]
    fn mismatched_split_is_left_alone() {
        // 字符数对不上（繁体转换过、或切分不是字符级）时不动，宁可不补也不能错位
        assert_eq!(insert_at_seams("我用Rust", &[1, 9]), "我用Rust");
        assert_eq!(insert_at_seams("一个字", &[3]), "一个字");
    }
}
