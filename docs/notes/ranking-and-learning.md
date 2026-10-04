# 候选排序与学习：本地调优踩过的坑

2026-10-04。一次本地定制里改了不少候选排序与学习行为（大写自造英文词、跨上屏凑整词、汉字+字母
自造词、整句非最优路径、来源感知的精确命中），这里记下**方法、时机陷阱与取材纪律**——
下次动这块先看这篇，能省掉同一批弯路。实现细节与常数在 `crate-notes.md`，排序规则本身在
`crates/qingjian-core/src/ranking/mod.rs` 的模块文档里（带回放数字）。

## 先把尺子用对

- 排序 / 整句 / 纠错的改动，改前改后各跑一次 `qingjian-cli --replay <input-log.jsonl>`（另外
  `--eval-text` 量整句还原）；常数用 `--tune 名=值` 扫，别凭感觉调。
- **冷引擎（不带 `--user-dict`）的数字会骗人**。回放里「第一次选中的词」那一刻还没进学习数据
  （`weight` 为 0），凡是依赖学习状态的规则都会在那一刻表现不同。衡量真实体验**必须带
  `--user-dict ~/Library/Application Support/Qingjian/user.tsv`**；冷热之差才是学习功能的贡献。
- 实例：来源感知排序（导入词库的精确命中降一档）冷引擎 88.8%（比基线低 0.7pp），热引擎
  92.9%（与基线一致）+ 整句前五 +0.4pp —— 差点因为看冷引擎的数字把它当成失败的实验。

## 排序：结构性硬键 vs 得分加分

排序键是**字典序**：结构项（音节数、覆盖字母、简拼数、选过的次数、末音节完整）在前，上下文得分在后。
好处是快且可预期，代价是一个硬键会让「差多少才翻过去」变成不可能。

软化要慎重，两种做法实测结果相反：

- **全软化**（所有精确命中只给加分）：一律变差（词首选 89.5% → 87.7%），而且亏在关键处——
  被高频延伸词顶掉的是**用户自己的词**和常用短词（`qkjm` 的青简、`ifyukh` 的陈于康、`uoe` 的说）。
- **按来源软化**（主词库 / 用户词库 / 用户选过的词保留硬键，导入词库里从没被选过的降一档）：
  热引擎下词首选前五不变，整句前五 +0.4pp。这是现在生效的规则。

教训：**动结构键之前先想清楚「谁被保护、谁被牺牲」**，并用热引擎 A/B 量。

## 学习：三处「时机」陷阱

这三处都不是逻辑错，而是**读状态的时机**错，页面上表现成「学了但没学到」「造出怪词」：

1. `record_word` 内部会 `chain.advance`，把「上一个词」推进到当前词上。想在记完接续后立刻用
   上一个词（或它的音节），必须在**推进之前**取好。踩过：造出 `湘BA = ba ba` 的坏词，
   `try_auto_word` 把 `湘` + `BA` 算成 `BABA`。
2. `log_commit` 会清空 `passthrough_pending`（供日志按真实顺序记直通字符）。想在上屏后用那段
   直通字母，必须提前取出。踩过两次（第一次修完还是漏了一处）。
3. `retain` / 去重会缩短候选列表，之后仍按原来的 `position` 插入就会越界 panic。插完把
   `position` 夹到 `items.len()` 以内。踩过：`extras.rs` 的英文原样候选，热引擎回放直接崩。

## 全拼与双拼的对称性

双拼（与注音）下 `decode()` 有值、`compose.scope()` 是敲的键；全拼下 `decode()` 是 `None`、
`scope` 就是输入本身。凡是「按拼音判断」的规则，两条路都要写：

- 双拼看 `decode()` 解出来的音节（`is_complete` / `marked`）
- 全拼问 `parser::is_syllable` / `parser::segment`

