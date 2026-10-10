use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::modifiers::Modifiers;

/// 修饰键 + 一个字母键的组合，配置里写成 `control+option+t`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KeyCombo {
    /// 修饰键，至少一个。
    pub modifiers: Modifiers,

    /// 字母或数字键（小写）。
    pub key: char,
}

impl KeyCombo {
    pub const TRANSLATE_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: true,
            shift: false,
            control: true,
            command: false,
        },
        key: 't',
    };

    /// 中 / 英切换（缺省 `⌃⇧R`）：切到英文就是纯英文模式，候选只出英文单词、输入框显示键盘原样。
    /// 另有「轻拍 Shift」（`[general] shift_tap_toggle`）也能切 —— 终端里 `⌃+字母` 那一族
    /// 基本被 shell 的 readline 占着（`⌃R` 是反向搜索），所以缺省带 `⇧`。
    pub const TOGGLE_ENGLISH_MODE_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: false,
            shift: true,
            control: true,
            command: false,
        },
        key: 'r',
    };

    /// 中文模式下拼音时英文候选的开关（缺省 `⌃⇧E`）。
    pub const TOGGLE_ENGLISH_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: false,
            shift: true,
            control: true,
            command: false,
        },
        key: 'e',
    };

    /// 打开偏好设置（缺省 `⌃⇧S`）：不用去菜单栏找，随手按一下。
    pub const OPEN_SETTINGS_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: false,
            shift: true,
            control: true,
            command: false,
        },
        key: 's',
    };

    /// 查询模式（中文想法 → 英文写法）：缺省 `⌃8`。
    /// 不用 `⌘8`（浏览器切标签）、也不用 `⌃数字` 以外顺手的位置，跟翻译 / 纠错一样避开应用常用键。
    pub const LOOKUP_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: false,
            shift: false,
            control: true,
            command: false,
        },
        key: '8',
    };

    /// 逐字模式：候选只留只吃一个音节的（单字与单音节词），一个字一个字挑。
    /// 用 `⌃⇧` 一族（`⌘` 被应用占、`⌃` 单键撞终端）。不用 `⌃⇧Z`：不少编辑器把 `⇧⌃Z`
    /// 当 redo，且实测这台机器上按它输入法收不到事件；`D` 是「单字」，左手好按。
    pub const WORD_BY_WORD_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: false,
            shift: true,
            control: true,
            command: false,
        },
        key: 'd',
    };

    pub const CORRECT_DEFAULT: Self = Self {
        modifiers: Modifiers {
            option: true,
            shift: false,
            control: true,
            command: false,
        },
        key: 'c',
    };

    /// 配置文件里的写法。
    pub fn key_string(&self) -> String {
        format!("{}+{}", self.modifiers.key(), self.key)
    }

    /// 给人看的写法：`⌃⌥T`。
    pub fn label(&self) -> String {
        format!(
            "{}{}",
            self.modifiers.label(),
            self.key.to_ascii_uppercase()
        )
    }
}

impl Default for KeyCombo {
    fn default() -> Self {
        Self::TRANSLATE_DEFAULT
    }
}

impl FromStr for KeyCombo {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (modifiers, key) = text
            .trim()
            .rsplit_once('+')
            .ok_or_else(|| format!("expected modifiers+key, got {text:?}"))?;
        let mut chars = key.trim().chars();
        let (Some(key), None) = (chars.next(), chars.next()) else {
            return Err(format!("key must be a single character: {key:?}"));
        };
        if !key.is_ascii_alphanumeric() {
            return Err(format!("key must be a letter or digit: {key:?}"));
        }
        Ok(Self {
            modifiers: modifiers.parse()?,
            key: key.to_ascii_lowercase(),
        })
    }
}

impl TryFrom<String> for KeyCombo {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<KeyCombo> for String {
    fn from(combo: KeyCombo) -> Self {
        combo.key_string()
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_labels() {
        let combo: KeyCombo = "control+option+t".parse().unwrap();
        assert_eq!(combo, KeyCombo::TRANSLATE_DEFAULT);
        assert_eq!(combo.label(), "⌃⌥T");
        assert_eq!(combo.key_string(), "control+option+t");
        assert!("t".parse::<KeyCombo>().is_err());
        assert!("option+tt".parse::<KeyCombo>().is_err());
        assert!("option+-".parse::<KeyCombo>().is_err());
        for option in ["control+shift+t", "control+option+e", "shift+command+9"] {
            assert_eq!(option.parse::<KeyCombo>().unwrap().key_string(), option);
        }
    }
}
