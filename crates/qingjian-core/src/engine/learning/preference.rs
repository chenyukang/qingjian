//! 候选的排序偏好：词库里的词删不掉（`Hahn` 是随包英文表里的），但用户能要求「别排在前面」。

/// 用户对某个候选设的排序偏好。两个修饰键各自把一个词切到自己的那一档，再按一次恢复：
/// `Shift+数字` → 后置（`Hahn` 不再挡路），`⌃+数字` → 隐藏（`wodge` 干脆不出现）。
/// **两者都不删词**：词库里的词删不掉，也不该因为一次手滑就消失。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SortPreference {
    /// 正常（没设过）。
    #[default]
    Normal,

    /// 后置：排在正常候选之后。给「词本身没错、只是不该排前面」的那些用（`ui` 下的 `UI`）。
    Down,

    /// 隐藏：不再出现在候选里。给「以后完全不想看到」的那些用（`wodge`、`WODGES`）。
    /// 隐藏的词在候选窗里就没法再按回来了，恢复入口在偏好设置的列表里。
    Hidden,
}

impl SortPreference {
    /// 按下表示 `target` 那一档的键之后的状态：已经是它就恢复，否则切到它。
    /// **必须能回到正常**，否则按错了没法撤销。
    pub fn toggled(self, target: Self) -> Self {
        if self == target { Self::Normal } else { target }
    }

    /// 从 `user-sort.tsv` 的文件列解析。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "down" => Some(Self::Down),
            "hidden" => Some(Self::Hidden),
            "normal" => Some(Self::Normal),
            _ => None,
        }
    }

    /// 写回文件用的写法。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Down => "down",
            Self::Hidden => "hidden",
        }
    }

    /// 排序键里的档位：0 正常、1 后置（越大越靠后）。见 `crate::ranking::SortKey`。
    /// 隐藏的词在候选组装完就被滤掉了，走不到排序这里。
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Normal | Self::Hidden => 0,
            Self::Down => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_always_comes_back() {
        assert_eq!(
            SortPreference::Normal.toggled(SortPreference::Down),
            SortPreference::Down
        );
        assert_eq!(
            SortPreference::Down.toggled(SortPreference::Down),
            SortPreference::Normal
        );
        assert_eq!(
            SortPreference::Normal.toggled(SortPreference::Hidden),
            SortPreference::Hidden
        );
        assert_eq!(
            SortPreference::Hidden.toggled(SortPreference::Hidden),
            SortPreference::Normal
        );
        // 换一个键按：直接切到那一档，不叠成两层
        assert_eq!(
            SortPreference::Down.toggled(SortPreference::Hidden),
            SortPreference::Hidden
        );
        assert_eq!(
            SortPreference::parse("hidden"),
            Some(SortPreference::Hidden)
        );
        assert_eq!(SortPreference::parse("bogus"), None);
    }
}
