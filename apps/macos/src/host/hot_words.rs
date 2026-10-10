//! 网络热点词：定期抓公开源（V2EX 热门 / Hacker News / 你自己配的 RSS），抽出新词加进个人词库。
//!
//! 调度不另开定时器：借 [`crate::host::config::ConfigWatch`] 每秒那次 tick（它只在输入法激活时跑），
//! 内部按 `[hot_words] interval_hours` 判断到没到点 —— 设置页改完频率，下一个 tick 就按新设置来。
//! 抓取在后台线程（`fetch_titles` 自建 runtime、会阻塞），结果经 channel 回主线程交给引擎，
//! 和云联想一个路子。拼音由随包 `dict.tsv` 的单字条拼出来（新词当然没现成拼音）。

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qingjian_hotwords::{HotWordsConfig, extract, fetch_titles};

use super::Host;

/// 状态文件：第一行上次跑的时间戳，之后是加过的词（避免重复加）。
const STATE_FILE: &str = "hot-words.tsv";

/// 起线程的阈值：线程跑着就不重复起；这条只是兜底（正常由 channel 收尾）。
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct HotWords {
    /// 上次跑完的时间（unix 秒）。
    last_run: Option<u64>,

    /// 已经加过的词。
    added: Vec<String>,

    /// 后台线程交回来的候选词。
    rx: Option<Receiver<Vec<String>>>,

    /// 起线程的时刻（超时兜底，避免线程挂住后永远不再跑）。
    started_at: Option<SystemTime>,

    /// 单字 → 拼音：新词的音节由它的每个字查出来拼（`送` + `码` → `song ma`）。
    pinyin: HashMap<char, String>,
}

impl HotWords {
    /// 读状态文件（没有就是第一次跑）。
    fn load(data_dir: Option<&std::path::Path>) -> Self {
        let mut hot = Self::default();
        let Some(dir) = data_dir else {
            return hot;
        };
        if let Ok(text) = std::fs::read_to_string(dir.join(STATE_FILE)) {
            for (index, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if index == 0 {
                    hot.last_run = line.parse().ok();
                } else {
                    hot.added.push(line.to_owned());
                }
            }
        }
        hot
    }

    /// 写状态。
    fn save(&self, data_dir: &std::path::Path) {
        let mut text = String::new();
        text.push_str("# 热点词库状态：第一行上次跑的时间戳，之后是加过的词\n");
        text.push_str(&format!("{}\n", self.last_run.unwrap_or(0)));
        for word in &self.added {
            text.push_str(word);
            text.push('\n');
        }
        let _ = std::fs::write(data_dir.join(STATE_FILE), text);
    }

    /// 到点没到点。
    fn due(&self, config: &HotWordsConfig) -> bool {
        let Some(last) = self.last_run else {
            return true;
        };
        now() >= last + config.interval_hours * 3600
    }

    /// 单字的拼音（缺字就返回空，宁可这个字不发音也不猜错）。
    fn syllables(&mut self, word: &str) -> Vec<String> {
        word.chars()
            .filter_map(|c| self.pinyin.get(&c).cloned())
            .collect()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Host {
    /// 启动时准备：读状态 + 从随包词库建单字拼音表。
    pub fn init_hot_words(&mut self) {
        let dir = crate::app::paths::user_data_dir();
        let mut hot = HotWords::load(dir.as_deref());
        if let Ok(path) = crate::app::paths::resource("dict.tsv")
            && let Ok(text) = std::fs::read_to_string(path)
        {
            for line in text.lines() {
                let mut fields = line.split('\t');
                let (Some(word), Some(pinyin)) = (fields.next(), fields.next()) else {
                    continue;
                };
                let mut chars = word.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    hot.pinyin
                        .entry(c)
                        .or_insert_with(|| pinyin.trim().to_owned());
                }
            }
        }
        tracing::debug!(singles = hot.pinyin.len(), "热点词：单字拼音表就绪");
        self.hot_words = hot;
    }

    /// 每次配置 tick 调一次：收后台结果 + 到点就起线程。
    pub fn tick_hot_words(&mut self) {
        // 1) 收结果：后台线程交回来的词，交给引擎记成个人词
        if let Some(rx) = self.hot_words.rx.take() {
            match rx.try_recv() {
                Ok(words) => {
                    self.hot_words.started_at = None;
                    self.add_hot_words(&words);
                }
                Err(TryRecvError::Empty) => {
                    let timed_out = self
                        .hot_words
                        .started_at
                        .is_some_and(|at| at.elapsed().unwrap_or_default() > FETCH_TIMEOUT);
                    if timed_out {
                        tracing::warn!("热点词：抓取超时，先放弃这一轮");
                        self.hot_words.started_at = None;
                    } else {
                        self.hot_words.rx = Some(rx);
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!("热点词：抓取线程没给结果就退了");
                    self.hot_words.started_at = None;
                }
            }
        }

        // 2) 到点就起线程
        if self.hot_words.rx.is_some() {
            return;
        }
        let config = self.settings.config().hot_words.sane();
        if !config.enabled {
            return;
        }
        if !self.hot_words.due(&config) {
            return;
        }
        let already: std::collections::HashSet<String> =
            self.hot_words.added.iter().cloned().collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let config_for_thread = config.clone();
        std::thread::spawn(move || {
            let titles = fetch_titles(&config_for_thread);
            tracing::info!(titles = titles.len(), "热点词：抓到标题");
            // 已知词与「已在词库里」的过滤留给主线程（引擎在主线程）：这里先多取一些
            let words = extract(
                &titles,
                &|_| false,
                &already,
                config_for_thread.max_words * 10,
            );
            let _ = tx.send(words);
        });
        self.hot_words.rx = Some(rx);
        self.hot_words.started_at = Some(SystemTime::now());
        tracing::info!(interval_hours = config.interval_hours, "热点词：开始抓取");
    }

    /// 把抽出来的词过一遍「词库里有没有」，然后把够格的记成个人词。
    fn add_hot_words(&mut self, words: &[String]) {
        let Some(dir) = crate::app::paths::user_data_dir() else {
            return;
        };
        let max = self.settings.config().hot_words.sane().max_words;
        let mut added: Vec<String> = Vec::new();
        for word in words {
            if added.len() >= max {
                break;
            }
            // 已经在个人词库里的交给 learner 自己判重（同样的词+拼音再学一次会直接返回）；
            // 词库里的常词这里不查（那要走拼音反查），最坏结果是给一个已有词加一点权重，无害
            let syllables = self.hot_words.syllables(word);
            if syllables.len() != word.chars().count() {
                tracing::debug!(word, "热点词：有字查不到拼音，这一个跳过");
                continue;
            }
            self.engine.learner_mut().learn_word(word, &syllables);
            added.push(word.clone());
        }
        self.hot_words.last_run = Some(now());
        self.hot_words.added.extend(added.iter().cloned());
        // 只记最近 2000 个，免得状态文件无限长
        let keep = self.hot_words.added.len().saturating_sub(2000);
        self.hot_words.added.drain(..keep);
        self.hot_words.save(&dir);
        if added.is_empty() {
            tracing::info!("热点词：这轮没有新词");
        } else {
            tracing::info!(words = %added.join("、"), "热点词：已加入个人词库");
        }
    }
}
