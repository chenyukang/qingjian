use qingjian_core::ModeKeys;
use serde::{Deserialize, Serialize};

use super::key_combo::KeyCombo;
use super::modifiers::Modifiers;
use super::switch_key::SwitchKeys;

/// 配置文件 `[shortcut]` 分节：前缀模式键（Core 的 [`ModeKeys`]）加壳层的修饰键组合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutConfig {
    /// 表达式 / 问字模式键，键名与以前一样直接在分节下（`expression` / `question`）。
    #[serde(flatten)]
    pub mode: ModeKeys,

    /// 中 / 英切换键（Windows 用），可多选：`["shift", "control", "ctrl+alt+space"]`。详见 [`SwitchKeys`]。
    pub switch_mode: SwitchKeys,

    /// 数字键配这些修饰键：上屏候选的第一个译词。
    pub translation: Modifiers,

    /// 数字键配这些修饰键：上屏候选的第二个译词（候选右侧有两个译词时）。
    pub translation_second: Modifiers,

    /// 把应用里选中的文字译成学习语言（需要云服务开着）。
    pub translate_selection: KeyCombo,

    /// 把应用里选中的文字纠错：中文改错别字与标点、英文改拼写与语法（需要云服务开着）。
    pub correct_selection: KeyCombo,

    /// 数字键配这些修饰键：删掉候选（用户词整个删掉，词库词清掉对它的学习）。
    /// 数字键配这些修饰键：删候选（用户词整个删掉，词库词清掉学习）。缺省 `⇧⌃` ——
    /// 破坏性动作放在难按的组合上，免得想置顶却按成它。
    pub delete_candidate: Modifiers,

    /// 中 / 英切换（缺省 `⌃⇧R`）：切到英文就是纯英文模式（候选只出英文单词、输入框键盘原样），
    /// 与 Caps Lock 并存 —— 两个任一开着就是英文。
    pub toggle_english_mode: KeyCombo,

    /// 打开偏好设置（缺省 `⌃⇧S`）。
    pub open_settings: KeyCombo,

    /// 查询模式（缺省 `⌃8`）：中文照常打拼音，候选给的是释义表里对应的英文词（带词性与中文解释），
    /// 按一次进、再按一次出；`Esc` 也退出。与中 / 英模式正交，激活时是什么模式就留在什么模式。
    pub lookup: KeyCombo,

    /// 切换中文模式下拼音时的英文词候选（缺省 `⌃⇧E`）：按一次关、再按一次开。
    pub toggle_english: KeyCombo,

    /// 数字键配这些修饰键：把这个候选「置顶」—— 排在正常候选之前。同音字的顺序会随上下文变
    ///（`ba` 下有时代 把、有时代 吧），用它钉住一个；再按一次恢复。缺省 `⇧⌃`。
    pub top_candidate: Modifiers,

    /// 数字键配这些修饰键：把这个候选「隐藏」，以后不再出现在候选里（词库里的词删不掉，这是它的去处）。
    /// 与 [`Self::delete_candidate`] 分工：那个是「后置」（还看得见，只排到最后），这个是「不要了」。
    pub hide_candidate: Modifiers,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        // Windows 上 Alt+数字被系统当菜单快捷键截走（TSF 收不到），译词键缺省用 Ctrl；macOS 用 Option。
        #[cfg(windows)]
        let (translation, translation_second) = (Modifiers::CONTROL, Modifiers::SHIFT_CONTROL);
        #[cfg(not(windows))]
        let (translation, translation_second) = (Modifiers::OPTION, Modifiers::SHIFT_OPTION);
        // 置顶用最好按的组合（常用、可逆），删候选用难按的（破坏性）。macOS 的 `⇧⌃` 不撞译词键；
        // Windows 上 `⇧⌃` 已经是第二个译词键，那边保持旧的 `⇧`（置顶还没接，等接了再一起调）
        #[cfg(not(windows))]
        let delete_candidate = Modifiers::SHIFT_CONTROL;
        #[cfg(windows)]
        let delete_candidate = Modifiers::SHIFT;
        Self {
            mode: ModeKeys::default(),
            switch_mode: SwitchKeys::default(),
            translation,
            translation_second,
            translate_selection: KeyCombo::TRANSLATE_DEFAULT,
            correct_selection: KeyCombo::CORRECT_DEFAULT,
            top_candidate: Modifiers::SHIFT,
            delete_candidate,
            hide_candidate: Modifiers::CONTROL,
            lookup: KeyCombo::LOOKUP_DEFAULT,
            toggle_english: KeyCombo::TOGGLE_ENGLISH_DEFAULT,
            toggle_english_mode: KeyCombo::TOGGLE_ENGLISH_MODE_DEFAULT,
            open_settings: KeyCombo::OPEN_SETTINGS_DEFAULT,
        }
    }
}

