#!/usr/bin/env python3
"""从 markdown 抽「英文专用词」：技术词、工具名、库名、项目名、缩写。

和中文那路不同，这里**把代码块也算进语料**（很多技术词只在代码里出现），并标注来源 prose/code。
判定分成两段：

- A 段：随包英文词表里**没有**的词 —— 项目名、缩写、专有名词（CKB / rustc / WasmEdge / tx-pool）
- B 段：词表里有、但你用得**远超英文通用统计**的词 —— 判据是 `Zipf < 3.5` 且出现 ≥ N 篇
  （`Obsidian` 在通用英文里是「黑曜石」（Zipf 2.95），可你写了 91 篇，它对你的意义显然不同）

用法：
    extract_en.py ROOT [ROOT...] --tsv OUT.tsv [--md OUT.md] [--min-docs 3] [--zipf-cut 3.5]
"""
from __future__ import annotations

import argparse
import os
import pathlib
import re
import sys
from collections import Counter

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import qingjian
from mdclean import prose_of, split_code

TOKEN = re.compile(r"[A-Za-z][A-Za-z0-9]*(?:[._'’\-][A-Za-z0-9]+)*")
SKIP = re.compile(
    r"^(?:[0-9a-f]{6,}|[A-Za-z]*[0-9][A-Za-z0-9]*$"
    r"|(?:pub|draft|note|source|author|title|date|tags?|link|url)_[A-Za-z0-9_]+$)"
)
DOMAIN = re.compile(r"^[A-Za-z0-9-]+(?:\.[A-Za-z]{2,}){1,3}$")
MAX_BYTES = 1_000_000
SKIP_DIRS = {".git", "target", "node_modules", ".obsidian", ".trash", ".venv", ".cache"}
# 模板 / 元数据词：Daily 与 Hypothesis 的标题、字段名，不是「专用词」
BOILER = {
    w.lower()
    for w in """notes note todo updated metadata author reference category title group hypo
annotation highlight highlights tags tag date created modified source link pub published url urls site
draft wip inbox journal log reply task tasks start end status priority""".split()
}


def usable(word: str) -> bool:
    if len(word) < 2 or SKIP.match(word):
        return False
    base = word.lower()
    if base in BOILER or DOMAIN.match(base):
        return False
    if len(word) == 2 and word.islower():  # vs / fn / ft 这类代码零碎
        return False
    if "'s" in base or "’s" in base:  # it's / that's 这类缩写不要
        return False
    return True


