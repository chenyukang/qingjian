#!/usr/bin/env python3
"""把人工过目后的候选清单并进青简个人词库。

用法：
    merge.py zh REVIEW.tsv [--data-dir DIR] [--min-docs 5] [--dry-run]
    merge.py en REVIEW.tsv [--data-dir DIR] [--min-docs 3] [--dry-run]

- `zh`：读 `mark_zh.py` 的输出（词 拼音 次数 篇数 标记）。收「无标记」的全部，加「带标记」里
  篇数 ≥ `--min-docs` 的；词频 = `min(600, 120 + 篇数×12)`。默认跳过 KNOWN_FRAGMENTS 里
  那些上一轮人工挑出来的碎片，`--blocklist FILE` 可以再补。
- `en`：读 `extract_en.py` 的输出（段 词 篇数 次数 来源 Zipf）。收篇数 ≥ `--min-docs` 的 A、B 两段；
  次数 = `min(300, 10 + 篇数×3)`。英文按小写判重，不动已有条目的真实次数。

两个子命令都会先备份到 `/tmp/qj-backup-<时间>-<zh|en>/`，已有词不重复添加。
"""
from __future__ import annotations

import argparse
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import qingjian

# 上一轮人工从中文候选里挑出来的碎片（看着像词、其实跨词），默认不并
KNOWN_FRAGMENTS = {
    "篇文章", "中有", "一个半", "发现自己", "地去", "地说", "很久没", "我花", "写些", "一读",
    "几篇", "时会", "上能", "两篇", "般的", "出块", "一部分的", "第一篇", "第二篇", "第三篇",
}


def load_blocklist(path: str | None) -> set[str]:
    if not path:
        return set()
    return {l.strip() for l in pathlib.Path(path).read_text(encoding="utf-8").splitlines() if l.strip()}


def merge_zh(args: argparse.Namespace) -> None:
    data = qingjian.data_dir(args.data_dir)
    blocklist = KNOWN_FRAGMENTS | load_blocklist(args.blocklist)
    existing = qingjian.read_user_words(data)
    have = {word for word, _, _ in existing}
    added, skipped = [], []
    for line in pathlib.Path(args.src).read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) < 4:
            continue
        word, pinyin, docs = fields[0], fields[1], int(fields[3])
        mark = fields[4] if len(fields) > 4 else ""
        if word in have:
            skipped.append((word, "已在词库里"))
            continue
        if docs < args.min_docs_clean:
            skipped.append((word, f"只出现 {docs} 篇（下限 {args.min_docs_clean}）"))
            continue
        if word in blocklist:
            skipped.append((word, "碎片"))
            continue
        if mark and docs < args.min_docs:
            skipped.append((word, f"带标记且只有 {docs} 篇"))
            continue
        if not pinyin:
            skipped.append((word, "没有拼音"))
            continue
        added.append((word, pinyin, qingjian.zh_frequency(docs)))
    print(f"清单 {len(added) + len(skipped)} 行：要加 {len(added)}，跳过 {len(skipped)}")
    for word, reason in skipped[:5]:
        print(f"  跳过 {word}（{reason}）")
    if args.dry_run:
        print("--dry-run：没有写文件")
        return
    if not added:
        return
    backup = qingjian.backup([data / qingjian.USER_WORDS, data / qingjian.USER_FREQ], label="zh")
    qingjian.write_user_words(data, existing + added)
    print(f"备份 {backup}")
    print(f"{data / qingjian.USER_WORDS}：{len(existing)} → {len(existing) + len(added)} 条")
    print("前 5 条：", "、".join(f"{w} {p} {f}" for w, p, f in added[:5]))


def merge_en(args: argparse.Namespace) -> None:
    data = qingjian.data_dir(args.data_dir)
    existing = qingjian.read_pairs(data / qingjian.USER_ENGLISH) if (data / qingjian.USER_ENGLISH).is_file() else []
    have = {word.lower() for word, _ in existing}
    added, skipped = [], 0
    for line in pathlib.Path(args.src).read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) < 4:
            continue
        word, docs = fields[1], int(fields[2])
        if word.lower() in have:
            skipped += 1
            continue
        if docs < args.min_docs:
            skipped += 1
            continue
        added.append((word, qingjian.en_frequency(docs)))
    print(f"清单：要加 {len(added)}，跳过 {skipped}（已存在 / 篇数 < {args.min_docs}）")
    if args.dry_run:
        print("--dry-run：没有写文件")
        return
    if not added:
        return
    backup = qingjian.backup([data / qingjian.USER_ENGLISH], label="en")
    qingjian.write_pairs(data / qingjian.USER_ENGLISH, existing + added, qingjian.EN_HEADER)
    print(f"备份 {backup}")
    print(f"{data / qingjian.USER_ENGLISH}：{len(existing)} → {len(existing) + len(added)} 条")
    print("前 5 条：", "、".join(f"{w} {n}" for w, n in added[:5]))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("kind", choices=["zh", "en"])
    parser.add_argument("src", help="人工过目的候选 TSV")
    parser.add_argument("--data-dir", default=None)
    parser.add_argument("--min-docs", type=int, default=None, help="带标记的条目至少要多少篇（zh 默认 5，en 默认 3）")
    parser.add_argument("--min-docs-clean", type=int, default=3,
                        help="无标记的条目也至少要这么多篇（zh 默认 3，防往词库里塞只出现过一两次的串）")
    parser.add_argument("--blocklist", default=None, help="额外的碎片黑名单（zh）")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.min_docs is None:
        args.min_docs = 5 if args.kind == "zh" else 3
    (merge_zh if args.kind == "zh" else merge_en)(args)


if __name__ == "__main__":
    main()
