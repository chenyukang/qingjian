//! 查询模式（`⌃8`）的壳侧状态：进出、两段之间的切换、云请求的去重与缓存。
//!
//! 规则都收在这里，别的文件只调这些方法 —— 之前散在 controller / cloud 里，每修一个症状就多一处
//! 状态没管好。三段状态各管什么：
//!
//! - `lookup_english`：进来之前是不是英文模式（查询模式里字母当拼音，退出时恢复）；
//! - `lookup_pending`：哪条中文的查询已经在飞（同一条不重复发，终端每敲键重建会话也不怕）；
//! - `lookup_result`：哪条中文查到了什么（含「查到个空」—— 空结果也必须记住，否则会死循环重发）。
//!
//! 引擎侧只看两件事：`lookup_mode`（开关）与 `lookup_source`（挑定的中文，为空即第一段）。
//! 这里的方法负责把界面状态与这两件事对齐。

use crate::host::{Candidate, CloudWord, Host, cloud_candidate};

impl Host {
    /// 进 / 出查询模式。进来时把引擎切到中文那条路（字母要当拼音）、记住原来的中 / 英模式，
    /// 退出时恢复；两种方向都清掉上一次的缓存，并弹一句提示。
    pub fn toggle_lookup(&mut self, on: bool) {
        self.lookup_pending = None;
        self.lookup_result = None;
        if on {
            self.lookup_english = self.engine.english_mode();
            self.engine.set_english_mode(false);
            self.engine.set_lookup_mode(true);
        } else {
            self.engine.set_lookup_mode(false);
            self.engine.set_english_mode(self.lookup_english);
        }
        tracing::info!(on, "查询模式");
        // 桌面那颗悬浮点（`dot`）与菜单栏那一项都要立刻刷新：它们只在切中 / 英、改配置时同步，
        // 不跟这次切换。注意别只刷 `indicator`（那是菜单栏项，用户可能已经关了）
        self.sync_indicator_dot(crate::imk::modifiers::caps_lock_on());
        self.indicator.update();
        let anchor = self.anchor;
        let message = if on {
            "查询模式：打中文，候选给英文写法（空格 / 回车选中即上屏，Esc 退出）"
        } else {
            "已退出查询模式"
        };
        self.show_notice(message, anchor);
    }

    /// 查询模式里选了一条候选：第一段是「就查这条中文」（返回 `true`，不上屏），
    /// 第二段（已经挑定）返回 `false`，交给正常的选词上屏。
    pub fn choose_lookup_chinese(&mut self, index: usize) -> bool {
        if !self.engine.lookup_mode() || self.engine.lookup_source().is_some() {
            return false;
        }
        let Some(candidate) = self.session.candidate(index) else {
            return false;
        };
        tracing::info!(text = %candidate.text, "查询模式：就查这条中文");
        self.lookup_pending = None;
        self.lookup_result = None;
        self.engine.set_lookup_source(&candidate.text);
        true
    }

    /// 第二段的候选从哪里来：本地查不到（`candidates` 为空）时依次看缓存、看是否已经在飞、
    /// 最后才发请求。返回该显示给用户的候选（可能是「查义中…」占位）。
    pub fn lookup_candidates(&mut self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        let Some(source) = self.engine.lookup_source() else {
            return candidates;
        };
        if !candidates.is_empty() {
            return candidates;
        }
        if let Some((cached, words)) = &self.lookup_result
            && *cached == source
        {
            // 查过（含查到空）：直接用，别再问一次 —— 空结果不记的话会一遍遍重发
            return words.clone();
        }
        if self.lookup_pending.as_deref() == Some(source.as_str()) {
            return vec![cloud_candidate("查义中…".to_owned())];
        }
        if self.engine.request_lookup(&source).is_none() {
            return candidates;
        }
        self.lookup_pending = Some(source);
        self.await_prediction();
        vec![cloud_candidate("查义中…".to_owned())]
    }

    /// 云端结果到了（查询模式）：换成英文候选摆进窗口；云端也没有说法就记住「这条查过、是空的」，
    /// 退回第一段并提示换一条。
    pub fn apply_lookup_words(&mut self, words: Vec<CloudWord>) {
        let source = self.engine.lookup_source().unwrap_or_default();
        self.lookup_pending = None;
        if words.is_empty() {
            tracing::info!(%source, "查询模式：云端也没有给出说法");
            self.lookup_result = Some((source.clone(), Vec::new()));
            self.engine.clear_lookup_source();
            let anchor = self.anchor;
            self.show_notice(
                &format!("云端没有「{source}」的说法，换一条中文再查"),
                anchor,
            );
            return;
        }
        let candidates: Vec<Candidate> = words
            .into_iter()
            .map(CloudWord::into_lookup_candidate)
            .collect();
        self.lookup_result = Some((source, candidates.clone()));
        self.reset_session(None, candidates);
        self.render();
    }

    /// 第二段上屏之后的收尾：退回第一段，下一个词重新看中文（缓存留着，同一个词再查是瞬时的）。
    pub fn finish_lookup(&mut self) {
        if !self.engine.lookup_mode() {
            return;
        }
        self.engine.clear_lookup_source();
        self.lookup_pending = None;
    }
}
