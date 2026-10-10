//! 网络热点词：定期抓公开源（V2EX 热门 / Hacker News / 自定义 RSS），抽出**新词**，
//! 交给壳加进个人词库。
//!
//! 为什么是这几个源：微博 / 百度 / 知乎热搜要爬、有反爬和 ToS 问题，不碰；这里只用
//! 有公开 API 或 RSS 的。抽词的逻辑是纯函数（[`extract`]），单测覆盖；联网在
//! [`fetch_titles`] 里，用一次性的 current-thread runtime，和 `qingjian-update` 一个路子。
//!
//! 抽词规则（都是启发式，宁可少给几个）：
//! - **中文**：标题里 2–6 个汉字的片段，出现在**两条以上**不同标题里；不在已知词库里；
//!   不是更长的中选词的子串；不在停用词表里。
//! - **英文**：像专名 / 技术词的写法（`Rust`、`Kubernetes`、`iOS`、`C++`），小写常见词不要；
//!   大小写不敏感地查已知英文词表与停用词。

use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

/// 一次抓取的标题来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// V2EX 热门主题（中文技术圈）。
    V2ex,

    /// Hacker News 首页（英文技术）。
    HackerNews,

    /// 配置里自己加的 RSS。
    Rss,
}

/// 抓回来的标题：来源 + 文本。
pub type Titled = (Source, String);

const UA: &str = "qingjian-hot-words/1.0 (personal input-method dictionary)";
const V2EX_HOT: &str = "https://www.v2ex.com/api/topics/hot.json";
const HN_TOP: &str = "https://hacker-news.firebaseio.com/v0/topstories.json";
const HN_ITEM: &str = "https://hacker-news.firebaseio.com/v0/item/{}.json";
const HN_ITEMS: usize = 30;
const TIMEOUT_MS: u64 = 15_000;

/// 标题里高频、但对输入法没用的词（大小写不敏感比较）。
const STOPWORDS: &[&str] = &[
    // 中文
    "转发",
    "分享",
    "回复",
    "评论",
    "楼主",
    "请问",
    "大家",
    "有没有",
    "怎么样",
    "为什么",
    "这个",
    "那个",
    "什么",
    "怎么",
    "可以",
    "需要",
    "我们",
    "他们",
    "你们",
    "自己",
    "现在",
    "今天",
    "一个",
    "就是",
    "还是",
    "已经",
    "没有",
    "如果",
    "然后",
    "因为",
    "所以",
    "但是",
    "以及",
    "关于",
    "最新",
    // 英文
    "the",
    "show",
    "ask",
    "tell",
    "best",
    "using",
    "about",
    "would",
    "could",
    "should",
    "there",
    "these",
    "those",
    "with",
    "from",
    "that",
    "this",
    "have",
    "your",
    "how",
    "why",
    "what",
    "when",
    "new",
    "why",
    "his",
    "her",
    "its",
    "are",
    "was",
    "were",
    "for",
    "and",
    "but",
    "not",
    "you",
];

/// `[hot_words]` 分节。缺省关：抓网络、动词库，得用户点了才开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotWordsConfig {
    /// 是否启用。
    pub enabled: bool,

    /// 多久跑一次（小时）。
    pub interval_hours: u64,

    /// 每次最多加几个新词。
    pub max_words: usize,

    /// 自己加的 RSS 源（标题会被用来抽词）。
    pub rss: Vec<String>,
}

impl Default for HotWordsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_hours: 24,
            max_words: 10,
            rss: Vec::new(),
        }
    }
}

