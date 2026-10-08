#!/usr/bin/env bash
# 给个人词库工具装一个本机虚拟环境：~/.venvs/qingjian-dict（只装 jieba + pypinyin）。
#
# 只有中文抽取（extract_zh.py）需要这两个包：系统 python3 没有、macOS 又不让直接 pip install
# （PEP 668），所以单独建一个 venv 放在仓库外。extract_zh.py 找不到包时会自动用这个 venv 重跑，
# 平时直接 `python3 tools/personal-dict/extract_zh.py ...` 就行。
#
# 换位置：QINGJIAN_DICT_VENV=/path/to/venv bash tools/personal-dict/setup.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

VENV="${QINGJIAN_DICT_VENV:-$HOME/.venvs/qingjian-dict}"

if [ -x "$VENV/bin/python" ] && "$VENV/bin/python" -c "import jieba, pypinyin" 2>/dev/null; then
    echo "虚拟环境已经就绪：$VENV/bin/python"
    exit 0
fi

mkdir -p "$(dirname "$VENV")"
python3 -m venv "$VENV"
"$VENV/bin/pip" install -q --upgrade pip
"$VENV/bin/pip" install -q -r tools/personal-dict/requirements.txt
echo "装好了：$VENV/bin/python（jieba + pypinyin）"
