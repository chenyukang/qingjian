//! 呈现：删候选、按应用关英文候选、翻译选区的起止、提示气泡、会话重置与候选窗口绘制。

mod notice;
mod translation_job;

pub(super) use notice::Notice;
pub use translation_job::TranslationJob;

use super::cloud::cloud_candidate;
use super::*;
use qingjian_core::SortPreference;

impl Host {
    /// 删掉当前页第 `offset` 格的候选：用户词整个删、词库词清学习；词库里的词删不掉（也没学习记录），
    /// 改记「后置」。返回给用户看的一句话；那格没有候选返回 `None`。
    pub fn forget_candidate(&mut self, offset: usize) -> Option<String> {
        let index = self.session.index_on_page(offset)?;
        let candidate = self.session.candidate(index)?;
        let forgotten = self.engine.forget(&candidate);
        Some(candidate_message(&candidate.text, forgotten))
    }

    /// 隐藏当前页第 `offset` 格的候选：词库里的词以后不再出现；自己学过的词与删候选一样删掉。
    pub fn hide_candidate(&mut self, offset: usize) -> Option<String> {
        let index = self.session.index_on_page(offset)?;
        let candidate = self.session.candidate(index)?;
        let forgotten = self.engine.hide(&candidate);
        Some(candidate_message(&candidate.text, forgotten))
    }

    /// 中 / 英切换（快捷键，缺省 `⌃⇧R`）：切到英文就是纯英文模式（候选只出英文单词，输入框键盘原样）。
    /// 不写配置：这是一次会话里的临时切换，想一直英文就按 Caps Lock。返回新状态与给用户看的一句话。
    /// 把 [`Host::per_app_english`] 落盘（`app-modes.tsv`）。按应用记的状态要跨重启 ——
    /// 这是"每个应用有自己的中 / 英"能兑现的前提。写失败只记一条日志，不影响输入。
    pub fn save_app_modes(&self) {
        let Some(dir) = crate::app::paths::user_data_dir() else {
            return;
        };
        let path = dir.join(APP_MODES_FILE);
        let text = format_app_modes(&self.per_app_english);
        if let Err(error) = std::fs::write(&path, text) {
            tracing::warn!(?path, %error, "按应用的中 / 英状态没写成");
        }
    }

    pub fn toggle_english_mode(&mut self) -> (bool, String) {
        self.english_mode_manual = !self.english_mode_manual;
        let on = self.english_mode_manual;
        // `[apps] per_app_mode`：这个状态记在当前应用名下，切到别的应用不受影响
        if self.per_app_mode
            && let Some(bundle) = self.application.clone()
        {
            self.per_app_english.insert(bundle, on);
            self.save_app_modes();
        }
        // `[status_bar] notice` 关掉就不提示 —— 指示器的颜色本身已经说明模式
        let message = if !self.status_bar.notice {
            String::new()
        } else if on {
            "英文输入".to_owned()
        } else {
            "中文输入".to_owned()
        };
        (on, message)
    }

    /// 切到某个应用（`activateServer`）：按应用记状态的话，把这个应用上次的中 / 英取回来。
    /// `per_app_mode` 关着就什么都不做（全局一个状态）。
    pub fn switch_application(&mut self, bundle: Option<String>) {
        if self.per_app_mode {
            // **离开时先把这个应用的状态存下来**：否则一个应用里切了英文、没去别的应用"确认"过，
            // 这个状态就只在内存里飘着；而 `english_mode_manual` 是全局的那一个值 —— 下一个
            // 应用就"继承"了它，正是用户报的「受上一个应用影响」。
            if let Some(left) = self.application.clone()
                && left != bundle.clone().unwrap_or_default()
            {
                self.per_app_english.insert(left, self.english_mode_manual);
            }
            self.save_app_modes();
        }
        self.application = bundle;
        if !self.per_app_mode {
            return;
        }
        let restored = self
            .application
            .as_ref()
            .and_then(|bundle| self.per_app_english.get(bundle).copied())
            .unwrap_or(false);
        if restored != self.english_mode_manual {
            tracing::info!(
                application = ?self.application,
                english = restored,
                "切应用：恢复该应用自己的中 / 英状态"
            );
            self.english_mode_manual = restored;
        }
    }

    /// 轻拍 Shift 到点了：切换中 / 英。定时器里调，所以不依赖 `TextClient`：
    /// 组句中写状态行（不动手上那串拼音），没组句时在记下的锚点弹一下。
    pub fn tap_toggle_english(&mut self) {
        let (on, message) = self.toggle_english_mode();
        tracing::info!(on, "轻拍 Shift 切换中 / 英");
        let composing = !self.engine.composition().is_empty();
        if !message.is_empty() {
            if composing {
                self.status = Some(message);
            } else {
                let anchor = self.anchor;
                self.show_notice(&message, anchor);
            }
        }
        self.indicator.update();
        self.sync_indicator_dot(crate::imk::modifiers::caps_lock_on());
        self.render();
    }