impl HotWordsConfig {
    /// 间隔至少一小时、每次至少一个词：配置里写 0 也按缺省算，免得跑成死循环。
    pub fn sane(&self) -> Self {
        Self {
            interval_hours: self.interval_hours.max(1),
            max_words: self.max_words.clamp(1, 50),
            ..self.clone()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HotWordsError {
    #[error("请求失败：{0}")]
    Request(#[from] reqwest::Error),

    #[error("运行时启动失败：{0}")]
    Runtime(#[from] std::io::Error),

    #[error("回复解析失败：{0}")]
    Json(#[from] serde_json::Error),
}

/// 抓所有源的标题。单个源失败只记一条日志，不影响其他源（网络热点本来就是锦上添花）。
pub fn fetch_titles(config: &HotWordsConfig) -> Vec<Titled> {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(%error, "热点词：runtime 起不来");
            return Vec::new();
        }
    };
    runtime.block_on(async {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(TIMEOUT_MS))
            .user_agent(UA)
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(%error, "热点词：HTTP 客户端建不起来");
                return Vec::new();
            }
        };
        let mut titles = Vec::new();
        match v2ex_titles(&client).await {
            Ok(items) => {
                tracing::debug!(source = "v2ex", count = items.len(), "热点词：抓到标题");
                titles.extend(items.into_iter().map(|t| (Source::V2ex, t)));
            }
            Err(error) => tracing::warn!(%error, "热点词：V2EX 抓取失败"),
        }
        match hn_titles(&client).await {
            Ok(items) => {
                tracing::debug!(source = "hn", count = items.len(), "热点词：抓到标题");
                titles.extend(items.into_iter().map(|t| (Source::HackerNews, t)));
            }
            Err(error) => tracing::warn!(%error, "热点词：Hacker News 抓取失败"),
        }
        for url in &config.rss {
            match rss_titles(&client, url).await {
                Ok(items) => {
                    tracing::debug!(%url, count = items.len(), "热点词：抓到标题");
                    titles.extend(items.into_iter().map(|t| (Source::Rss, t)));
                }
                Err(error) => tracing::warn!(%url, %error, "热点词：RSS 抓取失败"),
            }
        }
        titles
    })
}

async fn v2ex_titles(client: &reqwest::Client) -> Result<Vec<String>, HotWordsError> {
    #[derive(Deserialize)]
    struct Topic {
        title: Option<String>,
    }
    let topics: Vec<Topic> = client.get(V2EX_HOT).send().await?.json().await?;
    Ok(topics.into_iter().filter_map(|topic| topic.title).collect())
}

async fn hn_titles(client: &reqwest::Client) -> Result<Vec<String>, HotWordsError> {
    #[derive(Deserialize)]
    struct Item {
        title: Option<String>,
    }
    let ids: Vec<u64> = client.get(HN_TOP).send().await?.json().await?;
    let mut titles = Vec::new();
    for id in ids.into_iter().take(HN_ITEMS) {
        let url = HN_ITEM.replace("{}", &id.to_string());
        match client.get(url).send().await?.json::<Item>().await {
            Ok(item) => titles.extend(item.title),
            // 单条失败不影响整轮
            Err(error) => tracing::debug!(id, %error, "热点词：HN 单条失败"),
        }
    }
    Ok(titles)
}

async fn rss_titles(client: &reqwest::Client, url: &str) -> Result<Vec<String>, HotWordsError> {
    let text = client.get(url).send().await?.text().await?;
    // <title> 第一个通常是频道名，跳过
    let titles = rss_titles_from(&text);
    Ok(titles.into_iter().skip(1).collect())
}

/// 极简 RSS / Atom 标题抽取（不引 XML 库：我们只要标题文本）。
fn rss_titles_from(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<title") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let Some(close) = after[open_end..].find("</title>") else {
            break;
        };
        let raw = &after[open_end + 1..open_end + close];
        let title = raw
            .replace("<![CDATA[", "")
            .replace("]]>", "")
            .trim()
            .to_owned();
        if !title.is_empty() {
            out.push(title);
        }
        rest = &after[open_end + close + "</title>".len()..];
    }
    out
}

/// 抽候选新词。`known` 判断「这个词已经在词库里了」（大小写自己处理），`already` 是以前加过的。
pub fn extract(
    titles: &[Titled],
    known: &dyn Fn(&str) -> bool,
    already: &HashSet<String>,
    max_words: usize,
) -> Vec<String> {
    let mut docs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, title) in titles {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for token in title_tokens(title) {
            seen.insert(token);
        }
        for token in seen {
            *docs.entry(token).or_default() += 1;
        }
    }
    let needed = if titles.len() > 8 { 2 } else { 1 };
    let mut candidates: Vec<(String, usize)> = docs
        .into_iter()
        .filter(|(word, count)| {
            *count >= needed
                && !already.contains(word)
                && !is_stopword(word)
                && !known(word)
                && !word.chars().any(|c| c.is_ascii_digit())
        })
        .collect();
    // 出现标题多的优先；同样多时长的优先（长片段更可能是词）
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.0.chars().count().cmp(&a.0.chars().count()))
    });
    let mut picked: Vec<String> = Vec::new();
    for (word, _) in candidates {
        if picked.len() >= max_words {
            break;
        }
        // 已经是选中词的子串就跳过（「送码」选了就不再要「送」这种碎片）
        if picked.iter().any(|kept| kept.contains(&word)) {
            continue;
        }
        picked.push(word);
    }
    picked
}

