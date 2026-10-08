# 个人词库工具

从**自己的 markdown 语料**（Obsidian 库、笔记、文章）抽候选词，人工过目后并进青简的个人词库。

```
extract_zh.py ── mark_zh.py ──(人工过目)── merge.py zh ── verify.py zh
extract_en.py ──────────────(人工过目)── merge.py en ── verify.py en
discover_zh.py（实验：互信息 / 左右熵，辅助判断成词）
```

## 依赖

只有 **中文抽取**（`extract_zh.py`）需要第三方包（jieba 分词、pypinyin 补拼音）。系统 python3 没有、
macOS 又不让直接 `pip install`（PEP 668），所以给本机建一个仓库外的虚拟环境：

```bash
bash tools/personal-dict/setup.sh        # 建 ~/.venvs/qingjian-dict（jieba + pypinyin）
```

装好之后**直接跑就行**：`extract_zh.py` 发现系统 python3 缺包时会自动用那个 venv 重跑。
其余命令（`extract_en.py` / `mark_zh.py` / `merge.py` / `verify.py`）只用标准库，`python3` 直接跑。

## 中文

```bash
OB=~/code/writing/ob

# 1. 抽候选（只留词库里没有的词），输出：词 拼音 出现次数 篇数
python3 tools/personal-dict/extract_zh.py /tmp/zh-raw.tsv $OB

# 2. 打可疑标记（首尾虚字 / 量词 / 两词连写），无标记的排前面；顺手出一份能读的 md
python3 tools/personal-dict/mark_zh.py /tmp/zh-raw.tsv /tmp/zh-marked.tsv --md ~/Documents/候选词.md

# 3. 人工过目：把不要的行删掉（或保留带标记的某些行）

# 4. 合并：无标记的全收，带标记的默认要 ≥5 篇；先 --dry-run 看一遍
python3 tools/personal-dict/merge.py zh /tmp/zh-marked.tsv --dry-run
python3 tools/personal-dict/merge.py zh /tmp/zh-marked.tsv

# 5. 验证：拿 engine 在副本上查，确认「词 + 拼音」能出候选
python3 tools/personal-dict/verify.py zh /tmp/zh-marked.tsv --limit 9
```

## 英文

```bash
# 1. 抽候选（正文 + 代码块都算），输出分两段：
#    A = 随包英文词表里没有的专用词；B = 词表里有但你用得远超通用统计的（Zipf < 3.5）
python3 tools/personal-dict/extract_en.py ~/code/writing/ob --tsv /tmp/en.tsv --md ~/Documents/英文候选词.md

# 2. 过目 → 合并 → 验证
python3 tools/personal-dict/merge.py en /tmp/en.tsv
python3 tools/personal-dict/verify.py en /tmp/en.tsv
```

## 注意

- **`--user-dict` 接的是 `user.tsv`（词\t次数），不是 `user-words.tsv`**：CLI 会自己去找同目录的
  `user-words.tsv` / `user-english.tsv` / `user-ngram.tsv`，所以整组文件要一起拷（`verify.py` 已经这么做了）。
  传错文件时日志会写 `学习数据里有格式不对的行，已跳过`，候选为 0。
- **查询要用连起来的拼音**：清单里是空格分隔的音节（`yi lin`，词库格式），喂给引擎要拼成 `yilin`。
- **备份**：`merge.py` 写回前把原文件拷到 `/tmp/qj-backup-<时间>-<zh|en>/`。要回退就把文件拷回去，
  再 `pkill -x qingjian-macos` 让输入法重新加载。
- **验证必须用副本**：`qingjian-cli` 退出时会写回用户文件（学习），`verify.py` 已经把它们拷到临时目录再跑，
  别手动指向真实数据目录。
- **全拼**：本机配置是双拼（小鹤），验证时 `verify.py` 固定加 `--shuangpin off` 按全拼查。
- **改完重启输入法**：`pkill -x qingjian-macos`（系统下次用到时会重新拉起）。
- **形状过滤**：中国抽词会跳过 `.excalidraw.md`（画图 JSON 不是词汇）、域名、颜色哈希、模板字段；
  繁体词（`真實`）不做简繁归并，过目时手动删。
- **词频公式**：中文 `min(600, 120 + 篇数×12)`，英文 `min(300, 10 + 篇数×3)` —— 你写得越多的词排序越靠前，
  但不会盖过既有个人词的真实使用次数。
- `discover_zh.py` 是实验工具（互信息 + 左右熵），实测这两个量单独分不开好词与碎片，见文件头注释。

## 文件

| 文件 | 作用 |
| --- | --- |
| `mdclean.py` | markdown → 纯文本（去 frontmatter/代码/链接/批注元数据），中文与英文两路共用 |
| `qingjian.py` | 青简数据文件：定位、读写 `user-words.tsv` / `user-english.tsv`、备份、词频公式 |
| `extract_zh.py` | jieba 分词 + pypinyin 补拼音 → 中文候选 |
| `mark_zh.py` | 中文候选的可疑标记（首尾虚字 / 量词 / 两词连写） |
| `extract_en.py` | 英文专用词（正文 + 代码块，按随包词表的 Zipf 分 A/B 段） |
| `merge.py` | 并入个人词库（备份 + 去重 + 词频） |
| `verify.py` | 用 `qingjian-cli` 在副本上验证候选能不能出得来 |
| `discover_zh.py` | 互信息 / 左右熵（实验） |
| `setup.sh` | 建本机虚拟环境 `~/.venvs/qingjian-dict`（jieba + pypinyin，给 `extract_zh.py` 用） |
