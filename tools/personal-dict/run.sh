#!/usr/bin/env bash
# 一键跑完个人词库的更新：抽取 → 自动选词 → 并入 → 验证 → 重启输入法。
#
# 与手工流程的区别：**不等待人工过目**，直接按规则选词：
#   中文：无标记的全收 + 带标记里篇数 ≥ --min-docs-zh（默认 5）的收，跳过已知碎片
#   英文：A 段（随包词表没有的专用词）+ B 段（词表里有但你用得多）里篇数 ≥ --min-docs-en（默认 3）的全收
# 清单（含每条的篇数、来源）留在 --out-dir（默认 ~/Documents/qingjian-词库/<日期>/），事后能翻查；
# 合并前备份在 /tmp/qj-backup-*。删掉某个候选随时可以在输入法里组句时按 ⇧+数字。
#
# 用法：
#   bash tools/personal-dict/run.sh                      # 默认扫 ~/code/writing/ob
#   bash tools/personal-dict/run.sh --corpus ~/notes     # 换语料（可重复给）
#   bash tools/personal-dict/run.sh --dry-run            # 只抽取 + 打印要加什么，不写文件、不重启
#   bash tools/personal-dict/run.sh --zh-only --min-docs-zh 8
set -euo pipefail
cd "$(dirname "$0")/../.."

CORPUS=()
OUT_DIR=""
MIN_DOCS_ZH=5
MIN_DOCS_EN=3
DRY_RUN=0
DO_ZH=1
DO_EN=1
RESTART=1

while [ $# -gt 0 ]; do
    case "$1" in
        --corpus) CORPUS+=("$2"); shift 2 ;;
        --out-dir) OUT_DIR="$2"; shift 2 ;;
        --min-docs-zh) MIN_DOCS_ZH="$2"; shift 2 ;;
        --min-docs-en) MIN_DOCS_EN="$2"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        --zh-only) DO_EN=0; shift ;;
        --en-only) DO_ZH=0; shift ;;
        --no-restart) RESTART=0; shift ;;
        -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
        *) echo "不认识的参数：$1（--help 看用法）" >&2; exit 2 ;;
    esac
done

if [ ${#CORPUS[@]} -eq 0 ]; then
    CORPUS=("$HOME/code/writing/ob")
fi
if [ -z "$OUT_DIR" ]; then
    OUT_DIR="$HOME/Documents/qingjian-词库/$(date +%Y-%m-%d)"
fi
mkdir -p "$OUT_DIR"

PY=python3  # extract_zh.py 缺 jieba 时会自动切到 ~/.venvs/qingjian-dict

echo "语料：${CORPUS[*]}"
echo "清单：$OUT_DIR"
[ "$DRY_RUN" = 1 ] && echo "模式：dry-run（不写个人词库、不重启）"
echo

if [ "$DO_ZH" = 1 ]; then
    echo "[1/4] 抽中文候选"
    $PY tools/personal-dict/extract_zh.py "$OUT_DIR/中文-原始.tsv" "${CORPUS[@]}"
    $PY tools/personal-dict/mark_zh.py "$OUT_DIR/中文-原始.tsv" "$OUT_DIR/中文-带标记.tsv" \
        --md "$OUT_DIR/中文-候选.md"
    echo
    echo "[2/4] 并入中文个人词库（无标记 + 带标记且 ≥ ${MIN_DOCS_ZH} 篇）"
    if [ "$DRY_RUN" = 1 ]; then
        $PY tools/personal-dict/merge.py zh "$OUT_DIR/中文-带标记.tsv" --min-docs "$MIN_DOCS_ZH" --dry-run
    else
        $PY tools/personal-dict/merge.py zh "$OUT_DIR/中文-带标记.tsv" --min-docs "$MIN_DOCS_ZH"
        echo "验证候选能不能出得来："
        $PY tools/personal-dict/verify.py zh "$OUT_DIR/中文-带标记.tsv" --limit 9 --min-docs 3 || true
    fi
    echo
fi

if [ "$DO_EN" = 1 ]; then
    echo "[3/4] 抽英文候选并入"
    $PY tools/personal-dict/extract_en.py "${CORPUS[@]}" --tsv "$OUT_DIR/英文-候选.tsv" \
        --md "$OUT_DIR/英文-候选.md" --min-docs "$MIN_DOCS_EN"
    echo
    echo "[4/4] 并入英文个人词库（A + B 段，篇数 ≥ ${MIN_DOCS_EN}）"
    if [ "$DRY_RUN" = 1 ]; then
        $PY tools/personal-dict/merge.py en "$OUT_DIR/英文-候选.tsv" --min-docs "$MIN_DOCS_EN" --dry-run
    else
        $PY tools/personal-dict/merge.py en "$OUT_DIR/英文-候选.tsv" --min-docs "$MIN_DOCS_EN"
        echo "验证候选能不能出得来："
        $PY tools/personal-dict/verify.py en "$OUT_DIR/英文-候选.tsv" --limit 9 || true
    fi
    echo
fi

if [ "$DRY_RUN" = 1 ]; then
    echo "dry-run 结束：没有改个人词库。去掉 --dry-run 就是真跑。"
    exit 0
fi

if [ "$RESTART" = 1 ]; then
    pkill -x qingjian-macos 2>/dev/null && echo "输入法已重启（系统下次用到时重新拉起）" || echo "输入法没在跑，下次启动时会读到新词库"
fi
echo
echo "完成。清单与日志在 ${OUT_DIR}，回退见 /tmp/qj-backup-*（拷回文件 + pkill -x qingjian-macos）"
echo "提示：自动跑会把「带标记但篇数够」的也收进来，个别不像词的可以事后在输入法里按 ⇧+数字删掉。"
