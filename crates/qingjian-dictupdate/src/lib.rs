//! 词库自动更新：盯上游 release（如 [`ZHWIKI_REPO`]），有新版就把资产下载下来交给壳导入。
//!
//! 为什么是这条路：上一版「抓网络热点、启发式抽词」质量一般（热榜标题又短又吵），
//! 而社区**长期维护、定期发版**的词库（维基词频、网络流行语、维基词典）本来就是更好的
//! 语料 —— 壳负责下载与导入（[`qingjian_dictionary::import`]），这里只管
//! 「上游有没有新版」「该下哪个文件」。
//!
//! 联网在 [`fetch_latest`]；挑文件的逻辑是纯函数（[`pick_asset`]），单测覆盖。

use std::collections::BTreeMap;

use serde::Deserialize;

/// 上游仓库（`fcitx5-pinyin-zhwiki`：维基词频 / 网络流行语 / 维基词典，三本都在这个 release 里）。
pub const ZHWIKI_REPO: &str = "felixonmars/fcitx5-pinyin-zhwiki";

/// 认得出的上游词库：词库名（`dicts/<名字>.qj` 的主干）→ 资产名前缀。
///
/// 只有在这里挂着上游的词库，设置页才给「自动更新」；其他导入词库（自己导的 CEDICT 等）
/// 没有稳定的上游，只能手工更新 —— 不去猜。
pub fn upstream(dict: &str) -> Option<&'static str> {
    match dict {
        "zhwiki" => Some("zhwiki-"),
        "web-slang" => Some("web-slang-"),
        "zhwiktionary" => Some("zhwiktionary-"),
        "zhwikisource" => Some("zhwikisource-"),
        _ => None,
    }
}

/// 这里挂着的所有上游词库名。
pub fn updatable_dicts() -> Vec<&'static str> {
    ["zhwiki", "web-slang", "zhwiktionary", "zhwikisource"].to_vec()
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("请求失败：{0}")]
    Request(#[from] reqwest::Error),

    #[error("运行时启动失败：{0}")]
    Runtime(#[from] std::io::Error),

    #[error("回复解析失败：{0}")]
    Json(#[from] serde_json::Error),
}

/// 上游最新 release 的资产：文件名 → 下载地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// release 标签，如 `0.3.0`。
    pub tag: String,

    /// 资产文件名 → 下载地址（`browser_download_url`）。
    pub assets: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
}

/// 某本词库该下哪个资产：前缀匹配里**名字里日期最大**的那个（`zhwiki-20260416.dict.yaml`）。
///
/// 纯函数，单测覆盖：上游把日期放在文件名里（`zhwiki-20260416`），所以「日期最大的文件名」
/// 就是最新的一版；同名日期取 `.dict.yaml`（Rime 文本，青简的导入器认得）。
pub fn pick_asset<'a>(release: &'a Release, prefix: &str) -> Option<(&'a str, &'a str)> {
    let mut best: Option<(&str, &str)> = None;
    for (name, url) in &release.assets {
        if !name.starts_with(prefix) {
            continue;
        }
        let better = match best {
            None => true,
            Some((current, _)) => asset_order(name) > asset_order(current),
        };
        if better {
            best = Some((name.as_str(), url.as_str()));
        }
    }
    best
}

/// 文件名里的日期（8 位数字）作为排序键；`.dict.yaml` 比 `.dict` 优先（文本格式，导入器认得）。
fn asset_order(name: &str) -> (u32, u8) {
    let date = name
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| part.len() >= 8)
        .filter_map(|part| part[..8].parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let text_preferred = u8::from(name.ends_with(".dict.yaml"));
    (date, text_preferred)
}

/// 读上游最新 release（GitHub API，一小时一次的频率下限额够用）。
pub fn fetch_latest(repo: &str) -> Result<Release, UpdateError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("qingjian-dict-update/1.0")
            .build()?;
        let url = format!("https://api.github.com/repos/{repo}/releases/latest");
        let raw: RawRelease = client.get(url).send().await?.json().await?;
        Ok(Release {
            tag: raw.tag_name,
            assets: raw
                .assets
                .into_iter()
                .map(|asset| (asset.name, asset.browser_download_url))
                .collect(),
        })
    })
}

/// 下载一个资产到内存（词库几 MB 到几十 MB，直接读进内存最简单；写完就交给导入器）。
pub fn download(url: &str) -> Result<Vec<u8>, UpdateError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .user_agent("qingjian-dict-update/1.0")
            .build()?;
        Ok(client.get(url).send().await?.bytes().await?.to_vec())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(names: &[&str]) -> Release {
        Release {
            tag: "0.3.0".to_owned(),
            assets: names
                .iter()
                .map(|name| ((*name).to_owned(), format!("https://example.com/{name}")))
                .collect(),
        }
    }

    #[test]
    fn picks_the_newest_dated_asset_of_that_dictionary() {
        let release = release(&[
            "zhwiki-20251104.dict.yaml",
            "zhwiki-20260416.dict.yaml",
            "zhwiki-20251223.dict.yaml",
            "web-slang-20260416.dict.yaml",
            "zhwiktionary-20260416.dict.yaml",
        ]);
        let (name, url) = pick_asset(&release, "zhwiki-").unwrap();
        assert_eq!(name, "zhwiki-20260416.dict.yaml");
        assert!(url.ends_with(name));
        // 前缀精确：`zhwiki-` 不会挑到 `zhwiktionary-`
        assert_eq!(
            pick_asset(&release, "zhwiktionary-").unwrap().0,
            "zhwiktionary-20260416.dict.yaml"
        );
    }

    #[test]
    fn prefers_the_text_format_when_the_date_is_the_same() {
        let release = release(&["zhwiki-20260416.dict", "zhwiki-20260416.dict.yaml"]);
        assert_eq!(
            pick_asset(&release, "zhwiki-").unwrap().0,
            "zhwiki-20260416.dict.yaml",
            "同一天：Rime 文本格式优先（导入器认得）"
        );
    }

    #[test]
    fn no_asset_means_no_answer() {
        let release = release(&["web-slang-20260416.dict.yaml"]);
        assert!(pick_asset(&release, "zhwiki-").is_none());
    }

    #[test]
    fn only_known_dictionaries_have_an_upstream() {
        assert_eq!(upstream("zhwiki"), Some("zhwiki-"));
        assert!(
            upstream("cedict").is_none(),
            "自己导的词库没有稳定上游，不去猜"
        );
        assert!(upstream("rime-ice").is_none());
    }
}