    /// 切换中文模式下拼音时的英文词候选（快捷键），写回配置（下次启动照旧）。
    /// 返回新状态与给用户看的一句话。
    pub fn toggle_english_in_pinyin(&mut self) -> (bool, String) {
        let on = !self.engine.english_in_pinyin();
        self.engine.set_english_in_pinyin(on);
        self.settings.set_bool("general", "english_in_pinyin", on);
        let message = if !self.status_bar.notice {
            String::new()
        } else if on {
            "英文候选：已开".to_owned()
        } else {
            "英文候选：已关（拼音只出中文，再按一次开启）".to_owned()
        };
        (on, message)
    }

    /// 这个应用里英文模式给不给候选：全局开关开着，且应用不在 `[apps] english_candidates_off` 里。
    pub fn english_candidates_in(&self, bundle: Option<&str>) -> bool {
        self.english_candidates && !bundle.is_some_and(|b| self.apps.english_candidates_off(b))
    }

    /// 开始一次翻译：记下选区，窗口先显示「翻译中…」。调用方已发出请求。
    pub fn begin_translation(
        &mut self,
        range: objc2_foundation::NSRange,
        placeholder: &str,
        original: &str,
        unchanged_notice: &'static str,
    ) {
        self.translation = Some(TranslationJob {
            range,
            result: None,
            original: original.to_owned(),
            unchanged_notice,
        });
        tracing::info!(?self.anchor, "弹框开始显示（锚点）");
        self.reset_session(None, vec![cloud_candidate(placeholder.to_owned())]);
        self.await_prediction();
        self.render();
    }

    /// 在候选窗口里显示一行提示，几秒后自动收起（敲键也收）。
    pub fn show_notice(&mut self, text: &str, anchor: NSRect) {
        self.anchor = anchor;
        self.reset_session(None, vec![cloud_candidate(text.to_owned())]);
        self.render();
        let mtm = MainThreadMarker::new().expect("Host 只在主线程用");
        self.notice = Some(Notice::schedule(mtm));
    }

    /// 收起提示；没在显示就什么都不做。
    pub fn clear_notice(&mut self) {
        if self.notice.take().is_some() && self.translation.is_none() {
            self.reset_session(None, Vec::new());
            self.window.hide();
        }
    }

    /// 翻译结束（接受、放弃或失败）：收窗、停轮询。
    pub fn end_translation(&mut self) {
        if self.translation.take().is_some() {
            self.manual_marks.clear();
            self.cancel_prediction();
            self.reset_session(None, Vec::new());
            self.window.hide();
        }
    }

    /// 横排矩阵这套按键是否生效：开关开着（`[general] horizontal_grid`）而且排布是横排。
    pub fn grid_keys(&self) -> bool {
        self.horizontal_grid && self.layout == LayoutMode::Horizontal
    }

    /// 新一轮候选：每页格数取配置与窗口能画的行数中较小者，云端槽位数取配置。
    pub fn reset_session(&mut self, preedit: Option<Preedit>, candidates: Vec<Candidate>) {
        self.status = None;
        let page_size = self.page_size.min(self.window.max_rows()).max(1);
        self.session
            .reset(preedit, candidates, page_size, self.cloud_slots);
    }