只写一条会漏掉另一半用户。踩过：「整段就是一个音节就不当英文词」的保护只查了 `decode`，
全拼下整条失效，`Ni` 被当成英文词——`apps/linux` 的测试抓到了。

## 验证与取材纪律

- 单测的迷你词表常常切不动真实输入（`oog` 直接 `NoSegmentation`），断言要挑单测吃得下的；
  想看真实行为就去 `/tmp` 起个小工程，用**真实词库 + 用户数据副本**跑（小工程要带
  `[patch.crates-io] cosmic-text = { git = "…/qingjian-team/cosmic-text", rev = … }`，否则 `FontSystem` 编译不过）。
- **别拿用户真实学习数据跑验证脚本**：`FrequencyLearner::from_path(~Library/…/user.tsv)` 会在退出时
  落盘，把测试词写进用户词表 / 英文词表。踩过：污染了 4 条（`inlane` / `WWinlane` / `Xgoogle` / `oogle`）。
  复制一份到 `/tmp` 再跑。
- 改 `~/Library/Application Support/Qingjian/*.tsv` 之前 **先 `pkill -x qingjian-macos`**：输入法每 60 秒
  把内存里的学习数据落盘，不先停会被覆盖。
- 不要 `killall -9 TextInputMenuAgent TextInputSwitcher`（会把输入法栈搞坏）；重启输入法只 `pkill -x qingjian-macos`，
  或重装（`bundle.sh --install` 结尾自己会 kill）。
- 判断跑的是不是新构建：进程启动时间（`ps -o lstart=`）晚于二进制 mtime。
- **别反复杀输入法与输入源 agent，坏了只有注销能修**（2026-10-04 把用户的输入法搞成"只能打英文"）：
  排查中反复 `pkill qingjian-macos`、还杀过 `TextInputMenuAgent` / `TextInputSwitcher`，之后系统在选青简时
  静默退回 ABC——进程能手动拉起、`tisl` 显示输入源已选中、签名有效、启动日志无报错，**从这些检查里看不出问题**，
  只有**注销重登**重建输入法栈才恢复。同类操作（kill 进程、改输入源启用状态、`launchctl kickstart imk*`）请先
  想想有没有只读的确认办法，别在同一台正在用的机器上反复试。
- **`bundle.sh --install` 结尾会 kill 输入法，别以为系统一定会按需自启**：客户端那边输入源仍显示"已激活"
  时不会产生新的激活事件，于是输入法一直不拉起、用户发现"打不出字"（2026-10-04 踩到，用户正在用）。
  装完要么显式启动一次，要么让用户切一次输入源；验证进程活着：`pgrep -x qingjian-macos`。

## 测试范围与提交

- 用 `cargo test --workspace`，别只跑 `-p qingjian-core`：踩过——一个改动让 `apps/linux` 的
  `shift_letter_configuration_does_not_change_caps_or_english` 失败，只跑 core 时看不见。
- 一个文件混多个主题时用 hunk 级暂存（`git apply --cached` 切好的 patch）。切 patch 时**只按新增行匹配
  主题**——上下文行会把邻近主题带进来，归类会失败。测试文件常跨主题，放最后一个提交即可。
- 实验失败的结论也要留在文档里（试过什么、为什么不行、数字多少），否则下一个人会重跑一遍。
  排序那条规则的两次实验就写在 `ranking/mod.rs` 的模块文档里。

## 词库导入与备份

- 导入词库（CC-CEDICT、雾凇这类）的词频要压到**低频带**（`cedict.qj` 用 2–40，雾凇用 8–48），
  补充词能查到但不压过随包常用词；导入前排除已有词库里的词；`.qj` 的 `META` 写清名称、许可、
  署名、来源、版本——第三方词库各带各的许可证，靠的就是这一节。
- 备份（dotr）里排除**生成的产物**（`dicts/rime-ice.qj` 33MB），但把**配方**（生成脚本 + 源 TSV）
  加进 `include`：恢复靠重跑脚本，而不是备一份大文件。
