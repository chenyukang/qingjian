#!/usr/bin/env python3
"""用 `qingjian-cli` 验证候选词真能出得来 —— **在副本上跑，绝不碰真实数据**。

`qingjian-cli` 退出时会写回 `--user-dict`（学习），所以先把数据目录里的用户文件拷到临时目录，
再让 CLI 指向副本。

用法：
    verify.py zh WORDS.tsv [--cli PATH] [--config PATH] [--limit 9] [--data-dir DIR]
    verify.py en WORDS.tsv [--cli PATH] [--limit 9]

- `zh`：按清单里的拼音查询（默认全拼，`--shuangpin off`），看词是否出现在前 `--limit` 个候选里。
- `en`：把清单并进一份临时英文词表（随包表 + 这些词），用 `--english-mode` 按前缀补全。
"""
from __future__ import annotations

import argparse
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import qingjian


def default_cli() -> pathlib.Path:
    path = qingjian.repo_root() / "target" / "release" / "qingjian-cli"
    if not path.is_file():
        raise SystemExit(f"找不到 {path}，先 `cargo build --release -p qingjian-cli`")
    return path


def read_rows(path: pathlib.Path) -> list[list[str]]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        rows.append(line.split("\t"))
    return rows


def cli_config(tmp_dir: pathlib.Path, given: str | None) -> str:
    """CLI 的 `--config`：没给就写一份最小的（关掉云联想，否则没配 api key 会直接报错退出）。"""
    if given:
        return given
    path = tmp_dir / "config.toml"
    path.write_text("[predict]\nenabled = false\n", encoding="utf-8")
    return str(path)


def copy_user_data(data: pathlib.Path, tmp_dir: pathlib.Path) -> None:
    """把用户数据拷到临时目录：`--user-dict` 只指 `user.tsv`，同目录的 `user-words.tsv`、
    `user-english.tsv`、`user-ngram.tsv` 由 CLI 自动加载，所以整组一起拷。"""
    for path in data.glob("user*.tsv"):
        shutil.copy2(path, tmp_dir / path.name)


def run_batch(cli: pathlib.Path, extra: list[str], queries: list[str]) -> list[str]:
    """一次跑完所有查询，按 `> 查询` 切成每段输出。"""
    proc = subprocess.run(
        [str(cli), *extra, *queries],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(proc.stderr[-2000:], file=sys.stderr)
        raise SystemExit(f"qingjian-cli 退出码 {proc.returncode}")
    blocks, current = [], []
    for line in proc.stdout.splitlines():
        if line.startswith("> "):
            if current:
                blocks.append("\n".join(current))
            current = [line]
        else:
            current.append(line)
    if current:
        blocks.append("\n".join(current))
    return blocks


def verify_zh(args: argparse.Namespace) -> int:
    data = qingjian.data_dir(args.data_dir)
    # 清单里的拼音是空格分隔的音节（词库格式），查询要连起来（yi lin → yilin）
    rows = [(r[0], r[1].replace(" ", "")) for r in read_rows(args.words) if len(r) >= 2 and r[1].strip()]
    with tempfile.TemporaryDirectory(prefix="qj-verify-") as tmp:
        tmp_dir = pathlib.Path(tmp)
        copy_user_data(data, tmp_dir)
        extra = ["--limit", str(args.limit), "--shuangpin", "off", "--user-dict", str(tmp_dir / qingjian.USER_FREQ)]
        extra += ["--config", cli_config(tmp_dir, args.config)]
        blocks = run_batch(args.cli, extra, [pinyin for _, pinyin in rows])
    return report(rows, blocks, args.limit)


def verify_en(args: argparse.Namespace) -> int:
    data = qingjian.data_dir(args.data_dir)
    rows = [r[1] for r in read_rows(args.words) if len(r) >= 2 and r[1].strip()]
    with tempfile.TemporaryDirectory(prefix="qj-verify-") as tmp:
        tmp_dir = pathlib.Path(tmp)
        copy_user_data(data, tmp_dir)
        extra = ["--limit", str(args.limit), "--english-mode", "--user-dict", str(tmp_dir / qingjian.USER_FREQ)]
        extra += ["--config", cli_config(tmp_dir, args.config)]
        blocks = run_batch(args.cli, extra, [w.lower() for w in rows])
    return report([(w, w) for w in rows], blocks, args.limit)


def report(rows: list[tuple[str, str]], blocks: list[str], limit: int) -> int:
    if len(blocks) != len(rows):
        print(f"注意：查询 {len(rows)} 个，只解析出 {len(blocks)} 段输出", file=sys.stderr)
    misses = []
    for (word, _), block in zip(rows, blocks):
        if word.lower() not in block.lower():  # 候选里的写法可能大小写不同（GitHub / Github）
            misses.append(word)
    print(f"验证 {len(rows)} 个：出现 {len(rows) - len(misses)}，没出现 {len(misses)}")
    if misses:
        print("没出现在前 %d 个候选里的：" % limit, "、".join(misses[:40]))
    return 1 if misses else 0


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("kind", choices=["zh", "en"])
    parser.add_argument("words", help="候选 TSV（merge 用的那份即可）")
    parser.add_argument("--cli", type=pathlib.Path, default=None)
    parser.add_argument("--config", default=None, help="CLI 的 --config（缺省用随包数据）")
    parser.add_argument("--limit", type=int, default=9, help="只看前 N 个候选（默认 9）")
    parser.add_argument("--data-dir", default=None)
    args = parser.parse_args()
    args.cli = args.cli or default_cli()
    args.words = pathlib.Path(args.words)
    sys.exit((verify_zh if args.kind == "zh" else verify_en)(args))


if __name__ == "__main__":
    main()
