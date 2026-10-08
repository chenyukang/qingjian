#!/usr/bin/env python3
"""给中文候选打「可疑标记」：首尾虚字 / 量词 / 两词连写。

分词器（jieba）经常会切出「篇文章」「的地去」这种跨词片段，光看篇数分不出来。这一层用几条
笨规则给它们打标，让过目的人先看「无标记」那批：

- 首尾虚字：以「的/了/是/在/个/这…」开头或结尾（多数是片段，也可能是「这个」这类真词）
- 量词：含「个/篇/次/张/条…」（会误伤姓氏——「张宏杰」就有「张」）
- 两词连写：能切出两段、每段都 ≥2 字且都在词库里（「读书笔记」= 读书 + 笔记）

用法：
    mark_zh.py IN.tsv OUT.tsv [--md OUT.md] [--data-dir DIR]
"""
from __future__ import annotations

import argparse
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import qingjian

# 首尾虚字：出现频率高、几乎不能单独成词的字（放首或尾基本是切分碎片）
FUNCTION_CHARS = set("的了是在和与也就都而或被把对给从向为以之其此该些很太更最不没有会能要想说做去来上下中里外时后前又再还只便等并若使让得以于并且但却才始")
# 量词 / 单位：跟在数字后面才成词，单看容易是碎片（也误伤「张」「段」这类姓氏）
MEASURE_CHARS = set("个篇次条只张块件本段句份种点页年月日天时分秒米克斤吨元角位名头匹台辆架艘间层遍趟顿阵场")


def split_into_known(word: str, known: set[str]) -> bool:
    """能切成两段、每段 ≥2 字且在词库里（「读书笔记」这类两词连写）。"""
    for i in range(2, len(word) - 1):
        left, right = word[:i], word[i:]
        if len(right) < 2:
            break
        if left in known and right in known:
            return True
    return False


def marks_of(word: str, known: set[str]) -> list[str]:
    marks = []
    if word[0] in FUNCTION_CHARS or word[-1] in FUNCTION_CHARS:
        marks.append("首尾虚字")
    if any(ch in MEASURE_CHARS for ch in word):
        marks.append("量词")
    if split_into_known(word, known):
        marks.append("两词连写")
    return marks


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("src", help="extract_zh.py 的输出")
    parser.add_argument("out", help="带标记的 TSV（给人工过目）")
    parser.add_argument("--md", default=None, help="同时写一份 Markdown 表格")
    parser.add_argument("--data-dir", default=None)
    args = parser.parse_args()

    known = qingjian.load_known_words(qingjian.data_dir(args.data_dir))
    rows = []
    for line in pathlib.Path(args.src).read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) < 4:
            continue
        word, pinyin, count, docs = fields[0], fields[1], int(fields[2]), int(fields[3])
        rows.append((word, pinyin, count, docs, "、".join(marks_of(word, known))))
    # 无标记的排前面：它们最像词
    rows.sort(key=lambda r: (bool(r[4]), -r[3], -r[2]))
    with open(args.out, "w", encoding="utf-8") as out:
        out.write("# 中文候选词：词\t拼音\t出现次数\t篇数\t可疑标记（空 = 看起来像词）\n")
        for word, pinyin, count, docs, mark in rows:
            out.write(f"{word}\t{pinyin}\t{count}\t{docs}\t{mark}\n")
    clean = [r for r in rows if not r[4]]
    print(f"{len(rows)} 个候选：无标记 {len(clean)}，带标记 {len(rows) - len(clean)}")
    print(f"写到 {args.out}")
    if args.md:
        with open(args.md, "w", encoding="utf-8") as md:
            md.write("# 青简中文候选词（从 markdown 抽出来）\n\n")
            md.write("用法：删掉不要的行，剩下的并进个人词库。**无标记**的排前面，最像词。\n\n")
            md.write("| 词 | 拼音 | 出现 | 篇数 | 可疑标记 |\n| --- | --- | --- | --- | --- |\n")
            for word, pinyin, count, docs, mark in rows:
                md.write(f"| {word} | {pinyin} | {count} | {docs} | {mark} |\n")
        print(f"Markdown 写到 {args.md}")


if __name__ == "__main__":
    main()
