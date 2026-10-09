//! 菜单与偏好设置窗口的动作：只改 config.toml（或触发一次性操作），改完由 apply_config 统一生效。

use super::diagnostics::{copy_to_pasteboard, open_with_system};
use super::*;
use crate::preferences::DEFAULT_FONT_LABEL;
use qingjian_platform::{
    Anchor, CandidateBackground, Color, SHIFT_TAP_WINDOW_CHOICES, Shape, ShiftLetter, Visibility,
};

/// 「指示器」页大小下拉里那几档，与页面上的列表一致。
const STATUS_BAR_SIZES: [i32; 8] = [8, 10, 12, 14, 16, 20, 24, 30];

impl Host {
    /// 写短语前读取文件；外部规则有变化时同步列表并请用户重新确认。
    fn phrases_are_current(&mut self) -> bool {
        let Some(path) = self.settings.path() else {
            return false;
        };
        match qingjian_platform::Config::load(path) {
            Ok(latest) if latest.custom_phrases == self.settings.config().custom_phrases => true,
            Ok(_) => {
                self.settings.reload();
                self.apply_config(false);
                let message = "规则已在其他地方修改，请重新确认后操作。";
                self.preferences.set_phrase_error(message);
                self.preferences.set_status(message);
                false
            }
            Err(error) => {
                self.preferences.set_phrase_error(&error.to_string());
                self.preferences.set_status(&error.to_string());
                false
            }
        }
    }

    /// 表格中的启用开关只修改所选规则。
    pub fn set_phrase_enabled(&mut self, index: usize, enabled: bool) {
        if !self.phrases_are_current() {
            return;
        }
        let mut phrases = self.settings.config().custom_phrases.clone();
        let Some(phrase) = phrases.get_mut(index) else {
            return;
        };
        phrase.enabled = enabled;
        let Some(path) = self.settings.path() else {
            return;
        };
        let result = qingjian_platform::Config::set_custom_phrases(path, &phrases);
        self.settings.reload();
        self.apply_config(false);
        if let Err(error) = result {
            self.preferences.set_status(&error);
        }
    }

    /// 菜单动作。开关类先落盘再热加载，菜单勾选状态永远来自文件里的值。
    pub fn perform(&mut self, action: MenuAction) {
        tracing::info!(?action, "菜单");
        match action {
            MenuAction::ToggleCloud => {
                let on = !self.settings.config().predict.enabled;
                if self.settings.set_bool("predict", "enabled", on) {
                    self.apply_config(false);
                }
            }
            MenuAction::ToggleFuzzy(index) => {
                let name = FuzzyRules::NAMES[index];
                let on = !self.settings.config().fuzzy.is_on(name);
                if self.settings.set_bool("fuzzy", name, on) {
                    self.apply_config(false);
                }
            }
            MenuAction::OpenPreferences => {
                // 后置 / 隐藏的列表可能刚在候选窗里改过，重新装配一次再显示
                self.apply_config(false);
                self.preferences.sync_usage(
                    &self.engine.usage_summary(),
                    &self.engine.vocabulary_summary(),
                    self.learning_language,
                );
                self.preferences.show();
            }
            MenuAction::OpenLogs => {
                if let Some(dir) = logging::log_dir() {
                    open_with_system(&[&dir.to_string_lossy()]);
                }
            }
            MenuAction::OpenDownload => open_with_system(&[qingjian_update::DOWNLOAD_URL]),
        }
    }

