//! 候选文字的折行：翻译 / 纠错的结果可能上千字，一行铺开会把窗口拉成几千像素的长条。
//!
//! 纯逻辑，宽度由调用方给（位图渲染器走 cosmic-text，AppKit 退路走 NSFont），所以可以单测。
//! 两个渲染路径共用同一份折行规则，窗口宽度与内容布局才不会一个折一个不折。

/// 折行的宽度上限（点）：13pt 文字大约 90 个西文字符或 45 个汉字一行。
pub const MAX_TEXT_WIDTH: f32 = 640.0;

/// 最多折几行：再长以省略号收尾。翻译 / 纠错的结果上屏时才生效，窗口里只是预览，
/// 几十行的窗口既放不下也没人看。
pub const MAX_TEXT_LINES: usize = 6;

/// 折出来的一行：文本，以及它在原文里的字符区间（左闭右开，已去掉首尾空白）。
/// 区间是给「改动用别的颜色标出来」用的：标记按原文下标给，折行后要能映射回每一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub range: (usize, usize),
    pub text: String,
}

/// 按宽度把 `text` 折成若干行。
///
/// - 显式的 `\n` 断行；
/// - 拉丁文优先在空白处断，一个词比整行还宽才逐字断；汉字逐字断；
/// - 超过 `max_lines` 行时以 `…` 收尾。
pub fn wrap_text(
    text: &str,
    mut width_of: impl FnMut(&str) -> f32,
    max_width: f32,
    max_lines: usize,
) -> Vec<Line> {
    let max_lines = max_lines.max(1);
    let mut lines: Vec<Line> = Vec::new();
    let mut truncated = false;
    for segment in text.split('\n') {
        if lines.len() >= max_lines {
            truncated = true;
            break;
        }
        let (mut part, more) =
            wrap_segment(segment, &mut width_of, max_width, max_lines - lines.len());
        lines.append(&mut part);
        truncated |= more;
    }
    if lines.is_empty() {
        lines.push(Line {
            range: (0, 0),
            text: String::new(),
        });
    }
    if truncated && let Some(last) = lines.last_mut() {
        // 省略号不是原文里的字，不影响区间
        last.text.push('…');
    }
    lines
}

/// 把一行按改动区间切成若干段：`(是否改动, 文本)`。`marks` 是原文下标，`line.range` 负责把
/// 行里的第几个字映射回原文。相邻同类合并，省得逐字画。
pub fn split_by_marks(line: &Line, marks: &[(usize, usize)]) -> Vec<(bool, String)> {
    let mut segments: Vec<(bool, String)> = Vec::new();
    for (index, ch) in line.text.chars().enumerate() {
        let at = line.range.0 + index;
        let marked = marks.iter().any(|&(start, end)| at >= start && at < end);
        match segments.last_mut() {
            Some((kind, text)) if *kind == marked => text.push(ch),
            _ => segments.push((marked, ch.to_string())),
        }
    }
    segments
}

/// 切出 `chars[start..end]` 的展示文本与它在原文里的字符区间（去掉首尾空白）。
fn trimmed_line(chars: &[char], start: usize, end: usize) -> ((usize, usize), String) {
    let raw: String = chars[start..end].iter().collect();
    let leading = raw.chars().take_while(|c| c.is_whitespace()).count();
    let text = raw.trim().to_string();
    (
        (start + leading, start + leading + text.chars().count()),
        text,
    )
}