def load_zipf_table(path: pathlib.Path | None) -> dict[str, float]:
    """随包英文词表：编码 → Zipf（数字越大越常见）。缺省找仓库里的 data/generated/english.tsv。"""
    candidates = [path] if path else []
    candidates += [
        qingjian.repo_root() / "data" / "generated" / "english.tsv",
        qingjian.repo_root() / "assets" / "lexicon" / "english.tsv",
    ]
    for candidate in candidates:
        if not candidate or not candidate.is_file():
            continue
        table = {}
        for line in candidate.read_text(encoding="utf-8", errors="ignore").splitlines():
            if line.startswith("#"):
                continue
            fields = line.split("\t")
            if len(fields) < 3:
                continue
            try:
                table[fields[1].strip().lower()] = int(fields[2]) / 1000
            except ValueError:
                continue
        if table:
            print(f"词表 {candidate}（{len(table)} 词）")
            return table
    print("警告：没找到随包英文词表，A/B 分段会全部落到 A 段")
    return {}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("roots", nargs="+")
    parser.add_argument("--tsv", required=True, help="输出候选 TSV")
    parser.add_argument("--md", default=None, help="同时写一份 Markdown 表格")
    parser.add_argument("--table", default=None, help="随包英文词表（默认自动找 data/generated/english.tsv）")
    parser.add_argument("--min-docs", type=int, default=3, help="至少要出现在多少篇里（默认 3）")
    parser.add_argument("--zipf-cut", type=float, default=3.5, help="B 段的 Zipf 上限（默认 3.5）")
    args = parser.parse_args()

    table = load_zipf_table(pathlib.Path(args.table) if args.table else None)
    counts: Counter = Counter()
    sources: dict[str, Counter] = {}
    casing: dict[str, Counter] = {}
    docs: dict[str, set] = {}
    files = 0
    doc_id = 0
    for root in args.roots:
        for base, dirs, names in os.walk(pathlib.Path(root).expanduser()):
            dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
            for name in names:
                if not name.lower().endswith(".md") or name.lower().endswith(".excalidraw.md"):
                    continue  # Excalidraw 是画图文件，JSON 字段不是你的词汇
                path = pathlib.Path(base) / name
                try:
                    if path.stat().st_size > MAX_BYTES:
                        continue
                    raw = path.read_text(encoding="utf-8", errors="ignore")
                except OSError:
                    continue
                files += 1
                doc_id += 1
                for text, source in ((prose_of(raw), "prose"), (split_code(raw)[1], "code")):
                    for match in TOKEN.finditer(text):
                        word = match.group(0)
                        if not usable(word):
                            continue
                        key = word.lower()
                        casing.setdefault(key, Counter())[word] += 1
                        sources.setdefault(key, Counter())[source] += 1
                        counts[key] += 1
                        docs.setdefault(key, set()).add(doc_id)

    def display(key: str) -> str:
        return casing[key].most_common(1)[0][0] if casing.get(key) else key

    def row_for(key: str):
        return (
            len(docs[key]),
            counts[key],
            "+".join(sorted(s for s, n in sources.get(key, Counter()).items() if n)),
            table.get(key),
        )

    keys = [k for k in counts if len(docs[k]) >= args.min_docs]
    spec = sorted([k for k in keys if table.get(k) is None], key=lambda k: (-len(docs[k]), -counts[k]))
    used = sorted(
        [k for k in keys if table.get(k) is not None and table[k] < args.zipf_cut],
        key=lambda k: (-len(docs[k]), table[k]),
    )
    with open(args.tsv, "w", encoding="utf-8") as out:
        out.write("# 从 markdown 抽的英文候选：A 段=随包词表里没有的专用词；B 段=词表里有但你用得多的\n")
        out.write("# 段\t词\t篇数\t出现次数\t来源（prose/code）\t词表 Zipf（空=词表没有）\n")
        for section, part in (("A", spec), ("B", used)):
            for key in part:
                d, c, src, zipf = row_for(key)
                out.write(f"{section}\t{display(key)}\t{d}\t{c}\t{src}\t{'' if zipf is None else f'{zipf:.2f}'}\n")
    print(f"文件 {files} 个：A 段 {len(spec)} 条（词表没有）、B 段 {len(used)} 条（Zipf < {args.zipf_cut}），"
          f"共 {len(spec) + len(used)} 条（篇数 ≥ {args.min_docs}）")
    print(f"写到 {args.tsv}")
    if args.md:
        with open(args.md, "w", encoding="utf-8") as md:
            md.write("# 青简英文候选词（从 markdown 抽出来）\n\n")
            md.write(f"过滤：不含 `'s / ’s`、不含纯两个小写字母、跳过域名/颜色哈希/模板字段/Excalidraw。\n\n")
            for title, part in (("A. 随包词表里没有（专用词 / 项目名 / 缩写）", spec),
                                (f"B. 词表里有、但你用得多（Zipf < {args.zipf_cut}）", used)):
                md.write(f"## {title}（{len(part)} 条）\n\n| 词 | 篇数 | 出现 | 来源 | Zipf |\n| --- | --- | --- | --- | --- |\n")
                for key in part:
                    d, c, src, zipf = row_for(key)
                    md.write(f"| {display(key)} | {d} | {c} | {src} | {'' if zipf is None else f'{zipf:.2f}'} |\n")
                md.write("\n")
        print(f"Markdown 写到 {args.md}")
    for label, part in (("A 段前 12", spec), ("B 段前 12", used)):
        print(f"{label}：", "、".join(f"{display(k)}({len(docs[k])})" for k in part[:12]))


if __name__ == "__main__":
    main()
