#!/usr/bin/env python3
"""从 markdown（Obsidian 库、笔记、文章）抽中文候选词，产出能并进个人词库的清单。

用法：
    extract_zh.py OUT.tsv ROOT [ROOT...] [--all] [--data-dir DIR]

做四件事：
1. 只留正文：去 frontmatter、代码块、行内代码、html、图片、链接地址、数学公式、callout 标记；
   Obsidian 的 [[双链]] 留显示文本（有别名用别名）。
2. jieba 分词，保留含汉字的词（2–12 个汉字）与中英混排词（B站 / C盘）。
3. 统计「出现在多少篇里」——它比总次数更能说明这是你的常用词，不是某篇引用了一次。
4. pypinyin 补拼音（空格分隔音节，与 dict.tsv / user-words.tsv 同格式）。

默认只留「随包词库与个人词库里没有的」词（`--all` 关掉）。输出 TSV：词 \\t 拼音 \\t 出现次数 \\t 篇数。
"""
from __future__ import annotations

import argparse
import os
import pathlib
import re
import sys
from collections import Counter

try:
    import jieba
    from pypinyin import lazy_pinyin
except ImportError as error:
    raise SystemExit(
        f"缺少依赖 {error.name}：先跑 tools/personal-dict/setup.sh 建目录内的虚拟环境，\n"
        f"再用 tools/personal-dict/.venv/bin/python 跑本脚本（其它命令用系统 python3 即可）。"
    ) from error

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import qingjian
from mdclean import plain_text

HAVE_HAN = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]")
MAX_BYTES = 1_000_000  # 单个 md 超过 1MB 的多半是整本书 / 导出文件，跳过并报告
SKIP_DIRS = {".git", "target", "node_modules", ".obsidian", ".trash", ".venv", ".cache"}


def keep(token: str) -> bool:
    """中文路径只收汉字词：≥2 个汉字（单字候选词库本来就有），中英混排算（B站 / C盘）；
    纯英文交给 extract_en.py。"""
    token = token.strip().strip("　")
    if not token or not HAVE_HAN.search(token):
        return False
    return 2 <= len(HAVE_HAN.findall(token)) <= 12


def pinyin_of(word: str) -> str:
    return " ".join(s.lower() for s in (p.strip() for p in lazy_pinyin(word)) if s)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("out", help="输出 TSV")
    parser.add_argument("roots", nargs="+", help="语料目录（递归找 .md）")
    parser.add_argument("--all", action="store_true", help="连词库里已有的词一起输出（默认只留新词）")
    parser.add_argument("--data-dir", default=None, help="青简数据目录（默认 macOS 的 Application Support）")
    args = parser.parse_args()

    known = qingjian.load_known_words(qingjian.data_dir(args.data_dir)) if not args.all else set()
    counts: Counter = Counter()
    docs: Counter = Counter()
    files = skipped = 0
    for root in args.roots:
        for base, dirs, names in os.walk(pathlib.Path(root).expanduser()):
            dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
            for name in names:
                if not name.lower().endswith(".md"):
                    continue
                path = pathlib.Path(base) / name
                try:
                    if path.stat().st_size > MAX_BYTES:
                        skipped += 1
                        continue
                    raw = path.read_text(encoding="utf-8", errors="ignore")
                except OSError:
                    continue
                files += 1
                seen = set()
                for token in jieba.cut(plain_text(raw)):
                    token = token.strip().strip("　")
                    if not keep(token) or token in known:
                        continue
                    if token not in seen:
                        seen.add(token)
                        docs[token] += 1
                    counts[token] += 1
    rows = sorted(counts, key=lambda w: (-docs[w], -counts[w], w))
    with open(args.out, "w", encoding="utf-8") as out:
        out.write("# 词\t拼音\t出现次数\t篇数（从 markdown 抽的中文候选，人工过一遍再并进词库）\n")
        for word in rows:
            out.write(f"{word}\t{pinyin_of(word)}\t{counts[word]}\t{docs[word]}\n")
    print(f"文件 {files} 个" + (f"（跳过 {skipped} 个 >1MB 的）" if skipped else ""))
    print(f"候选 {len(rows)} 个" + ("（已滤掉词库里已有的）" if not args.all else ""))
    print(f"写到 {args.out}")
    print("篇数前 20：", "、".join(f"{w}({docs[w]})" for w in rows[:20]))


if __name__ == "__main__":
    main()
