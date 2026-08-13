#!/usr/bin/env python3
"""Bóc tần suất âm tiết (và câu văn xuôi) tiếng Việt từ dump Wikipedia.

Đầu ra chính là TSV `âm_tiết<TAB>tần_suất` sắp giảm dần — dùng cho ba việc:

1. **Verify bảng ngữ âm**: âm tiết nào xuất hiện nhiều trong corpus mà
   `phonology.rs` loại bỏ thì bảng vần/âm đầu của ta còn thiếu. Đây là vòng kiểm
   chứng mà PLAN.md yêu cầu — không có nó, bảng ngữ âm chỉ là phỏng đoán.
   (Vòng này đã tìm ra bug nhập chữ khiến `gì`, `gìn`, `quỳnh` bị loại oan.)

2. **Đo false-positive rate** (`--sentences-out`): xuất câu văn xuôi để chạy
   `writa-cli scan`. Đây là chỉ tiêu quan trọng nhất của cả dự án.

3. **Dựng từ điển L2 + n-gram LM** (P1): cùng một lượt đọc corpus.

Chỉ dùng stdlib (`bz2`, `re`, `unicodedata`) — không thêm dependency, không cần
biên dịch C. Dump 1,09 GB nén giải ra khoảng 5 GB nên script chạy theo kiểu
streaming, không giữ toàn văn trong RAM.

Cách chạy:

    py scripts/extract_syllables.py                    # toàn bộ dump, ~30 phút
    py scripts/extract_syllables.py --limit-mb 200     # smoke test nhanh
    py scripts/extract_syllables.py --limit-mb 300 \\
        --sentences-out data/raw/sentences.txt         # kèm câu để đo FP
"""

from __future__ import annotations

import argparse
import bz2
import html
import re
import sys
import time
import unicodedata
from collections import Counter
from pathlib import Path
from typing import Iterator, TextIO

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DUMP = ROOT / "data" / "raw" / "viwiki-latest-pages-articles.xml.bz2"
DEFAULT_OUT = ROOT / "data" / "raw" / "syllable-freq.tsv"

# Chữ cái tiếng Việt, viết thường, dạng NFC dựng sẵn.
# Gộp rời từng nhóm thanh để dễ soát mắt hơn một chuỗi dài liền.
VN_LETTERS = (
    "abcdefghijklmnopqrstuvwxyz"
    "ăâêôơưđ"
    "àáảãạ"
    "ằắẳẵặ"
    "ầấẩẫậ"
    "èéẻẽẹ"
    "ềếểễệ"
    "ìíỉĩị"
    "òóỏõọ"
    "ồốổỗộ"
    "ờớởỡợ"
    "ùúủũụ"
    "ừứửữự"
    "ỳýỷỹỵ"
)

SYLLABLE_RE = re.compile(f"[{VN_LETTERS}]+")

# Cắt câu ở . ! ? … khi có khoảng trắng theo sau. Thô nhưng đủ cho mục đích đo
# false-positive: câu bị cắt lệch vẫn là văn bản tiếng Việt hợp lệ.
SENTENCE_SPLIT_RE = re.compile(r"(?<=[.!?…])\s+")

