#!/usr/bin/env python3
"""给候选词算「新词发现」的两个量：互信息（成词紧密度）与左右邻字熵（搭配自由度）。

- 互信息：min 切分下的 PMI —— 内部结合得越紧越好（「读书笔记」高，「篇文章」低）。
- 左右熵：左邻字 / 右邻字分布的熵取小 —— 左右越自由越像词（「篇文章」右邻被「一/这/几」占住）。

注意事项（实测）：光靠这两个量**分不开**。「读书笔记」整行出现 → 右邻熵只有 0.09；高频黏合的
「篇文章」互信息反而 8.99。想真用起来，得先把「标题 / 列表项」与「正文」分开统计再调参。
这里留作实验工具，扫一眼分布用。

用法：discover_zh.py ROWS.tsv OUT.tsv ROOT [ROOT...]
"""
from __future__ import annotations

import math
import os
import pathlib
import re
import sys
from collections import Counter

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from mdclean import plain_text

HAN = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]")
SKIP_DIRS = {".git", "target", "node_modules", ".obsidian", ".trash", ".venv", ".cache"}


def corpus_ngrams(rows_path: pathlib.Path, roots: list[str], n_max: int = 7):
    """返回 (n 元组计数, 总字数, 候选的左邻字计数, 右邻字计数)。"""
    candidates = set()
    for line in rows_path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("#") and line.strip():
            candidates.add(line.split("\t")[0])
    ngrams: Counter = Counter()
    left: dict[str, Counter] = {}
    right: dict[str, Counter] = {}
    total = 0
    for root in roots:
        for base, dirs, names in os.walk(pathlib.Path(root).expanduser()):
            dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
            for name in names:
                if not name.lower().endswith(".md"):
                    continue
                try:
                    raw = (pathlib.Path(base) / name).read_text(encoding="utf-8", errors="ignore")
                except OSError:
                    continue
                text = plain_text(raw)
                for chunk in re.split(r"[^\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]+", text):
                    if not chunk:
                        continue
                    total += len(chunk)
                    for size in range(1, n_max + 1):
                        for i in range(len(chunk) - size + 1):
                            ngrams[chunk[i : i + size]] += 1
                    for size in range(2, n_max + 1):
                        for i in range(len(chunk) - size + 1):
                            word = chunk[i : i + size]
                            if word not in candidates:
                                continue
                            left.setdefault(word, Counter())[chunk[i - 1] if i > 0 else "^"] += 1
                            j = i + size
                            right.setdefault(word, Counter())[chunk[j] if j < len(chunk) else "$"] += 1
    return ngrams, total, left, right


def pmi(word: str, ngrams: Counter, total: int) -> float:
    whole = ngrams.get(word, 0)
    if whole == 0:
        return 0.0
    best = None
    for i in range(1, len(word)):
        p_w = whole / total
        p_a = ngrams.get(word[:i], 0) / total
        p_b = ngrams.get(word[i:], 0) / total
        value = 0.0 if p_a == 0 or p_b == 0 else math.log2(p_w / (p_a * p_b))
        best = value if best is None else min(best, value)
    return best or 0.0


def entropy(counter: Counter) -> float:
    total = sum(counter.values())
    if total == 0:
        return 0.0
    return -sum((c / total) * math.log2(c / total) for c in counter.values())


def main() -> None:
    rows_path, out_path, *roots = sys.argv[1:]
    ngrams, total, left, right = corpus_ngrams(pathlib.Path(rows_path), roots)
    print(f"总汉字数 {total}，不同 n-gram {len(ngrams)}")
    out = []
    for line in pathlib.Path(rows_path).read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        parts = line.split("\t")
        if len(parts) < 4:
            continue
        word, pinyin, count, docs = parts[0], parts[1], int(parts[2]), int(parts[3])
        out.append((word, pinyin, count, docs,
                    pmi(word, ngrams, total),
                    min(entropy(left.get(word, Counter())), entropy(right.get(word, Counter())))))
    out.sort(key=lambda r: (-r[3], -r[5], -r[4]))
    with open(out_path, "w", encoding="utf-8") as dest:
        dest.write("# 词\t拼音\t出现\t篇数\t互信息(bit)\t左右熵(bit,取小)\n")
        for w, p, c, d, mi, e in out:
            dest.write(f"{w}\t{p}\t{c}\t{d}\t{mi:.2f}\t{e:.2f}\n")
    print(f"写到 {out_path}")


if __name__ == "__main__":
    main()
