//! 候选的排序偏好：词库里的词删不掉（`Hahn` 是随包英文表里的），但用户能要求「别排在前面」。

/// 用户对某个候选设的排序偏好。三个修饰键各自把一个词切到自己的那一档，再按一次恢复：
/// `⇧+数字` → 后置（`Hahn` 不再挡路），`⌃+数字` → 隐藏（`wodge` 干脆不出现），
/// `⇧⌃+数字` → 置顶（同音字里顺序随上下文变，用它钉住一个：`ba` 下 `吧` 永远第一）。
/// **三个都不删词**：词库里的词删不掉，也不该因为一次手滑就消失。
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

    /// 置顶：排在正常候选之前。给「这个词没错、但我要它永远第一个」的那些用
    ///（单字的同音候选顺序会随上下文变，`ba` 下有时代 把、有时代 吧）。
    Top,
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
            // `top` 不再从排序偏好文件读：置顶是按输入串的，存在 `user-pins.tsv`。
            // 旧文件里留下的 `top` 会被当成坏行跳过（并记一条警告）。
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
            Self::Top => "top",
        }
    }

    /// 排序键里的档位：-1 置顶、0 正常、1 后置（越大越靠后）。见 `crate::ranking::SortKey`。
    /// 隐藏的词在候选组装完就被滤掉了，走不到排序这里。
    pub(crate) fn rank(self) -> i8 {
        match self {
            Self::Top => -1,
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