# Gỡ markup wiki.
#
# Bản đầu tiên của cleaner này chạy TỪNG DÒNG, nên template và thẻ trải trên
# nhiều dòng không khớp pattern nào — hệ quả là `quot`, `lt`, `gt`, `ref`,
# `title`, `publisher` lọt vào kết quả và làm phép đo false-positive vô nghĩa
# (59,64/1000 toàn là rác markup). Giờ clean theo TRỌN TRANG nên `re.S` mới có
# tác dụng thật.
COMMENT_RE = re.compile(r"<!--.*?-->", re.S)
REF_PAIR_RE = re.compile(r"<ref[^>]*>.*?</ref\s*>", re.S | re.I)
REF_SELF_RE = re.compile(r"<ref[^>]*/\s*>", re.I)
# Một pattern cho mỗi thẻ, cố tình KHÔNG dùng backreference `\1` kèm alternation:
# cách đó gây backtracking siêu tuyến tính, và trên corpus 5 GB thì đó là rủi ro
# treo máy thật chứ không phải lý thuyết.
VERBATIM_TAGS = ("nowiki", "math", "chem", "code", "pre", "syntaxhighlight", "source", "timeline")
VERBATIM_RES = [
    re.compile(rf"<{tag}[^>]*>.*?</{tag}\s*>", re.S | re.I) for tag in VERBATIM_TAGS
]
TABLE_RE = re.compile(r"\{\|.*?\|\}", re.S)
TEMPLATE_RE = re.compile(r"\{\{[^{}]*\}\}", re.S)
# [[Tập tin:…]] / [[File:…]] / [[Thể loại:…]] — chú thích ảnh lẫn nhiều markup
NS_LINK_RE = re.compile(
    r"\[\[\s*(?:file|image|tập tin|hình|thể loại|category|media)\s*:[^\[\]]*\]\]",
    re.S | re.I,
)
# Không có pattern riêng cho tiêu đề `== Lịch sử ==`: chữ trong tiêu đề là văn
# xuôi tiếng Việt hợp lệ và đáng giữ lại. Chỉ cần xoá ký tự `=`, và LEFTOVER_RE
# đã làm việc đó. (Pattern `^={2,}.*?={2,}$` còn gây backtracking siêu tuyến tính.)
PIPED_LINK_RE = re.compile(r"\[\[[^\]|]*\|")
EXTLINK_RE = re.compile(r"\[(?:https?|ftp)://[^\s\]]*")
URL_RE = re.compile(r"(?:https?|ftp)://\S+")
LEFTOVER_RE = re.compile(r"[\[\]{}|'*#:;=]")

TEXT_OPEN = re.compile(r"<text\b[^>]*>")
TEXT_CLOSE = "</text>"

# Namespace của trang. Dump `pages-articles` KHÔNG chỉ có bài viết — nó còn chứa
# Bản mẫu:, Thể loại:, Thảo luận:, Thành viên:. Trang thảo luận đầy teencode
# ("mọi ng" = "mọi người"), chữ ký và dấu thời gian, và những thứ đó lọt thẳng vào
# từ điển L2 dưới dạng "từ được chấp nhận" — khiến engine im lặng trước lỗi thật.
# Chỉ giữ ns 0 (bài viết).
NS_RE = re.compile(r"<ns>(\d+)</ns>")
ARTICLE_NS = 0

REPORT_EVERY_PAGES = 20_000

# Ngưỡng lọc câu văn xuôi cho bộ đo false-positive.
SENT_MIN_LEN = 40
SENT_MAX_LEN = 400
SENT_MIN_LETTER_RATIO = 0.9


