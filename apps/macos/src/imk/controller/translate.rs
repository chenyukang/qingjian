//! 翻译选中文字、译词键与删候选键。

use super::*;

impl QingjianInputController {
    /// Option+数字：上屏当前页第几个候选的译文（学习和拼音消耗与选那个候选一样）。
    /// 不在组句中时不管；候选没有译文就吞掉按键不动，免得 ¡™£ 进应用。
    /// 翻译应用里选中的文字：云服务关着、密码框、没有选区都不动（键交回应用）。
    /// 翻译选中的文字（`[shortcut] translate_selection`）。
    pub(super) fn translate_selection(&self, client: TextClient<'_>) -> bool {
        self.selection_job(client, SelectionJob::Translate)
    }

    /// 纠错选中的文字（`[shortcut] correct_selection`）：中文改错别字与标点、英文改拼写与语法，不翻译。
    pub(super) fn correct_selection(&self, client: TextClient<'_>) -> bool {
        self.selection_job(client, SelectionJob::Correct)
    }

    /// 翻译 / 纠错共用的流程：读应用里的选区 → 交给云端 → 结果进候选窗口等回车替换。
    fn selection_job(&self, client: TextClient<'_>, job: SelectionJob) -> bool {
        let (label, placeholder, unchanged_notice) = job.texts();
        if !host::with(|h| h.engine.prediction_enabled()).unwrap_or(false) {
            tracing::info!(label, "云服务没开，这个快捷键不生效");
            return false;
        }
        if secure_input::enabled() {
            tracing::debug!("Secure Input 中，不翻译");
            return false;
        }
        let Some((text, range)) = client.selected_text(MAX_TRANSLATE_CHARS) else {
            // 分不清是没选还是应用不给读（不少 Electron 应用不支持），两种情况都提示一下，键吞掉
            tracing::debug!("没有选中的文字，或应用不支持读选区");
            let anchor = mouse_anchor();
            host::with(|h| {
                h.show_notice(
                    &format!("没有选中的文字，或这个应用不支持读取选区（{label}，最多 500 字）"),
                    anchor,
                )
            });
            return true;
        };
        // 弹框锚点：**跟当前鼠标**。三种「问应用要位置」的办法都不靠谱：
        // `caret_rect()` 问第 0 个字符（很多应用返回文档开头）、`firstRectForCharacterRange:`
        // 在不少应用返回视图坐标或第一行、选区矩形的坐标系各家不一。锚在鼠标是最可预期的；
        // 上下翻转与屏幕夹取由候选窗口自己负责
        let anchor = mouse_anchor();
        tracing::info!(
            label,
            chars = text.chars().count(),
            ?anchor,
            "选中文字的云端任务（锚在鼠标）"
        );
        let sent = host::with(|h| {
            h.anchor = anchor;
            match job {
                SelectionJob::Translate => h.engine.request_translation(&text),
                SelectionJob::Correct => h.engine.request_correction(&text),
            }
            .is_some()
        })
        .unwrap_or(false);
        if !sent {
            return false;
        }
        tracing::debug!(chars = text.chars().count(), label, "选中文字交给云端");
        host::with(|h| h.begin_translation(range, placeholder, &text, unchanged_notice));
        true
    }

    /// 翻译窗口开着时的按键：回车 / 空格 / 1 用译文替换选区，Esc 放弃；其他键放弃并交回应用。
    pub(super) fn handle_translation_review(&self, key: u16, client: TextClient<'_>) -> bool {
        let job = host::with(|h| h.translation.clone()).flatten();
        let Some(job) = job else {
            return false;
        };
        tracing::info!(
            key,
            has_result = job.result.is_some(),
            "翻译 / 纠错窗口收到按键"
        );
        match key {
            // 回车 / 小键盘回车 / 空格 / 1：接受。结果还没回来时不能默默吞掉这个键
            // ——用户会以为「回车不接受、空格才接受」（实测反馈），这里明确告诉他还在等
            36 | 76 | 49 | 18 => {
                // 回车系：同一颗键随后还会以 `insertNewline:` 命令送来一次，置标记让它被吃掉
                let from_return = matches!(key, 36 | 76);
                match job.result {
                    Some(result) => {
                        tracing::info!(
                            key,
                            chars = result.chars().count(),
                            location = job.range.location,
                            length = job.range.length,
                            "接受翻译 / 纠错结果（insertText 替换选区）"
                        );
                        client.replace_range(&result, job.range);
                        host::with(|h| {
                            h.end_translation();
                            h.swallow_newline = from_return;
                        });
                    }
                    None => {
                        tracing::debug!("结果还没到，按键先等一等");
                        host::with(|h| {
                            let anchor = h.anchor;
                            h.show_notice("云端还在算，结果回来后再按回车 / 空格", anchor);
                        });
                    }
                }
                true
            }
            // Esc：放弃
            53 => {
                host::with(|h| {
                    h.end_translation();
                    h.swallow_newline = false;
                });
                true
            }
            _ => {
                host::with(|h| h.end_translation());
                false
            }
        }
    }

    pub(super) fn handle_translation_key(
        &self,
        digit: usize,
        sense: usize,
        client: TextClient<'_>,
    ) -> bool {
        let composing = host::with(|h| !h.engine.composition().is_empty()).unwrap_or(false);
        if !composing {
            return false;
        }
        let candidate = host::with(|h| {
            h.session
                .index_on_page(digit - 1)
                .and_then(|index| h.session.candidate(index))
        })
        .flatten();
        let text = candidate
            .and_then(|c| host::with(|h| h.engine.commit_translation(&c, sense)).flatten());
        match text {
            Some(text) => {
                tracing::debug!(%text, "commit translation");
                client.insert_text(&text);
                self.refresh(client);
            }
            None => tracing::debug!(digit, sense, "这个候选没有这条译文"),
        }
        true
    }

    /// 修饰键 + 数字（缺省 ⇧）：删掉当前页第几个候选。不在组句中不管；那格没有候选就吞掉按键不动。
    /// 删完重新查一遍（排序会变），结果那句话显示在拼音行右侧，敲下一键就没了。
    pub(super) fn handle_delete_key(&self, digit: usize, client: TextClient<'_>) -> bool {
        let composing = host::with(|h| !h.engine.composition().is_empty()).unwrap_or(false);
        if !composing {
            return false;
        }
        let Some(message) = host::with(|h| h.forget_candidate(digit - 1)).flatten() else {
            tracing::debug!(digit, "这一格没有候选，没什么可删");
            return true;
        };
        tracing::info!(%message);
        self.refresh(client);
        host::with(|h| h.status = Some(message));
        self.render(client);
        true
    }
}

/// 应用给不出位置时的弹框锚点：当前鼠标位置（零高度，候选窗口会贴着它摆）。
fn mouse_anchor() -> objc2_foundation::NSRect {
    objc2_foundation::NSRect::new(
        objc2_app_kit::NSEvent::mouseLocation(),
        objc2_foundation::NSSize::new(0.0, 0.0),
    )
}

/// 选中文字交给云端的两种任务：翻译（译成学习语言 / 译回中文）与纠错（中文改错别字、英文改拼写语法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionJob {
    Translate,
    Correct,
}

impl SelectionJob {
    /// 日志里用的名字，与候选窗口里的占位文字。
    fn texts(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Translate => (
                "翻译",
                "翻译中…",
                "✓ 检查完毕：原文就是译文（云端没有改动）",
            ),
            Self::Correct => ("纠错", "纠错中…", "✓ 一切完美：没有需要修改的地方"),
        }
    }
}
