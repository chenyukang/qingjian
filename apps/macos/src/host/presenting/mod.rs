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

    /// 切换中文模式下拼音时的英文词候选（快捷键），写回配置（下次启动照旧）。
    /// 返回新状态与给用户看的一句话。
    pub fn toggle_english_in_pinyin(&mut self) -> (bool, String) {
        let on = !self.engine.english_in_pinyin();
        self.engine.set_english_in_pinyin(on);
        self.settings.set_bool("general", "english_in_pinyin", on);
        let message = if on {
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
                    };
                };
                let mut row = Row::from_candidate(offset, candidate);
                row.index = index;
                row.cloud = candidate.kind == CandidateKind::Cloud;
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
