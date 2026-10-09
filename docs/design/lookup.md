# 查询模式：中文想法 → 英文写法（2026-10-09）

## 起因

在英文模式下打字时，常常只知道自己想表达什么、不知道英文怎么写（「敷衍」怎么说？「严谨」是
rigorous 还是 strict？）。这时要么切中文输入法查词典，要么放弃换一个会写的词。要一个快捷键：
按下去进入查询模式，照中文打拼音，候选表直接给英文词，每条带词性与中文解释，选中即上屏。

## 行为

| | |
|---|---|
| 进入 / 退出 | `[shortcut] lookup`（缺省 `⌃8`）切换；`Esc` 退出（丢掉拼音、不上屏） |
| 输入 | 与中文模式一样打拼音（**模式正交**：激活时是中 / 英模式就留在那个模式） |
| 候选 | 英文词，右侧是「词性 + 中文」解释 |
| 选中 | `空格` / `回车` / 数字键 / `⌃⏎` 都是选中即上屏（把英文词写进应用） |
| 上屏后 | 记进 `user-english.tsv`，下次打这个英文前缀就能出（走英文候选那条路） |

## 数据

- **本地（首选，瞬发）**：随包释义表 `glossary-<语言>.tsv` 是**中文词 → 学习语言释义**
  （`敷衍	v. perfunctory`），释义表条目的词直接当候选；解释优先取英→中表
  `glossary-zh.tsv`（`rigorous	adj. 严格的	adj. 严谨的`），查不到就用源词那条
  （`perfunctory v. 敷衍`）—— 保证候选表里总有解释可看。
- **云兜底（句子 / 说法）**：本地一条都查不出来时（「我不太好意思拒绝他」这种整句），
  把中文结果作为 `PredictionKind::Lookup` 交给云端，要 3–6 个英文候选 + 中文解释 + 词性。

## 实现

- Core：`Engine::set_lookup_mode`；`query()` 在 `lookup_mode` 打开时把中文候选换成英文候选
  （`engine/query/lookup_words.rs`）。候选是 `CandidateKind::English`：上屏吃整段作用域、
  记个人英文词表、遗忘也走英文那条路，全都不用另写。
- `annotate` 对英文候选多了「保留候选自带解释」这一步（`or(existing)`）：英→中表没有的词
  也能显示解释。
- CLI：`--lookup` 打开这个模式（验证通路用，`qingjian-cli --lookup --shuangpin off fuyan`）。

## 代码在哪

- Core：`engine/query/lookup_words.rs`（两段的判断与候选构造都在这里：自动挑中文、按挑定的中文
  展开英文候选）、`Engine::set_lookup_mode` / `lookup_source` / `set_lookup_source` /
  `clear_lookup_source` / `lookup_translator`。
- mac 壳：`host/lookup.rs` 收着全部壳侧状态（`lookup_english` 记住原来的中 / 英模式、
  `lookup_pending` 哪条在飞、`lookup_result` 哪条查到了什么——**含查到空**，空结果不记会死循环重发）；
  `toggle_lookup` / `choose_lookup_chinese` / `lookup_candidates` / `apply_lookup_words` /
  `finish_lookup` 五个方法对着一件事，别处只调它们。
- 指示：悬浮指示点在查询模式下换成 `[status_bar] lookup_color`（缺省琥珀 `#D9822B`）并描一圈白边
 （`indicator/mod.rs`）；菜单栏标题带「查」（`menubar/indicator.rs`）；切换时弹一句提示。
  三者互补：悬浮点常显、菜单栏能给全貌（中/英 + 查 + ☁︎）、提示给出解释。
- 挑定的中文与**当时那段拼音**绑定（`lookup_source_scope`）：拼音一变（继续打字、退格）这条选择
  自动作废，回到第一段 —— 否则旧的中文会一直挡着新输入的候选。

## 进展

- Core 与 CLI：已完成（`--lookup` 可验证；CLI 里直接给英文候选，等价于壳里挑定中文之后的第二段）。
- mac 壳：`⌃8` 切换、状态标「查」、`Esc` 退出、空格 / 回车 / 数字 / `⌃⏎` 选中即上屏、
  激活时是什么模式就留在什么模式；设置页「快捷键」页里可录制。
- 云兜底：已完成 —— 本地查不到时候选表先摆一条「查义中…」，把这次查的中文
  （`Engine::lookup_source`，多半是整句那个候选）作为 `PredictionKind::Lookup` 交给云端；
  提示词要 3–6 个英文说法（`text` + `pos` + `gloss`），结果经 `CloudWord::into_lookup_candidate`
  变成英文候选（解释用云端给的中文，英→中表里没有的生僻说法也有解释），选词即上屏。
- 还没做：Windows / Linux 壳同步。