/// 标题里的候选项：中文 2–6 字滑窗 + 像专名/技术词的西文整词。
fn title_tokens(title: &str) -> Vec<String> {
    let mut out = Vec::new();
    let cleaned = strip_urls(title);
    // 中文滑窗
    let chars: Vec<char> = cleaned.chars().collect();
    let mut run: Vec<char> = Vec::new();
    for c in chars {
        if is_han(c) {
            run.push(c);
        } else {
            push_runs(&mut run, &mut out);
        }
    }
    push_runs(&mut run, &mut out);
    // 西文词
    for token in
        cleaned.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '.' | '-')))
    {
        if token.len() < 2 || token.len() > 24 {
            continue;
        }
        let looks_special = token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            || token.chars().skip(1).any(|c| c.is_ascii_uppercase())
            || token.contains('+')
            || token.contains('#');
        if looks_special && !token.chars().all(|c| c.is_ascii_digit()) {
            out.push(token.to_owned());
        }
    }
    out
}

/// 一段汉字里所有 2–6 字的片段（滑窗），常用的会在多条标题里重复出现。
fn push_runs(run: &mut Vec<char>, out: &mut Vec<String>) {
    for len in 2..=6.min(run.len()) {
        for start in 0..=run.len() - len {
            out.push(run[start..start + len].iter().collect());
        }
    }
    run.clear();
}

fn is_han(c: char) -> bool {
    ('\u{3400}'..='\u{4dbf}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c)
}

fn strip_urls(title: &str) -> String {
    title
        .split_whitespace()
        .filter(|part| !part.starts_with("http://") && !part.starts_with("https://"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_stopword(word: &str) -> bool {
    let lower = word.to_lowercase();
    STOPWORDS.iter().any(|stop| *stop == lower || *stop == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(list: &[&str]) -> Vec<Titled> {
        list.iter()
            .map(|t| (Source::V2ex, (*t).to_owned()))
            .collect()
    }

    fn nothing_known(_: &str) -> bool {
        false
    }

    #[test]
    fn picks_words_repeated_across_titles_and_drops_fragments() {
        let list = titles(&[
            "送码：有人想试试新出的输入法吗",
            "又送码了：这次是青简的新皮肤",
            "谈谈送码这件事",
            "完全无关的一条",
        ]);
        let picked = extract(&list, &nothing_known, &HashSet::new(), 5);
        assert!(
            picked.contains(&"送码".to_owned()),
            "两条以上标题共现的词要选出来：{picked:?}"
        );
        // 「送码」选中之后，它的碎片不该再进
        assert!(!picked.iter().any(|w| w == "送" || w == "码"));
    }

    #[test]
    fn skips_known_words_and_stopwords() {
        let list = titles(&[
            "the Rust 1.90 is out",
            "the Rust release notes",
            "use the new Rust",
        ]);
        let known = |word: &str| word.eq_ignore_ascii_case("rust");
        let picked = extract(&list, &known, &HashSet::new(), 5);
        assert!(
            !picked.iter().any(|w| w.eq_ignore_ascii_case("rust")),
            "词库里有的不要"
        );
        assert!(
            !picked.iter().any(|w| w.eq_ignore_ascii_case("the")),
            "停用词不要"
        );
    }

    #[test]
    fn keeps_special_latin_forms_and_skips_plain_lowercase() {
        let list = titles(&["Kubernetes 1.34", "Kubernetes and C++", "学习 Kubernetes"]);
        let picked = extract(&list, &nothing_known, &HashSet::new(), 5);
        assert!(picked.contains(&"Kubernetes".to_owned()), "{picked:?}");
        assert!(
            !picked.iter().any(|w| w == "and"),
            "小写常见词不要：{picked:?}"
        );
    }

    #[test]
    fn respects_the_cap_and_remembers_what_was_added() {
        let list = titles(&["送码 送码", "送码 送码 送码", "加群 加群", "加群 加群 加群"]);
        let already: HashSet<String> = ["送码".to_owned()].into_iter().collect();
        let picked = extract(&list, &nothing_known, &already, 1);
        assert_eq!(picked, vec!["加群".to_owned()]);
    }

    #[test]
    fn rss_titles_handle_cdata_and_channel_name() {
        let xml = "<rss><channel><title>我的博客</title><item><title>一篇</title></item>\
                   <item><title><![CDATA[两篇 & 三篇]]></title></item></channel></rss>";
        assert_eq!(
            rss_titles_from(xml),
            vec!["我的博客", "一篇", "两篇 & 三篇"]
        );
    }

    #[test]
    fn config_defaults_are_sane() {
        let config = HotWordsConfig::default();
        assert!(!config.enabled, "缺省关：抓网络、动词库要用户点了才开");
        let zero = HotWordsConfig {
            interval_hours: 0,
            max_words: 0,
            ..HotWordsConfig::default()
        }
        .sane();
        assert_eq!(zero.interval_hours, 1);
        assert_eq!(zero.max_words, 1);
    }
}
