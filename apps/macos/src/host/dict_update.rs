//! 词库自动更新：按 `[dictionaries] auto_update` 定期看上游有没有新版，有就下载 + 导入。
//!
//! 调度借 `ConfigWatch` 每秒那次 tick（只在输入法激活时跑），内部按 `auto_update_hours`
//! 判到没到点。检查与下载在后台线程（网络会阻塞），导入写文件也在那边；回到主线程只做
//! 一件事：`reload_dictionaries()` —— 让新词库生效。
//!
//! 上游只有一本：fcitx5-pinyin-zhwiki 的 release 里带着 zhwiki / web-slang / zhwiktionary /
//! zhwikisource 四本（`crates/qingjian-dictupdate` 里有认得出的名字表，别的词库不给「自动更新」）。

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qingjian_dictupdate::{ZHWIKI_REPO, download, fetch_latest, pick_asset, upstream};

use super::Host;

/// 状态文件：一行一本 `词库名\t已装的资产名`，加上第一行上次检查的时间。
const STATE_FILE: &str = "dict-update.tsv";

/// 检查 + 下载 + 导入的兜底超时（超过就当这轮失败，下次再来）。
const RUN_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Default)]
pub struct DictUpdate {
    /// 上次检查时间（unix 秒）。
    last_check: Option<u64>,

    /// 每本词库当前装的是哪个资产（`zhwiki-20260416.dict.yaml`）。
    installed: BTreeMap<String, String>,

    /// 后台线程交回来的「更新了哪几本」。
    rx: Option<Receiver<Vec<String>>>,

    /// 起线程的时刻（超时兜底）。
    started_at: Option<SystemTime>,
}

impl DictUpdate {
    fn load(data_dir: Option<&std::path::Path>) -> Self {
        let mut job = Self::default();
        let Some(dir) = data_dir else {
            return job;
        };
        let Ok(text) = std::fs::read_to_string(dir.join(STATE_FILE)) else {
            return job;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.split_once('\t') {
                Some((dict, asset)) => {
                    job.installed.insert(dict.to_owned(), asset.to_owned());
                }
                None => job.last_check = line.parse().ok(),
            }
        }
        job
    }

    fn save(&self, data_dir: &std::path::Path) {
        let mut text = String::new();
        text.push_str("# 词库自动更新状态：一行时间戳，之后是「词库名\t已装的资产名」\n");
        text.push_str(&format!("{}\n", self.last_check.unwrap_or(0)));
        for (dict, asset) in &self.installed {
            text.push_str(&format!("{dict}\t{asset}\n"));
        }
        let _ = std::fs::write(data_dir.join(STATE_FILE), text);
    }

    fn due(&self, hours: u64) -> bool {
        match self.last_check {
            None => true,
            Some(last) => now() >= last + hours.max(1) * 3600,
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Host {
    /// 启动时读状态。
    pub fn init_dict_update(&mut self) {
        let dir = crate::app::paths::user_data_dir();
        self.dict_update = DictUpdate::load(dir.as_deref());
    }

    /// 每次配置 tick 调一次：收后台结果 + 到点就起线程。
    pub fn tick_dict_update(&mut self) {
        // 1) 收结果：更新过的词库重新装配（新 `.qj` 生效）
        if let Some(rx) = self.dict_update.rx.take() {
            match rx.try_recv() {
                Ok(updated) => {
                    self.dict_update.started_at = None;
                    if updated.is_empty() {
                        tracing::info!("词库自动更新：上游没有新版");
                    } else {
                        tracing::info!(dicts = %updated.join("、"), "词库自动更新：已装上新版");
                        self.reload_dictionaries();
                    }
                    self.dict_update.last_check = Some(now());
                    if let Some(dir) = crate::app::paths::user_data_dir() {
                        self.dict_update.save(&dir);
                    }
                }
                Err(TryRecvError::Empty) => {
                    let timed_out = self
                        .dict_update
                        .started_at
                        .is_some_and(|at| at.elapsed().unwrap_or_default() > RUN_TIMEOUT);
                    if timed_out {
                        tracing::warn!("词库自动更新：这轮超时，先放弃");
                        self.dict_update.started_at = None;
                    } else {
                        self.dict_update.rx = Some(rx);
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!("词库自动更新：线程没给结果就退了");
                    self.dict_update.started_at = None;
                }
            }
        }
        if self.dict_update.rx.is_some() {
            return;
        }

        // 2) 到点就起线程
        let config = self.settings.config().dictionaries.clone();
        let wanted: Vec<String> = config
            .auto_update
            .iter()
            .filter(|dict| upstream(dict).is_some())
            .cloned()
            .collect();
        if wanted.is_empty() || !self.dict_update.due(config.auto_update_hours) {
            return;
        }
        let Some(dir) = crate::app::paths::user_data_dir() else {
            return;
        };
        let installed = self.dict_update.installed.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let wanted_for_thread = wanted.clone();
        std::thread::spawn(move || {
            let _ = tx.send(run_update(&wanted_for_thread, &installed, &dir));
        });
        self.dict_update.rx = Some(rx);
        self.dict_update.started_at = Some(SystemTime::now());
        tracing::info!(dicts = %wanted.join("、"), "词库自动更新：开始检查上游");
    }
}

/// 后台线程：查上游 release，挑出每本词库最新的一版，跟已装的比，不一样就下载 + 导入。
/// 返回真正更新了的词库名。
fn run_update(
    wanted: &[String],
    installed: &BTreeMap<String, String>,
    data_dir: &std::path::Path,
) -> Vec<String> {
    let release = match fetch_latest(ZHWIKI_REPO) {
        Ok(release) => release,
        Err(error) => {
            tracing::warn!(%error, "词库自动更新：读上游 release 失败");
            return Vec::new();
        }
    };
    let dicts_dir = data_dir.join("dicts");
    if std::fs::create_dir_all(&dicts_dir).is_err() {
        return Vec::new();
    }
    let mut updated = Vec::new();
    for dict in wanted {
        let Some(prefix) = upstream(dict) else {
            continue;
        };
        let Some((asset, url)) = pick_asset(&release, prefix) else {
            tracing::warn!(dict, "词库自动更新：上游没有这本的资产");
            continue;
        };
        if installed.get(dict).is_some_and(|current| current == asset) {
            continue;
        }
        let bytes = match download(url) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(dict, %error, "词库自动更新：下载失败");
                continue;
            }
        };
        // 导入器按源文件主干命名目标：临时文件叫 `<词库名>.dict.yaml`，出来就是 `<词库名>.qj`
        let staging = data_dir.join("dicts-source");
        if std::fs::create_dir_all(&staging).is_err() {
            continue;
        }
        let temp = staging.join(format!("{dict}.dict.yaml"));
        if std::fs::write(&temp, &bytes).is_err() {
            continue;
        }
        match qingjian_dictionary::import::import(&temp, &dicts_dir) {
            Ok(imported) => {
                tracing::info!(
                    dict,
                    entries = imported.entries,
                    asset,
                    "词库自动更新：已导入"
                );
                updated.push(dict.clone());
            }
            Err(error) => tracing::warn!(dict, %error, "词库自动更新：导入失败"),
        }
    }
    updated
}