impl ShortcutConfig {
    /// 删候选的修饰键；为空或与任一组译词键撞了就退回缺省。
    pub fn delete_keys(&self) -> Modifiers {
        let (first, second) = self.translation_keys();
        if self.delete_candidate.is_empty()
            || self.delete_candidate == first
            || self.delete_candidate == second
        {
            tracing::warn!(
                configured = %self.delete_candidate.key(),
                "删候选的修饰键为空或与译词键相同，回落缺省"
            );
            Self::default().delete_candidate
        } else {
            self.delete_candidate
        }
    }

    /// 置顶候选的修饰键；为空、与删候选 / 隐藏候选相同、或与任一组译词键撞了就退回缺省。
    pub fn top_keys(&self) -> Modifiers {
        let (first, second) = self.translation_keys();
        if self.top_candidate.is_empty()
            || self.top_candidate == self.delete_keys()
            || self.top_candidate == self.hide_keys()
            || self.top_candidate == first
            || self.top_candidate == second
        {
            tracing::warn!(
                configured = %self.top_candidate.key(),
                "置顶候选的修饰键为空或与其它动作撞车，回落缺省"
            );
            Self::default().top_candidate
        } else {
            self.top_candidate
        }
    }

    /// 隐藏候选的修饰键；为空、与删候选相同、或与任一组译词键撞了就退回缺省。
    pub fn hide_keys(&self) -> Modifiers {
        let (first, second) = self.translation_keys();
        if self.hide_candidate.is_empty()
            || self.hide_candidate == self.delete_keys()
            || self.hide_candidate == first
            || self.hide_candidate == second
        {
            tracing::warn!(
                configured = %self.hide_candidate.key(),
                "隐藏候选的修饰键为空或与其它动作撞车，回落缺省"
            );
            Self::default().hide_candidate
        } else {
            self.hide_candidate
        }
    }