    /// 按会话状态画候选窗口。候选为空且没有 preedit 时收窗。
    pub fn render(&mut self) {
        let size = self.session.layout.page_size();
        let page = self.session.page;
        // 横排展开成矩阵时画视口里的几行，序号只标在高亮所在那一行（数字键选的就是它）；单行时只画当前页
        let grid = self.session.grid_cells();
        let first = self.session.grid.map_or(page, |grid| grid.top()) * size;
        let (cells, columns) = match &grid {
            Some((cells, columns)) => (cells.clone(), *columns),
            None => (self.session.page_cells(), 0),
        };
        let rows: Vec<Row> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let offset = i % size;
                let labelled = columns == 0 || (first + i) / size == page;
                let index = if labelled {
                    (offset + 1).to_string()
                } else {
                    String::new()
                };
                let Some(candidate) = cell.candidate() else {
                    return Row {
                        // 矩阵里的空位什么都不画；单行里的空位留着序号
                        index: if columns == 0 { index } else { String::new() },
                        text: String::new(),
                        annotation: Vec::new(),
                        cloud: false,
                        marks: Vec::new(),
                    };
                };
                let mut row = Row::from_candidate(offset, candidate);
                row.index = index;
                row.cloud = candidate.kind == CandidateKind::Cloud;
                if self.translation.is_some() {
                    // 手动任务（翻译 / 纠错）：窗口里只有结果一个候选，改动的那几段标出来
                    row.marks = self.manual_marks.clone();
                }
                row
            })
            .collect();
        // 页上的译词告诉 Engine：用户上屏那一刻它们在屏幕上，算「见过」（词汇记录）；窗口收起时传空
        self.engine
            .note_displayed(cells.iter().copied().filter_map(Cell::candidate));
        // 配置成只在行内显示时，窗口顶部不画拼音行
        let preedit = self
            .preedit_mode
            .in_window()
            .then(|| self.session.preedit.clone())
            .flatten();
        if rows.is_empty() && self.session.preedit.is_none() {
            self.window.hide();
            return;
        }
        let pages = self.session.pages();
        let footer = (pages > 1).then(|| format!("{}/{pages}", page + 1));
        let frame = Frame {
            preedit,
            rows,
            highlighted: self.session.highlighted.saturating_sub(first),
            columns,
            column_ems: if columns > 0 {
                qingjian_core::Grid::column_ems(&self.session.layout)
            } else {
                Vec::new()
            },
            footer,
            sentence: self.sentence.clone(),
            status: self.status.clone(),
        };
        self.window.show(frame, self.anchor);
    }
}

/// 删候选 / 后置 / 隐藏之后给用户看的那句话。
fn candidate_message(text: &str, forgotten: qingjian_core::Forgotten) -> String {
    if forgotten.user_word {
        format!("已删除用户词「{text}」")
    } else if forgotten.learning {
        format!("已忘掉对「{text}」的学习记录")
    } else {
        match forgotten.preference {
            Some(SortPreference::Down) => {
                format!("「{text}」已后置：以后排在候选最后（再按一次恢复）")
            }
            Some(SortPreference::Hidden) => {
                format!("「{text}」已隐藏：以后不再出现（偏好设置 → 词库 里可恢复）")
            }
            Some(SortPreference::Normal) => format!("「{text}」已恢复正常排序"),
            None => format!("「{text}」是词库里的词，也没有学习记录，没什么可删"),
        }
    }
}

/// 按应用记的中 / 英落盘用的文件名（在用户数据目录里）。
const APP_MODES_FILE: &str = "app-modes.tsv";

/// 解析 `app-modes.tsv`：每行 `bundle<TAB>0|1`。坏行（空行、没制表符、值不是 0/1）直接跳过。
pub fn parse_app_modes(text: &str) -> std::collections::HashMap<String, bool> {
    text.lines()
        .filter_map(|line| {
            let (bundle, value) = line.split_once('\t')?;
            let bundle = bundle.trim();
            if bundle.is_empty() {
                return None;
            }
            match value.trim() {
                "0" => Some((bundle.to_owned(), false)),
                "1" => Some((bundle.to_owned(), true)),
                _ => None,
            }
        })
        .collect()
}

/// 序列化成 `app-modes.tsv` 的正文（按 bundle 排序，方便人看、逐行 diff 稳定）。
pub fn format_app_modes(modes: &std::collections::HashMap<String, bool>) -> String {
    let mut entries: Vec<_> = modes.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut out =
        String::from("# 按应用记住的「中 / 英」：中文 0、英文 1（`[apps] per_app_mode`）\n");
    for (bundle, english) in entries {
        out.push_str(bundle);
        out.push('\t');
        out.push(if *english { '1' } else { '0' });
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod app_modes_tests {
    use super::{format_app_modes, parse_app_modes};
    use std::collections::HashMap;

    /// 落盘再读回来要一模一样；坏行不能把整张表带崩。
    #[test]
    fn app_modes_round_trip_and_bad_lines() {
        let mut modes = HashMap::new();
        modes.insert("dev.warp.Warp-Stable".to_owned(), false);
        modes.insert("com.google.Chrome".to_owned(), true);
        let text = format_app_modes(&modes);
        assert_eq!(parse_app_modes(&text), modes);

        let dirty = "# 注释\n\ncom.google.Chrome\t1\n坏行\tmaybe\ndev.warp.Warp-Stable\t0\n";
        let parsed = parse_app_modes(dirty);
        assert_eq!(parsed.len(), 2);
        assert!(parsed["com.google.Chrome"]);
        assert!(!parsed["dev.warp.Warp-Stable"]);
    }
}
