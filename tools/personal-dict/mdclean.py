#!/usr/bin/env python3
"""markdown → 纯文本：给抽词用的清洗。不依赖 jieba，可以单独测。

两套入口：
- `plain_text()`：只要正文（去 frontmatter、代码、链接、公式、批注元数据），中文抽词用。
- `split_code()`：正文本 + 代码块（含行内代码），英文抽词用 —— 很多技术词只在代码里出现。
"""
import re

FRONTMATTER = re.compile(r"\A---\r?\n.*?\r?\n---\r?\n", re.S)
FENCE_LINE = re.compile(r"^[ \t]*(```|~~~)")
CODE_FENCE = re.compile(r"^[ \t]*(```|~~~)(.*)$")
INLINE_FIELD = re.compile(r"^[ \t]*[A-Za-z][A-Za-z0-9_-]*::[ \t]*\S")
MATH_BLOCK = re.compile(r"\$\$.*?\$\$", re.S)
MATH_INLINE = re.compile(r"(?<!\\)\$[^$\n]{1,80}\$")
INLINE_CODE = re.compile(r"`[^`\n]*`")
HTML = re.compile(r"<[^>\n]{1,200}>")
IMAGE_MD = re.compile(r"!\[[^\]]*\]\([^)]*\)")
LINK_MD = re.compile(r"\[([^\]]*)\]\([^)]*\)")
URL = re.compile(r"https?://\S+")
OBSIDIAN_EMBED = re.compile(r"!\[\[[^\]]*\]\]")
OBSIDIAN_LINK = re.compile(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]")
CALLOUT = re.compile(r"^[ \t]*>[ \t]*\[!\w+\][-+]?[ \t]*", re.M)
ANCHOR = re.compile(r"^\s*(=+|-{3,}|\*{3,}|_{3,})\s*$", re.M)
# Hypothesis 批注格式（Read/Hypo、annotations 两批）：- Reference: / - Category: / 行尾 — Group / - Annotation: …
HYPO_META = re.compile(
    r"^\s*[-*+]\s*(Reference|Category|Updated|Author|Metadata|Group|Notes|Tags?|URLs?|Source|Title|Date|Relevant|Annotated|Id)\s*:",
    re.I,
)
HYPO_MARK = re.compile(r"\s*(—|--)+\s*(Group|Updated on [^—-]*|Annotated on [^—-]*)\s*$")
ANNOTATION = re.compile(r"^\s*[-*+]\s*(Annotation|Highlight|Note|Reply)\s*:\s*", re.I)
CHECKBOX = re.compile(r"^\s*[-*+]\s*\[[ xX]\]\s*")
TAG = re.compile(r"(?<![\w#])#[\w\-/\u4e00-\u9fff]+")


def strip_fences(text: str) -> str:
    """行级剥代码围栏：未闭合的围栏（网摘 / 批注里常见）也一并清掉。"""
    out = []
    open_fence = None
    for line in text.splitlines():
        match = FENCE_LINE.match(line)
        if match:
            fence = match.group(1)
            if open_fence is None:
                open_fence = fence
            elif open_fence == fence:
                open_fence = None
            continue
        if open_fence is None:
            out.append(line)
    return "\n".join(out)


def clean_lines(text: str) -> str:
    """行内元数据：Hypothesis 批注标记、复选框、#tag、`key:: value`。"""
    out = []
    for line in text.splitlines():
        if HYPO_META.match(line):
            continue
        line = ANNOTATION.sub("", line)
        line = CHECKBOX.sub("", line)
        line = HYPO_MARK.sub("", line)
        line = TAG.sub(" ", line)
        out.append(line)
    return "\n".join(out)


def plain_text(raw: str) -> str:
    text = FRONTMATTER.sub("", raw)
    text = strip_fences(text)
    text = clean_lines(text)
    text = MATH_BLOCK.sub(" ", text)
    text = MATH_INLINE.sub(" ", text)
    text = INLINE_CODE.sub(" ", text)
    text = IMAGE_MD.sub(" ", text)
    text = OBSIDIAN_EMBED.sub(" ", text)
    text = OBSIDIAN_LINK.sub(lambda m: m.group(2) or m.group(1), text)
    text = LINK_MD.sub(lambda m: m.group(1), text)
    text = URL.sub(" ", text)
    text = CALLOUT.sub("", text)
    text = "\n".join(l for l in text.splitlines() if not INLINE_FIELD.match(l))
    text = HTML.sub(" ", text)
    text = ANCHOR.sub("", text)
    return text


def split_code(raw: str):
    """返回 (正文, 代码)：代码含围栏块内容与行内代码，正文是剩下的。"""
    body = FRONTMATTER.sub("", raw)
    prose, code = [], []
    open_fence = None
    for line in body.splitlines():
        match = CODE_FENCE.match(line)
        if match:
            if open_fence is None:
                open_fence = match.group(1)
                code.append(match.group(2))
            elif open_fence == match.group(1):
                open_fence = None
            continue
        (code if open_fence is not None else prose).append(line)
    prose_text = "\n".join(prose)
    code += INLINE_CODE.findall(prose_text)
    return INLINE_CODE.sub(" ", prose_text), "\n".join(code)


def prose_of(raw: str) -> str:
    """英文抽词用的正文：比 plain_text 轻（保留 callout 文本与 key: value 行）。"""
    text, _ = split_code(raw)
    text = clean_lines(text)
    text = IMAGE_MD.sub(" ", text)
    text = OBSIDIAN_EMBED.sub(" ", text)
    text = OBSIDIAN_LINK.sub(lambda m: m.group(2) or m.group(1), text)
    text = LINK_MD.sub(lambda m: m.group(1), text)
    return URL.sub(" ", text)
