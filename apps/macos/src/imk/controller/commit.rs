//! 上屏：高亮候选、指定候选、拼音原样。

use super::*;

impl QingjianInputController {
    pub(super) fn commit_highlighted(&self, client: TextClient<'_>) -> bool {
        let index = host::with(|h| h.session.highlighted).unwrap_or(0);
        self.commit_index(index, client)
    }

    /// 上屏第 `index` 个候选；没有候选时上屏拼音本身。上屏后剩余拼音继续组句。
    pub(super) fn commit_index(&self, index: usize, client: TextClient<'_>) -> bool {
        // 查询模式第一段：候选是中文，选它不是上屏，而是「就查这一句」（状态与提示都在 host/lookup.rs）
        if host::with(|h| h.choose_lookup_chinese(index)).unwrap_or(false) {
            self.refresh(client);
            return true;
        }
        let candidate = host::with(|h| h.session.candidate(index)).flatten();
        let Some(candidate) = candidate else {
            if host::with(|h| index < h.session.layout.len()).unwrap_or(false) {
                return true;
            }
            return self.commit_raw(client);
        };
        let Some(text) = host::with(|h| h.engine.commit(&candidate)) else {
            return false;
        };
        tracing::debug!(%text, "commit");
        // 查询模式：这条查完了就退回第一段（下一个词重新看中文）
        host::with(|h| h.finish_lookup());
        client.insert_text(&text);
        self.refresh(client);
        true
    }

    /// 把拼音原样上屏并清空。缓冲区为空时返回 false。
    pub(super) fn commit_raw(&self, client: TextClient<'_>) -> bool {
        let Some(raw) = host::with(|h| h.engine.take_raw()) else {
            return false;
        };
        if raw.is_empty() {
            return false;
        }
        tracing::debug!(%raw, "commit raw");
        client.insert_text(&raw);
        self.refresh(client);
        true
    }
}