def clean(page: str) -> str:
    """Gỡ markup wiki khỏi **trọn một trang**.

    Phải nhận trọn trang, không phải từng dòng: template và thẻ `<ref>` của
    Wikipedia thường trải nhiều dòng, và nếu cắt theo dòng thì không pattern nào
    khớp được — tên thuộc tính (`title`, `publisher`, `name`) sẽ lọt ra thành
    "từ" và làm nhiễu mọi phép đo.
    """
    # PHẢI decode entity về ký tự thật TRƯỚC mọi thứ khác, và phải là decode chứ
    # không phải xoá. Dump XML của Wikipedia escape wikitext, nên `<ref>` nằm
    # trong file dưới dạng `&lt;ref&gt;`. Bản trước thay entity bằng dấu cách —
    # phá luôn cấu trúc thẻ và để trơ lại `ref name=…` thành chữ, khiến `ref`
    # trở thành token bị báo lỗi nhiều nhất (48.215 lần). Decode xong thì các
    # pattern thẻ bên dưới mới có ngoặc nhọn thật để khớp.
    # Decode HAI lần: wikitext viết `&nbsp;` thì trong XML thành `&amp;nbsp;`,
    # nên một lượt decode chỉ trả về `&nbsp;` và `nbsp` vẫn lọt ra thành chữ.
    page = html.unescape(html.unescape(page))
    page = COMMENT_RE.sub(" ", page)
    for pat in VERBATIM_RES:
        page = pat.sub(" ", page)
    page = REF_PAIR_RE.sub(" ", page)
    page = REF_SELF_RE.sub(" ", page)
    page = TABLE_RE.sub(" ", page)

    # Template lồng nhau: lặp cho tới khi ổn định. Mỗi lượt gỡ được tầng trong
    # cùng, nên vài lượt là đủ cho mọi trang thực tế.
    for _ in range(6):
        stripped = TEMPLATE_RE.sub(" ", page)
        if stripped == page:
            break
        page = stripped

    page = NS_LINK_RE.sub(" ", page)
    page = re.sub(r"<[^>]{0,200}>", " ", page)  # thẻ HTML còn lại, chặn độ dài
    page = EXTLINK_RE.sub(" ", page)
    page = URL_RE.sub(" ", page)
    page = PIPED_LINK_RE.sub(" ", page)  # [[đích|hiện]] -> giữ phần hiện
    page = LEFTOVER_RE.sub(" ", page)
    return page


def report_progress(n_pages: int, total_bytes: int, started: float, n_skipped: int) -> None:
    print(
        f"  {n_pages:>8,} bài · {total_bytes / 1024 / 1024:>8,.0f} MB · "
        f"bỏ {n_skipped:>7,} trang ngoài bài viết · {time.time() - started:>5.0f}s",
        flush=True,
    )


def iter_page_text(fh: TextIO, limit_bytes: int) -> Iterator[str]:
    """Sinh ra nội dung **trọn từng trang** trong `<text>…</text>`, đã gỡ markup + NFC.

    Gom trọn trang trước khi clean là điều bắt buộc — xem docstring của [`clean`].
    Một trang Wikipedia lớn nhất cũng chỉ vài trăm KB nên giữ trong RAM vô hại.
    """
    total_bytes = 0
    n_pages = 0
    n_skipped = 0
    next_report = REPORT_EVERY_PAGES
    in_text = False
    buf: list[str] = []
    page_ns: int | None = None
    started = time.time()

    for line in fh:
        total_bytes += len(line)

        if not in_text:
            ns_match = NS_RE.search(line)
            if ns_match:
                page_ns = int(ns_match.group(1))
            m = TEXT_OPEN.search(line)
            if not m:
                continue
            in_text = True
            buf = []
            line = line[m.end():]

        if TEXT_CLOSE not in line:
            buf.append(line)
            continue

        buf.append(line[: line.index(TEXT_CLOSE)])
        in_text = False
        joined, buf = "".join(buf), []

        # Bỏ mọi trang không phải bài viết. `ns` không xác định cũng bỏ: bài viết
        # luôn có `<ns>0</ns>`, nên thiếu thẻ đó nghĩa là ta đọc sai cấu trúc.
        if page_ns != ARTICLE_NS:
            page_ns = None
            n_skipped += 1
            continue
        page_ns = None

        n_pages += 1
        yield unicodedata.normalize("NFC", clean(joined))

        if n_pages >= next_report:
            next_report += REPORT_EVERY_PAGES
            report_progress(n_pages, total_bytes, started, n_skipped)
        if limit_bytes and total_bytes >= limit_bytes:
            print(f"  đạt giới hạn, dừng ({n_skipped:,} trang ngoài bài viết đã bỏ)")
            return

    print(f"  hết dump ({n_skipped:,} trang ngoài bài viết đã bỏ)")