    /// 两组译词修饰键；两组相同或有一组为空时整个退回缺省，不做一半。
    pub fn translation_keys(&self) -> (Modifiers, Modifiers) {
        if self.translation == self.translation_second
            || self.translation.is_empty()
            || self.translation_second.is_empty()
        {
            let default = Self::default();
            (default.translation, default.translation_second)
        } else {
            (self.translation, self.translation_second)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_without_modifier_keys_still_parse_and_get_defaults() {
        // 缺省值分平台（Windows 用 Ctrl 系、其余用 Option 系），断言跟着平台的 Default 走
        let default = ShortcutConfig::default();
        let parsed: ShortcutConfig = toml::from_str("expression = \"i\"\n").unwrap();
        assert_eq!(parsed.mode.expression, 'i');
        assert_eq!(parsed.switch_mode, SwitchKeys::default());
        assert_eq!(
            parsed.translation_keys(),
            (default.translation, default.translation_second)
        );
        let same: ShortcutConfig =
            toml::from_str("translation = \"option\"\ntranslation_second = \"option\"\n").unwrap();
        assert_eq!(
            same.translation_keys(),
            (default.translation, default.translation_second)
        );
        let swapped: ShortcutConfig =
            toml::from_str("translation = \"control+option\"\ntranslation_second = \"option\"\n")
                .unwrap();
        assert_eq!(swapped.translation_keys().1, Modifiers::OPTION);
    }

    #[test]
    fn top_keys_fall_back_when_clashing() {
        let default = ShortcutConfig::default();
        // 缺省 ⇧（好按、可逆的那个动作）
        let parsed: ShortcutConfig = toml::from_str("").unwrap();
        assert_eq!(parsed.top_keys(), default.top_candidate);
        assert_eq!(default.top_candidate, Modifiers::SHIFT);
        // 缺省的两个动作不会撞在一起
        assert_ne!(default.top_keys(), default.delete_keys());
        // 与删候选撞上就回落
        let clashing = format!("top_candidate = \"{}\"\n", default.delete_candidate.key());
        let same: ShortcutConfig = toml::from_str(&clashing).unwrap();
        assert_eq!(same.top_keys(), default.top_candidate);
    }

    #[test]
    fn hide_keys_fall_back_when_clashing() {
        let default = ShortcutConfig::default();
        let parsed: ShortcutConfig = toml::from_str("").unwrap();
        assert_eq!(parsed.hide_keys(), default.hide_candidate);
        // 与平台缺省的译词键撞上
        let clash: ShortcutConfig = toml::from_str(&format!(
            "hide_candidate = \"{}\"\n",
            default.translation.key()
        ))
        .unwrap();
        assert_eq!(clash.hide_keys(), default.hide_candidate);
        // 与删候选撞上
        let clashing = format!("hide_candidate = \"{}\"\n", default.delete_candidate.key());
        let same: ShortcutConfig = toml::from_str(&clashing).unwrap();
        assert_eq!(same.hide_keys(), default.hide_candidate);
        // 自己设的照常生效（command 不与任何缺省键相撞）
        let own: ShortcutConfig = toml::from_str("hide_candidate = \"command\"\n").unwrap();
        assert!(own.hide_keys().command, "{:?}", own.hide_keys());
    }

    #[test]
    fn delete_keys_fall_back_when_clashing_with_translation_keys() {
        let default = ShortcutConfig::default();
        let parsed: ShortcutConfig = toml::from_str("").unwrap();
        assert_eq!(parsed.delete_keys(), default.delete_candidate);
        // 与平台缺省的译词键撞上才算「冲突」，两边平台都成立
        let clash: ShortcutConfig = toml::from_str(&format!(
            "delete_candidate = \"{}\"\n",
            default.translation.key()
        ))
        .unwrap();
        assert_eq!(clash.delete_keys(), default.delete_candidate);
        // 不与任何一组译词键冲突的修饰键：平台上取一个，断言它原样生效
        let free = [Modifiers::OPTION, Modifiers::CONTROL, Modifiers::SHIFT]
            .into_iter()
            .find(|m| *m != default.translation && *m != default.translation_second)
            .unwrap();
        let custom: ShortcutConfig =
            toml::from_str(&format!("delete_candidate = \"{}\"\n", free.key())).unwrap();
        assert_eq!(custom.delete_keys(), free);
    }

    #[test]
    fn switch_mode_parses_and_defaults_to_shift() {
        let parsed: ShortcutConfig = toml::from_str("switch_mode = [\"ctrl\"]\n").unwrap();
        assert!(parsed.switch_mode.control && !parsed.switch_mode.shift);
        let off: ShortcutConfig = toml::from_str("switch_mode = \"none\"\n").unwrap();
        assert_eq!(off.switch_mode, crate::SwitchKeys::NONE);
        let missing: ShortcutConfig = toml::from_str("").unwrap();
        assert_eq!(missing.switch_mode, SwitchKeys::default());
    }
}
