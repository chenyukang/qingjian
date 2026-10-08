#!/usr/bin/env python3
"""青简个人数据文件：定位、读取、备份、写回。

数据目录默认 `~/Library/Application Support/Qingjian`（macOS），用 `--data-dir` 覆盖。

- `user-words.tsv`   中文个人词：`词\\t拼音\\t词频`（词频越大排序越靠前）
- `user-english.tsv` 英文个人词：`词\\t次数`
- `user.tsv`         学习到的词频：`词\\t次数`（单字频率也会影响整句路径，别乱动）

写回前一律先备份到 `/tmp/`（约定：备份不放在原件旁边、不放配置目录）。
"""
from __future__ import annotations

import pathlib
import shutil
import time

DEFAULT_DATA_DIR = pathlib.Path.home() / "Library/Application Support/Qingjian"
USER_WORDS = "user-words.tsv"
USER_ENGLISH = "user-english.tsv"
USER_FREQ = "user.tsv"
ZH_HEADER = "# 中文个人词：词\t拼音\t词频"
EN_HEADER = "# 青简个人英文词：词\t次数"


def data_dir(override: str | None = None) -> pathlib.Path:
    path = pathlib.Path(override).expanduser() if override else DEFAULT_DATA_DIR
    if not path.is_dir():
        raise SystemExit(f"数据目录不存在：{path}")
    return path


def repo_root() -> pathlib.Path:
    """仓库根（本文件在 tools/personal-dict/ 下）。"""
    return pathlib.Path(__file__).resolve().parents[2]


def lexicon_files(repo: pathlib.Path | None = None) -> list[pathlib.Path]:
    """随包词库源文件：assets/lexicon 下的 dict.tsv、dicts/*.tsv、分音节表、英文表等。"""
    root = (repo or repo_root()) / "assets" / "lexicon"
    return sorted(p for p in root.rglob("*.tsv") if p.is_file())


def load_known_words(data_dir_path: pathlib.Path, repo: pathlib.Path | None = None) -> set[str]:
    """已知词集合：随包词库 + 个人词库（中文按原样，英文按小写）。"""
    known: set[str] = set()
    for path in lexicon_files(repo):
        for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
            if line.startswith("#") or not line.strip():
                continue
            word = line.split("\t")[0].strip()
            if word:
                known.add(word)
    for name in (USER_WORDS, USER_ENGLISH):
        path = data_dir_path / name
        if not path.is_file():
            continue
        for word, _ in read_pairs(path):
            known.add(word)
    return known


def read_pairs(path: pathlib.Path) -> list[tuple[str, int]]:
    """读 `词\\t次数`（或 `词\\t拼音\\t词频`，只取第 1、末列）。"""
    rows = []
    for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = line.split("\t")
        word = fields[0].strip()
        if not word:
            continue
        try:
            value = int(fields[-1].strip()) if len(fields) > 1 else 0
        except ValueError:
            value = 0
        rows.append((word, value))
    return rows


def write_pairs(path: pathlib.Path, rows: list[tuple[str, int]], header: str) -> None:
    path.write_text(header + "\n" + "".join(f"{w}\t{n}\n" for w, n in rows), encoding="utf-8")


def read_user_words(data_dir_path: pathlib.Path) -> list[tuple[str, str, int]]:
    """`user-words.tsv`：词 \\t 拼音 \\t 词频（拼音列缺失时补空）。"""
    rows = []
    path = data_dir_path / USER_WORDS
    if not path.is_file():
        return rows
    for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = line.split("\t")
        word = fields[0].strip()
        if not word:
            continue
        pinyin = fields[1].strip() if len(fields) > 2 else ""
        try:
            freq = int(fields[2].strip()) if len(fields) > 2 else 0
        except ValueError:
            freq = 0
        rows.append((word, pinyin, freq))
    return rows


def write_user_words(data_dir_path: pathlib.Path, rows: list[tuple[str, str, int]]) -> None:
    write_pairs(data_dir_path / USER_WORDS, [(f"{w}\t{p}", f) for w, p, f in rows], ZH_HEADER)


def backup(paths: list[pathlib.Path], label: str = "qingjian-userdata") -> pathlib.Path:
    """把要改的文件备份到 /tmp/qj-backup-<时间>-<label>/。"""
    stamp = time.strftime("%Y%m%d-%H%M%S")
    dest = pathlib.Path("/tmp") / f"qj-backup-{stamp}-{label}"
    dest.mkdir(parents=True, exist_ok=False)
    for path in paths:
        if path.is_file():
            shutil.copy2(path, dest / path.name)
    return dest


def zh_frequency(docs: int) -> int:
    """中文候选的初始词频：你写得越多的词越靠前，上限 600。"""
    return min(600, 120 + docs * 12)


def en_frequency(docs: int) -> int:
    """英文候选的初始次数：上限 300（现有个人英文词的真实次数多在几十，别压过去）。"""
    return min(300, 10 + docs * 3)
