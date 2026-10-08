#!/usr/bin/env bash
# 建目录内的虚拟环境（tools/personal-dict/.venv），只装 jieba 与 pypinyin。
#
# 只有 extract_zh.py 需要这两个包：系统 python3 里没有、macOS 又不让直接 pip install（PEP 668），
# 所以放一个目录内的 venv。其它命令（extract_en / mark_zh / merge / verify）用系统 python3 就能跑。
set -euo pipefail
cd "$(dirname "$0")"

if [ -x .venv/bin/python ] && .venv/bin/python -c "import jieba, pypinyin" 2>/dev/null; then
    echo "虚拟环境已经就绪：.venv/bin/python"
    exit 0
fi

python3 -m venv .venv
.venv/bin/pip install -q --upgrade pip
.venv/bin/pip install -q -r requirements.txt
echo "装好了：$(pwd)/.venv/bin/python（jieba + pypinyin）"