def harvest_sentences(cleaned: str, out: list[str], cap: int) -> None:
    """Thu câu văn xuôi đủ chuẩn vào `out`, tối đa `cap` câu.

    Giữ NGUYÊN chữ hoa/thường: engine dùng chính tín hiệu viết hoa giữa câu để
    nhận ra tên riêng, nên hạ chữ thường ở đây sẽ làm phép đo FP sai lệch.
    """
    for sent in SENTENCE_SPLIT_RE.split(cleaned):
        if len(out) >= cap:
            return
        sent = " ".join(sent.split())
        if not SENT_MIN_LEN <= len(sent) <= SENT_MAX_LEN:
            continue
        letters = sum(c.isalpha() or c.isspace() for c in sent)
        if letters / len(sent) < SENT_MIN_LETTER_RATIO:
            continue
        out.append(sent)


def write_freq(path: Path, counts: Counter[str], min_count: int, dump_name: str) -> int:
    """Ghi TSV tần suất, trả về số dòng đã giữ."""
    kept = [(s, c) for s, c in counts.items() if c >= min_count]
    kept.sort(key=lambda kv: (-kv[1], kv[0]))

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as out:
        out.write(f"# nguồn: {dump_name}\n")
        out.write(f"# tổng token: {sum(counts.values())}\n")
        out.write(
            f"# âm tiết khác nhau: {len(counts)} (giữ >= {min_count}: {len(kept)})\n"
        )
        for syl, cnt in kept:
            out.write(f"{syl}\t{cnt}\n")
    return len(kept)


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dump", type=Path, default=DEFAULT_DUMP)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument(
        "--limit-mb",
        type=int,
        default=0,
        help="Chỉ đọc N MB đầu (đã giải nén) — để smoke test pipeline cho nhanh",
    )
    ap.add_argument(
        "--min-count",
        type=int,
        default=2,
        help="Bỏ âm tiết có tần suất dưới ngưỡng (mặc định 2) để cắt nhiễu",
    )
    ap.add_argument(
        "--sentences-out",
        type=Path,
        default=None,
        help="Xuất thêm câu văn xuôi ra file này — dùng đo false-positive rate",
    )
    ap.add_argument("--sentences-max", type=int, default=200_000)
    return ap.parse_args()


def main() -> int:
    args = parse_args()

    if not args.dump.exists():
        print(f"LỖI: không thấy dump tại {args.dump}", file=sys.stderr)
        return 1

    limit_bytes = args.limit_mb * 1024 * 1024 if args.limit_mb else 0
    counts: Counter[str] = Counter()
    sentences: list[str] | None = [] if args.sentences_out else None
    started = time.time()

    print(f"Đọc {args.dump}")
    if limit_bytes:
        print(f"Giới hạn {args.limit_mb} MB (đã giải nén) — chế độ smoke test")

    with bz2.open(args.dump, "rt", encoding="utf-8", errors="replace") as fh:
        for cleaned in iter_page_text(fh, limit_bytes):
            counts.update(SYLLABLE_RE.findall(cleaned.lower()))
            if sentences is not None and len(sentences) < args.sentences_max:
                harvest_sentences(cleaned, sentences, args.sentences_max)

    kept = write_freq(args.out, counts, args.min_count, args.dump.name)

    print(f"\nXong trong {time.time() - started:.0f}s")
    print(f"  tổng token           : {sum(counts.values()):,}")
    print(f"  âm tiết khác nhau    : {len(counts):,}")
    print(f"  giữ lại (>= {args.min_count})       : {kept:,}")
    print(f"  ghi ra               : {args.out}")

    if sentences is not None:
        args.sentences_out.parent.mkdir(parents=True, exist_ok=True)
        with args.sentences_out.open("w", encoding="utf-8", newline="\n") as out:
            out.write("\n".join(sentences) + "\n")
        print(f"  câu văn xuôi         : {len(sentences):,} → {args.sentences_out}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
