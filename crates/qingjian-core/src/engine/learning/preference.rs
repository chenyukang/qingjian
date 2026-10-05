//! 候选的排序偏好：词库里的词删不掉（`Hahn` 是随包英文表里的），但用户能要求「别排在前面」。

/// 用户对某个候选设的排序偏好。按「删候选」的修饰键 + 数字一次一档循环：
/// 正常 → 后置 → 正常。**后置不删词**，词还在候选里，只是沉到最后。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SortPreference {
    /// 正常（没设过）。
    #[default]
    Normal,

    /// 后置：排在正常候选之后。给「词本身没错、只是不该排前面」的那些用（`ui` 下的 `UI`）。
    Down,
}

impl SortPreference {
    /// 再按一次之后的状态：后置 ↔ 正常轮流。**必须能回到正常**，否则按错了没法撤销。
    pub fn cycled(self) -> Self {
        match self {
            Self::Normal => Self::Down,
            Self::Down => Self::Normal,
        }
    }

    /// 从 `user-sort.tsv` 的文件列解析。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "down" => Some(Self::Down),
            "normal" => Some(Self::Normal),
            _ => None,
        }
    }

    /// 写回文件用的写法。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Down => "down",
        }
    }

    /// 排序键里的档位：0 正常、1 后置（越大越靠后）。见 `crate::ranking::SortKey`。
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Down => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycling_always_comes_back() {
        assert_eq!(SortPreference::Normal.cycled(), SortPreference::Down);
        assert_eq!(SortPreference::Down.cycled(), SortPreference::Normal);
        assert_eq!(SortPreference::parse("down"), Some(SortPreference::Down));
        assert_eq!(SortPreference::parse("bogus"), None);
    }
}