/// 折一段（不含换行符），返回（行们，是否还有没画下的）。
fn wrap_segment(
    segment: &str,
    width_of: &mut impl FnMut(&str) -> f32,
    max_width: f32,
    max_lines: usize,
) -> (Vec<Line>, bool) {
    let chars: Vec<char> = segment.chars().collect();
    let mut lines: Vec<Line> = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        if lines.len() >= max_lines {
            return (lines, true);
        }
        let slice = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };

        // 这一行能装到哪：整段装得下就到末尾，装不下就二分（宽度对前缀单调）
        let mut end = chars.len();
        if width_of(&slice(start, end)) > max_width {
            let (mut lo, mut hi) = (start + 1, chars.len());
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if width_of(&slice(start, mid)) <= max_width {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            end = lo;
        }

        if end >= chars.len() {
            let (range, line) = trimmed_line(&chars, start, end);
            if !line.is_empty() {
                lines.push(Line { range, text: line });
            }
            break;
        }

        // 别把西文词从中间劈开：往回找最后一个空白，断在它前面
        let mut cut = end;
        if !chars[end].is_whitespace()
            && let Some(pos) = (start + 1..end).rev().find(|&i| chars[i].is_whitespace())
        {
            cut = pos;
        }

        let (range, line) = trimmed_line(&chars, start, cut);
        if !line.is_empty() {
            lines.push(Line { range, text: line });
        }
        start = cut;
        while start < chars.len() && chars[start].is_whitespace() {
            start += 1;
        }
    }
    (lines, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个字符宽度算 1，方便对数字。
    fn width(s: &str) -> f32 {
        s.chars().count() as f32
    }

    /// 只要文本，忽略区间。
    fn texts(lines: Vec<Line>) -> Vec<String> {
        lines.into_iter().map(|line| line.text).collect()
    }

    #[test]
    fn breaks_on_width() {
        assert_eq!(
            texts(wrap_text("abcdefghij", width, 4.0, 5)),
            vec!["abcd", "efgh", "ij"]
        );
    }

    #[test]
    fn prefers_breaking_at_spaces() {
        assert_eq!(
            texts(wrap_text("hello world again", width, 8.0, 5)),
            vec!["hello", "world", "again"]
        );
    }

    #[test]
    fn breaks_chinese_per_character() {
        assert_eq!(
            texts(wrap_text("今天天气不错我们出去走走吧", width, 4.0, 5)),
            vec!["今天天气", "不错我们", "出去走走", "吧"]
        );
    }

    #[test]
    fn truncates_past_max_lines_with_ellipsis() {
        assert_eq!(
            texts(wrap_text("abcdefghijklmnopqrstuvwxyz", width, 4.0, 2)),
            vec!["abcd", "efgh…"]
        );
    }

    #[test]
    fn keeps_short_text_as_one_line_and_handles_empty() {
        assert_eq!(texts(wrap_text("短", width, 4.0, 5)), vec!["短"]);
        assert_eq!(texts(wrap_text("", width, 4.0, 5)), vec![""]);
    }

    #[test]
    fn splits_a_line_by_marks() {
        let line = Line {
            range: (10, 15),
            text: "hello".to_owned(),
        };
        // 改动区间是原文下标：原文 11..13 就是这一行的 "el"
        assert_eq!(
            split_by_marks(&line, &[(11, 13)]),
            vec![
                (false, "h".to_owned()),
                (true, "el".to_owned()),
                (false, "lo".to_owned()),
            ]
        );
        assert_eq!(
            split_by_marks(&line, &[]),
            vec![(false, "hello".to_owned())]
        );
        assert_eq!(
            split_by_marks(&line, &[(0, 99)]),
            vec![(true, "hello".to_owned())]
        );
    }

    #[test]
    fn reports_each_line_range_in_the_source() {
        let lines = wrap_text("aa bb cc", width, 2.0, 5);
        assert_eq!(texts(lines.clone()), vec!["aa", "bb", "cc"]);
        let ranges: Vec<(usize, usize)> = lines.into_iter().map(|line| line.range).collect();
        assert_eq!(ranges, vec![(0, 2), (3, 5), (6, 8)]);
    }

    #[test]
    fn honours_explicit_newlines() {
        assert_eq!(
            texts(wrap_text("first\nsecond", width, 40.0, 5)),
            vec!["first", "second"]
        );
        // 段落比行数预算还多：末尾给省略号
        assert_eq!(
            texts(wrap_text("aaaa\nbbbb\ncccc", width, 4.0, 2)),
            vec!["aaaa", "bbbb…"]
        );
    }

    #[test]
    fn a_word_wider_than_the_line_still_breaks() {
        assert_eq!(
            texts(wrap_text("abcdefghij klm", width, 4.0, 5)),
            vec!["abcd", "efgh", "ij", "klm"]
        );
    }
}