    /// 设置窗口里改了一个控件：写配置、热加载；写不成（非法组合、空文本）也要把控件同步回真实值。
    pub fn change_setting(&mut self, setting: Setting, value: SettingValue) {
        // 密钥值不进日志
        tracing::info!(?setting, "设置");
        let config = self.settings.config().clone();
        // 左侧栏导航：只切页面，不碰配置
        if let Setting::SelectPage(index) = setting {
            self.preferences.select_page(index);
            return;
        }
        match (setting, value) {
            (Setting::NewPhrase, _) => {
                self.preferences.edit_phrase(&config, None);
                return;
            }
            (Setting::EditPhrase, _) => {
                if let Some(index) = self.preferences.selected_phrase() {
                    self.preferences.edit_phrase(&config, Some(index));
                }
                return;
            }
            (Setting::CancelPhraseEdit, _) => {
                self.preferences.close_phrase_editor();
                return;
            }

            (Setting::PhraseDraft, _) => return,
            (Setting::SelectPhrase, SettingValue::Index(index)) => {
                self.preferences.select_phrase(&config, index);
                return;
            }
            (Setting::SavePhrase | Setting::DeletePhrase, _) => {
                if !self.phrases_are_current() {
                    return;
                }
                let config = self.settings.config();
                let mut phrases = config.custom_phrases.clone();
                let mut saved_index = phrases.len();
                if setting == Setting::DeletePhrase {
                    let Some(index) = self.preferences.selected_phrase() else {
                        return;
                    };
                    if index >= phrases.len() {
                        return;
                    }
                    phrases.remove(index);
                } else {
                    let (index, draft) = match self.preferences.phrase_draft(config) {
                        Ok(value) => value,
                        Err(error) => {
                            self.preferences.set_phrase_error(&error);
                            self.preferences.set_status(&error);
                            return;
                        }
                    };
                    if let Some(index) = index {
                        let Some(phrase) = phrases.get_mut(index) else {
                            return;
                        };
                        *phrase = draft;
                        saved_index = index;
                    } else {
                        phrases.push(draft);
                    }
                }
                let Some(path) = self.settings.path() else {
                    return;
                };
                if let Err(error) = qingjian_platform::Config::set_custom_phrases(path, &phrases) {
                    self.preferences.set_phrase_error(&error);
                    self.preferences.set_status(&error);
                    return;
                }
                self.preferences.close_phrase_editor();
                self.settings.reload();
                self.apply_config(false);
                if setting == Setting::SavePhrase {
                    self.preferences
                        .select_phrase(self.settings.config(), saved_index + 1);
                }
                self.preferences.set_status("自定义短语已保存");
                return;
            }
            (Setting::FullWidthPunctuation, SettingValue::Index(index)) => {
                self.settings
                    .set_bool("general", "full_width_punctuation", index == 0);
            }
            (Setting::LearningLanguage, SettingValue::Index(index)) => {
                // 菜单最后一项是「不显示译文」
                let code = self
                    .languages
                    .get(index)
                    .map_or(LEARNING_LANGUAGE_OFF, |language| language.code());
                self.settings
                    .set_value("general", "learning_language", code);
            }
            (Setting::PageSize, SettingValue::Index(index)) => {
                self.settings
                    .set_value("general", "page_size", index as i64 + 1);
            }
            (Setting::PageKeys, SettingValue::Index(index)) => {
                if let Some(pair) = PAGE_KEY_OPTIONS.get(index) {
                    self.settings.set_value("general", "page_keys", *pair);
                }
            }
            (Setting::Theme, SettingValue::Index(index)) => {
                if let Some(theme) = ThemeMode::ALL.get(index) {
                    self.settings.set_value("general", "theme", theme.key());
                }
            }
            (Setting::Renderer, SettingValue::Index(index)) => {
                if let Some(renderer) = CandidateRenderer::ALL.get(index) {
                    self.settings
                        .set_value("general", "renderer", renderer.key());
                }
            }
            (Setting::CandidateBackground, SettingValue::Index(index)) => {
                if let Some(background) = CandidateBackground::ALL.get(index) {
                    self.settings
                        .set_value("general", "candidate_background", background.key());
                }
            }
            (Setting::Font, SettingValue::Text(text)) => {
                let font = text.trim();
                let font = if font == DEFAULT_FONT_LABEL { "" } else { font };
                self.settings.set_value("general", "font", font);
            }
            (Setting::Layout, SettingValue::Index(index)) => {
                if let Some(layout) = LayoutMode::ALL.get(index) {
                    self.settings.set_value("general", "layout", layout.key());
                }
            }
            (Setting::HorizontalGrid, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "horizontal_grid", on);
            }
            (Setting::Preedit, SettingValue::Index(index)) => {
                if let Some(mode) = PreeditMode::ALL.get(index) {
                    self.settings.set_value("general", "preedit", mode.key());
                }
            }
            (Setting::QuestionMark, SettingValue::Bool(on)) => {
                self.settings.set_bool("shortcut", "question_mark", on);
            }
            (Setting::ExpressionKey | Setting::QuestionKey, SettingValue::Index(index)) => {
                if let Some(&key) = ModeKeys::CANDIDATES.get(index) {
                    let mut keys = config.shortcut.mode.sanitized();
                    let name = if setting == Setting::ExpressionKey {
                        keys.expression = key;
                        "expression"
                    } else {
                        keys.question = key;
                        "question"
                    };
                    if keys.is_valid() {
                        self.settings.set_value("shortcut", name, key.to_string());
                    } else {
                        tracing::warn!("表达式键与问字键不能相同，未改");
                    }
                }
            }
            (
                Setting::TranslationKeys | Setting::TranslationSecondKeys,
                SettingValue::Text(text),
            ) => match text.parse::<Modifiers>() {
                Ok(chosen) => {
                    let (first, second) = config.shortcut.translation_keys();
                    let (name, other) = if setting == Setting::TranslationKeys {
                        ("translation", second)
                    } else {
                        ("translation_second", first)
                    };
                    if chosen == other {
                        tracing::warn!("两组译词快捷键不能相同，未改");
                    } else {
                        self.settings.set_value("shortcut", name, chosen.key());
                    }
                }
                Err(error) => tracing::warn!(%error, "修饰键组合不合法，未改"),
            },
            (Setting::DeleteCandidateKeys, SettingValue::Text(text)) => {
                match text.parse::<Modifiers>() {
                    Ok(chosen) => {
                        let (first, second) = config.shortcut.translation_keys();
                        if chosen == first || chosen == second {
                            tracing::warn!("删候选的快捷键不能与译词快捷键相同，未改");
                        } else {
                            self.settings
                                .set_value("shortcut", "delete_candidate", chosen.key());
                        }
                    }
                    Err(error) => tracing::warn!(%error, "修饰键组合不合法，未改"),
                }
            }
            (Setting::OpenSettingsKeys, SettingValue::Text(text)) => {
                match text.parse::<KeyCombo>() {
                    Ok(combo) => {
                        self.settings
                            .set_value("shortcut", "open_settings", combo.key_string());
                    }
                    Err(error) => self.preferences.set_status(&error.to_string()),
                }
                self.apply_config(false);
                return;
            }
            (Setting::ToggleEnglishModeKeys, SettingValue::Text(text)) => {
                match text.parse::<KeyCombo>() {
                    Ok(combo) => {
                        self.settings.set_value(
                            "shortcut",
                            "toggle_english_mode",
                            combo.key_string(),
                        );
                    }
                    Err(error) => tracing::warn!(%error, "中 / 英切换的键不合法，未改"),
                }
            }
            (Setting::CorrectSelectionKeys, SettingValue::Text(text)) => {
                match text.parse::<KeyCombo>() {
                    Ok(combo) => {
                        self.settings.set_value(
                            "shortcut",
                            "correct_selection",
                            combo.key_string(),
                        );
                    }
                    Err(error) => tracing::warn!(%error, "快捷键不合法，未改"),
                }
            }
            (Setting::TranslateSelectionKeys, SettingValue::Text(text)) => {
                match text.parse::<KeyCombo>() {
                    Ok(combo) => {
                        self.settings.set_value(
                            "shortcut",
                            "translate_selection",
                            combo.key_string(),
                        );
                    }
                    Err(error) => tracing::warn!(%error, "快捷键不合法，未改"),
                }
            }
            (Setting::LookupKeys, SettingValue::Text(text)) => match text.parse::<KeyCombo>() {
                Ok(combo) => {
                    self.settings
                        .set_value("shortcut", "lookup", combo.key_string());
                }
                Err(error) => tracing::warn!(%error, "快捷键不合法，未改"),
            },
            (Setting::ResetShortcuts, _) => {
                let defaults = ShortcutConfig::default();
                self.settings
                    .set_value("general", "page_keys", PAGE_KEY_OPTIONS[0]);
                self.settings.set_value(
                    "shortcut",
                    "expression",
                    defaults.mode.expression.to_string(),
                );
                self.settings
                    .set_value("shortcut", "question", defaults.mode.question.to_string());
                self.settings
                    .set_bool("shortcut", "question_mark", defaults.mode.question_mark);
                self.settings
                    .set_value("shortcut", "translation", defaults.translation.key());
                self.settings.set_value(
                    "shortcut",
                    "translation_second",
                    defaults.translation_second.key(),
                );
                self.settings.set_value(
                    "shortcut",
                    "translate_selection",
                    defaults.translate_selection.key_string(),
                );
                self.settings.set_value(
                    "shortcut",
                    "correct_selection",
                    defaults.correct_selection.key_string(),
                );
                self.settings.set_value(
                    "shortcut",
                    "delete_candidate",
                    defaults.delete_candidate.key(),
                );
            }
            (Setting::HideCandidateKeys, SettingValue::Text(text)) => {
                match text.parse::<Modifiers>() {
                    Ok(chosen) => {
                        let (first, second) = config.shortcut.translation_keys();
                        if chosen == first
                            || chosen == second
                            || chosen == config.shortcut.delete_keys()
                            || chosen == config.shortcut.hide_keys()
                        {
                            // 与译词 / 删候选撞了就退回缺省：hide_keys() 自己会兜底
                            tracing::warn!("隐藏候选的快捷键与别的键冲突，退回缺省");
                        }
                        self.settings
                            .set_value("shortcut", "hide_candidate", chosen.key());
                    }
                    Err(error) => tracing::warn!(%error, "隐藏候选的快捷键解析失败，未改"),
                }
                return;
            }
            (Setting::DictionaryEnabled(index), SettingValue::Bool(on)) => {
                if let Some(info) = self.dictionary_list.get(index).cloned() {
                    if info.builtin {
                        self.set_domain_enabled(&info.stem, on);
                    } else {
                        self.set_dictionary_enabled(&info.stem, on);
                    }
                }
            }
            (Setting::DictionaryRemove(index), _) => {
                self.remove_dictionary(index);
                return;
            }
            (Setting::ImportDictionary, _) => {
                if let Some(path) = crate::preferences::choose_dictionary_file() {
                    self.import_dictionary(&path);
                }
                return;
            }
            (Setting::RestoreSortPreference(index), _) => {
                if let Some((word, _)) = self.sort_preference_list.get(index).cloned() {
                    self.engine.restore_sort_preference(&word);
                    // 重新装配一遍：列表、别的页与引擎状态一起刷新
                    self.apply_config(false);
                }
                return;
            }
            (Setting::PerAppMode, SettingValue::Bool(on)) => {
                self.settings.set_bool("apps", "per_app_mode", on);
                self.apply_config(false);
                return;
            }
            (Setting::StatusBarVisibility, SettingValue::Index(index)) => {
                if let Some(visibility) = Visibility::ALL.get(index) {
                    self.settings
                        .set_value("status_bar", "visibility", visibility.key());
                    self.apply_config(false);
                }
                return;
            }
            (Setting::StatusBarEnabled, SettingValue::Bool(on)) => {
                self.settings.set_bool("status_bar", "enabled", on);
                self.apply_config(false);
                return;
            }
            (Setting::StatusBarOutline, SettingValue::Bool(on)) => {
                self.settings.set_bool("status_bar", "outline", on);
                self.apply_config(false);
                return;
            }
            (Setting::StatusBarAnchor, SettingValue::Index(index)) => {
                if let Some(anchor) = Anchor::ALL.get(index) {
                    self.settings
                        .set_value("status_bar", "anchor", anchor.key());
                    self.apply_config(false);
                }
                return;
            }
            (Setting::StatusBarShape, SettingValue::Index(index)) => {
                if let Some(shape) = Shape::ALL.get(index) {
                    self.settings.set_value("status_bar", "shape", shape.key());
                    self.apply_config(false);
                }
                return;
            }
            (Setting::StatusBarSize, SettingValue::Index(index)) => {
                if let Some(size) = STATUS_BAR_SIZES.get(index) {
                    self.settings
                        .set_value("status_bar", "size", i64::from(*size));
                    self.apply_config(false);
                }
                return;
            }
            (Setting::StatusBarCloudIcon, SettingValue::Bool(on)) => {
                self.settings.set_bool("status_bar", "menubar_item", on);
                self.apply_config(false);
                return;
            }
            (Setting::StatusBarNotice, SettingValue::Bool(on)) => {
                self.settings.set_bool("status_bar", "notice", on);
                self.apply_config(false);
                return;
            }
            (Setting::StatusBarOffsetX, SettingValue::Text(text))
            | (Setting::StatusBarOffsetY, SettingValue::Text(text)) => {
                let key = if matches!(setting, Setting::StatusBarOffsetY) {
                    "offset_y"
                } else {
                    "offset_x"
                };
                match text.trim().parse::<i32>() {
                    Ok(value) => {
                        self.settings.set_value(
                            "status_bar",
                            key,
                            i64::from(value.clamp(-4000, 4000)),
                        );
                        self.apply_config(false);
                    }
                    Err(error) => tracing::warn!(%error, key, "偏移要写整数（点），未改"),
                }
                return;
            }
            (Setting::ShiftTapWindow, SettingValue::Index(index)) => {
                if let Some(ms) = SHIFT_TAP_WINDOW_CHOICES.get(index) {
                    self.settings
                        .set_value("general", "shift_tap_window_ms", i64::from(*ms));
                    self.apply_config(false);
                }
                return;
            }
            (Setting::ShiftTapToggle, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "shift_tap_toggle", on);
                return;
            }
            (Setting::StatusBarChineseColor, SettingValue::Color(native))
            | (Setting::StatusBarEnglishColor, SettingValue::Color(native))
            | (Setting::StatusBarLookupColor, SettingValue::Color(native)) => {
                match crate::indicator::config_color(&native) {
                    Some(color) => {
                        self.settings.set_value(
                            "status_bar",
                            status_bar_color_key(setting),
                            color.hex(),
                        );
                        self.apply_config(false);
                    }
                    None => tracing::warn!("取色器给的颜色转不成 sRGB，未改"),
                }
                return;
            }
            (Setting::StatusBarChineseColor, SettingValue::Text(text))
            | (Setting::StatusBarEnglishColor, SettingValue::Text(text))
            | (Setting::StatusBarLookupColor, SettingValue::Text(text)) => {
                match text.parse::<Color>() {
                    Ok(color) => {
                        self.settings.set_value(
                            "status_bar",
                            status_bar_color_key(setting),
                            color.hex(),
                        );
                        self.apply_config(false);
                    }
                    Err(error) => tracing::warn!(%error, "颜色没解析出来，未改"),
                }
                return;
            }
            (Setting::RestoreSortPreferences, _) => {
                let restored = self.engine.restore_sort_preferences();
                tracing::info!(restored, "恢复后置 / 隐藏的候选");
                self.apply_config(false);
                return;
            }
            (Setting::Fuzzy(index), SettingValue::Bool(on)) => {
                self.settings
                    .set_bool("fuzzy", FuzzyRules::NAMES[index], on);
            }
            (Setting::CloudEnabled, SettingValue::Bool(on)) => {
                self.settings.set_bool("predict", "enabled", on);
            }
            (Setting::LocalModelEnabled, SettingValue::Bool(on)) => {
                self.settings.set_bool("model", "enabled", on);
            }
            (Setting::UpdateCheck, SettingValue::Bool(on)) => {
                self.settings.set_bool("update", "check", on);
            }
            (Setting::UpdateChannel, SettingValue::Index(index)) => {
                if let Some(channel) = UpdateChannel::ALL.get(index) {
                    self.settings.set_value("update", "channel", channel.key());
                }
            }
            (Setting::CloudSlots, SettingValue::Index(index)) => {
                self.settings.set_value("predict", "slots", index as i64);
            }
            (Setting::Traditional, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "traditional", on);
            }
            (Setting::EnglishCandidates, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "english_candidates", on);
            }
            (Setting::EnglishInPinyin, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "english_in_pinyin", on);
            }
            (Setting::EmojiCandidates, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "emoji_candidates", on);
            }
            (Setting::MixedSpace, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "mixed_space", on);
            }
            (Setting::EnglishModeMinLetters, SettingValue::Index(index)) => {
                self.settings
                    .set_value("general", "english_mode_min_letters", index as i64 + 1);
                self.apply_config(false);
                return;
            }
            (Setting::EnglishMinLetters, SettingValue::Index(index)) => {
                self.settings
                    .set_value("general", "english_min_letters", index as i64 + 1);
            }
            (Setting::ChineseFirst, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "chinese_first", on);
            }
            (Setting::ShiftLetter, SettingValue::Bool(on)) => {
                let mode = if on {
                    ShiftLetter::Compose
                } else {
                    ShiftLetter::Passthrough
                };
                self.settings
                    .set_value("general", "shift_letter", mode.key());
            }
            // 勾上写缺省的终端 / 编辑器列表，去掉写空表；手改过的列表勾一下就回缺省
            (Setting::EnglishCandidatesOffInApps, SettingValue::Bool(on)) => {
                let apps: toml_edit::Array = if on {
                    DEFAULT_ENGLISH_CANDIDATES_OFF.iter().copied().collect()
                } else {
                    toml_edit::Array::new()
                };
                self.settings
                    .set_value("apps", "english_candidates_off", apps);
            }
            // 弹出菜单按 Scheme::ALL 的顺序。写的是 [general] scheme（旧键 shuangpin 已并入它）：
            // 写旧键的话，配置里 scheme 的缺省值非空、解析时优先，用户选的方案会被静默忽略。
            (Setting::Scheme, SettingValue::Index(index)) => {
                let key = Scheme::ALL
                    .get(index)
                    .map_or(Scheme::Pinyin.key(), |scheme| scheme.key());
                self.settings.set_value("general", "scheme", key);
            }
            (Setting::ShuangpinRawPreedit, SettingValue::Bool(on)) => {
                self.settings
                    .set_bool("general", "shuangpin_raw_preedit", on);
            }
            // 五笔：勾上就是 86 版，取消就是关。与上面的拼音方案同时开着就是混输。
            (Setting::Wubi, SettingValue::Bool(on)) => {
                self.settings
                    .set_value("general", "wubi", if on { "wubi86" } else { "" });
            }
            // 文本框失焦也会发 action：值没变就不写，免得每次切窗口都重写一遍配置
            (Setting::BaseUrl, SettingValue::Text(text)) => {
                let text = text.trim();
                if !text.is_empty() && text != config.predict.base_url {
                    self.settings.set_value("predict", "base_url", text);
                }
            }
            (Setting::Model, SettingValue::Text(text)) => {
                let text = text.trim();
                if !text.is_empty() && text != config.predict.model {
                    self.settings.set_value("predict", "model", text);
                }
            }
            (Setting::ApiKey, SettingValue::Text(text)) => {
                let text = text.trim();
                // 密码框看不见内容，粘贴多了（带上了终端提示符、命令）用户发现不了；这种值写进 .env 还会让整个文件解析失败
                if text.chars().any(|c| !c.is_ascii_graphic()) {
                    self.preferences.set_status(
                        "密钥没有保存：里面有空格或非英文字符，多半是粘贴时多带了别的内容",
                    );
                    return;
                }
                if text.is_empty() {
                    return;
                }
                if self.settings.set_env_var(&config.predict.api_key_env, text) {
                    // 密钥换了必须重建 Predictor
                    self.apply_config(true);
                    self.preferences.set_status("密钥已保存");
                } else {
                    self.preferences
                        .set_status("密钥没有保存：写不进配置目录的 .env，详情见日志");
                }
                return;
            }
            (Setting::TestCloud, _) => {
                self.start_cloud_test();
                return;
            }
            (Setting::OpenConfigFile, _) => {
                if let Some(path) = self.settings.path() {
                    open_with_system(&["-t", &path.to_string_lossy()]);
                }
                return;
            }
            (Setting::InputLog, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "input_log", on);
            }
            (Setting::Learning, SettingValue::Bool(on)) => {
                self.settings.set_bool("general", "learning", on);
            }
            (Setting::SystemTextReplacements, SettingValue::Bool(on)) => {
                self.settings
                    .set_bool("general", "system_text_replacements", on);
            }
            (Setting::ClearInputLog, _) => {
                self.clear_input_log();
                return;
            }
            (Setting::VerboseLog, SettingValue::Bool(on)) => {
                let level = if on { LogLevel::Debug } else { LogLevel::Info };
                self.settings.set_value("general", "log_level", level.key());
            }
            (Setting::CheckUpdateNow, _) => {
                if let Some(updates) = &self.updates {
                    updates.check_now(&self.settings.config().update);
                }
                self.sync_update();
                return;
            }
            (Setting::OpenDownload, _) => {
                open_with_system(&[qingjian_update::DOWNLOAD_URL]);
                return;
            }
            (Setting::OpenWebsite, _) => {
                open_with_system(&[crate::preferences::WEBSITE_URL]);
                return;
            }
            (Setting::OpenRepository, _) => {
                open_with_system(&[crate::preferences::REPOSITORY_URL]);
                return;
            }
            (Setting::OpenLogDirectory, _) => {
                if let Some(dir) = logging::log_dir() {
                    open_with_system(&[&dir.to_string_lossy()]);
                }
                return;
            }
            (Setting::CopyDiagnostics, _) => {
                copy_to_pasteboard(&self.diagnostics());
                self.preferences
                    .set_status("诊断信息已复制到剪贴板，粘贴给作者即可");
                return;
            }
            (Setting::ExportLogs, _) => {
                match logging::export_logs() {
                    Ok(zip) => {
                        open_with_system(&["-R", &zip.to_string_lossy()]);
                        self.preferences
                            .set_status("日志已打包到桌面，发给作者即可");
                    }
                    Err(error) => self
                        .preferences
                        .set_status(&format!("打包日志失败：{error}")),
                }
                return;
            }
            (setting, value) => tracing::warn!(?setting, ?value, "设置项与控件值不匹配"),
        }
        self.apply_config(false);
    }
}

/// 三种指示器颜色各自写哪个配置键。
fn status_bar_color_key(setting: Setting) -> &'static str {
    match setting {
        Setting::StatusBarEnglishColor => "english_color",
        Setting::StatusBarLookupColor => "lookup_color",
        _ => "chinese_color",
    }
}
