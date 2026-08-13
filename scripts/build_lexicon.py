#!/usr/bin/env python3
"""Dựng từ vựng L2 từ corpus: âm tiết đã chứng thực, từ vay mượn, từ ghép.

# Vấn đề mà script này giải

Vòng đo false-positive cho thấy L1 một mình báo oan **20,52 lần / 1000 từ**, và
toàn bộ là **từ vay mượn lẫn trong văn bản Việt**: `electron`, `protein`, `virus`,
`oxy`, `ion`, cùng nhóm phiên âm có dấu Việt `vectơ`, `nitơ`, `hiđrô`, `mômen`,
`kilômét`, `axít`, `lôgic`. Chúng hợp lệ trong văn bản Việt nhưng không phải âm
tiết tiếng Việt, nên L1 không thể biết.

# Cách phân biệt từ vay mượn với lỗi gõ tay

Cả hai đều là "không phải âm tiết tiếng Việt". Thứ tách chúng ra là **độ lan toả**:
từ vay mượn xuất hiện rải khắp nhiều câu khác nhau, còn lỗi gõ tay thì lẻ tẻ.

Bằng chứng có sẵn trong dữ liệu: `vectơ` xuất hiện 929 lần, còn `thuớc` — một lỗi
chính tả **thật** của Wikipedia (đúng là `thước`) — chỉ 11 lần. Chúng nằm ở hai đầu
phân bố.

Dùng **số câu chứa** thay vì tần suất thô, vì tần suất thô bị một bài viết dài lặp
lại một từ làm lệch, còn độ lan toả thì không.

# Vì sao không cần từ điển bên ngoài

Toàn bộ danh sách này suy ra từ corpus, nên không phái sinh từ `hunspell-vi` hay
từ điển GPL nào — license MIT của dự án giữ nguyên.

Cách chạy:

    writa-cli dump data/build/syllables.txt      # cần chạy trước
    py scripts/build_lexicon.py
"""

from __future__ import annotations

import argparse
import re
import sys
import unicodedata
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

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
TOKEN_RE = re.compile(f"[{VN_LETTERS}]+")


def norm(s: str) -> str:
    return unicodedata.normalize("NFC", s).lower()


def load_syllables(path: Path) -> set[str]:
    if not path.exists():
        sys.exit(
            f"LỖI: không thấy {path}\n"
            "Chạy trước:  cargo run -p writa-cli --release -- dump data/build/syllables.txt"
        )
    return {norm(l) for l in path.read_text(encoding="utf-8").split() if l}


def load_freq(path: Path) -> Counter[str]:
    counts: Counter[str] = Counter()
    if not path.exists():
        return counts
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        tok, _, cnt = line.partition("\t")
        if cnt.strip().isdigit():
            counts[norm(tok)] += int(cnt)
    return counts


def scan_sentences(
    path: Path, valid: set[str], prune_every: int
) -> tuple[Counter[str], Counter[str], Counter[str], int, int]:
    """Một lượt qua file câu, trả về (tần suất, số câu chứa, bigram, số câu, số token).

    `bigrams` chỉ gồm cặp mà **cả hai** thành phần là âm tiết tiếng Việt hợp lệ —
    đó là định nghĩa của từ ghép, và cũng là cách chặn không gian khoá phình to.
    """
    freq: Counter[str] = Counter()
    spread: Counter[str] = Counter()
    bigrams: Counter[str] = Counter()
    n_sent = 0
    n_tok = 0

    with path.open(encoding="utf-8") as fh:
        for line in fh:
            # PHẢI hạ chữ thường TRƯỚC khi findall. TOKEN_RE chỉ chứa chữ thường,
            # nên quét trên dòng còn chữ hoa sẽ bỏ chữ cái đầu của mọi từ viết
            # hoa: `Wikipedia` → `ikipedia`, `Ông` → `ng`, `Anh` → `nh`.
            # Bản đầu tiên mắc đúng lỗi này và sinh ra một lexicon đầy mảnh vụn —
            # nguy hiểm hơn báo oan, vì nó khiến engine IM LẶNG trước lỗi thật.
            toks = TOKEN_RE.findall(norm(line))
            if not toks:
                continue
            n_sent += 1
            n_tok += len(toks)
            freq.update(toks)
            spread.update(set(toks))
            for a, b in zip(toks, toks[1:]):
                if a in valid and b in valid:
                    bigrams[f"{a} {b}"] += 1

            # Van an toàn bộ nhớ: số bigram khác nhau tăng gần như tuyến tính theo
            # corpus. Bỏ các cặp chỉ thấy 1 lần theo chu kỳ. Đây là XẤP XỈ — cặp
            # xuất hiện muộn có thể bị đếm thiếu — nhưng ta chỉ cần từ ghép PHỔ
            # BIẾN, nên đánh đổi này chấp nhận được.
            if prune_every and n_sent % prune_every == 0:
                before = len(bigrams)
                for k in [k for k, v in bigrams.items() if v <= 1]:
                    del bigrams[k]
                print(
                    f"  {n_sent:>8,} câu · tỉa bigram {before:,} → {len(bigrams):,}",
                    flush=True,
                )

    return freq, spread, bigrams, n_sent, n_tok


def scan_trigrams(
    path: Path, valid: set[str], keep_prefix: set[tuple[str, str]], min_count: int
) -> Counter[str]:
    """Lượt hai: đếm trigram, CHỈ những cái có tiền tố bigram đủ phổ biến.

    Đếm trigram không ràng buộc thì số khoá khác nhau xấp xỉ số token — với 13 triệu
    token thì Counter của Python ngốn vài GB. Ràng buộc theo tiền tố đã giữ lại ở
    lượt một chặn được không gian khoá mà gần như không mất thông tin: trigram có
    tiền tố hiếm thì bản thân nó cũng hiếm, và mô hình sẽ backoff về bigram.
    """
    counts: Counter[str] = Counter()
    with path.open(encoding="utf-8") as fh:
        for line in fh:
            toks = TOKEN_RE.findall(norm(line))
            for a, b, c in zip(toks, toks[1:], toks[2:]):
                if (a, b) in keep_prefix and c in valid:
                    counts[f"{a} {b} {c}"] += 1
    return Counter({k: v for k, v in counts.items() if v >= min_count})


def write_tsv(path: Path, header: list[str], rows: list[tuple]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as out:
        for line in header:
            out.write(f"# {line}\n")
        for row in rows:
            out.write("\t".join(str(c) for c in row) + "\n")


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--syllables", type=Path, default=ROOT / "data/build/syllables.txt")
    ap.add_argument("--sentences", type=Path, default=ROOT / "data/raw/sentences.txt")
    ap.add_argument("--freq", type=Path, default=ROOT / "data/raw/syllable-freq.tsv")
    ap.add_argument("--out-dir", type=Path, default=ROOT / "data/lexicon")
    ap.add_argument(
        "--accept-min-sentences",
        type=int,
        default=20,
        help="Token không phải âm tiết cần xuất hiện trong >= N câu khác nhau mới "
        "được coi là từ vay mượn được chấp nhận (mặc định 20)",
    )
    ap.add_argument(
        "--compound-min-count",
        type=int,
        default=8,
        help="Từ ghép cần xuất hiện >= N lần mới được giữ (mặc định 8)",
    )
    ap.add_argument(
        "--trigram-min-count",
        type=int,
        default=6,
        help="Trigram cần xuất hiện >= N lần mới được giữ (mặc định 6)",
    )
    ap.add_argument("--prune-every", type=int, default=100_000)
    return ap.parse_args()


def main() -> int:
    args = parse_args()
    valid = load_syllables(args.syllables)
    print(f"Âm tiết hợp lệ (từ writa-cli dump): {len(valid):,}")

    if not args.sentences.exists():
        sys.exit(f"LỖI: không thấy {args.sentences} — chạy extract_syllables.py --sentences-out")

    print(f"Đọc {args.sentences}")
    freq, spread, bigrams, n_sent, n_tok = scan_sentences(
        args.sentences, valid, args.prune_every
    )
    print(f"  {n_sent:,} câu · {n_tok:,} token · {len(freq):,} token khác nhau")

    # --- âm tiết đã chứng thực -------------------------------------------------
    # Tần suất lấy từ file đếm TRỌN dump nếu có (nhiều dữ liệu hơn hẳn), rơi về
    # số đếm trên tập câu nếu chưa có.
    dump_freq = load_freq(args.freq)
    source = dump_freq if dump_freq else freq
    attested = sorted(
        ((s, source[s], freq[s]) for s in valid if source[s] > 0),
        key=lambda kv: (-kv[1], kv[0]),
    )
    write_tsv(
        args.out_dir / "syllables.tsv",
        [
            "Âm tiết tiếng Việt hợp lệ ĐÃ CHỨNG THỰC trong corpus, kèm tần suất.",
            f"Nguồn cột 2: {args.freq.name if dump_freq else args.sentences.name}",
            f"{len(attested)} / {len(valid)} âm tiết sinh ra có xuất hiện thật.",
            "Âm tiết hợp lệ nhưng KHÔNG có ở đây là tín hiệu 'nghi vấn' cho L3/L4:",
            "đúng ngữ âm nhưng không ai dùng, nên rất có thể là lỗi gõ.",
            "",
            "Cột 3 = tần suất đếm trên CHÍNH tập câu đã dựng compounds.tsv. Cần cột",
            "riêng này để tính được xác suất có điều kiện P(b|a) = freq(a b)/freq(a):",
            "hai con số phải cùng một mẫu, không thì tỉ số vô nghĩa. Đó là thứ phân",
            "biệt từ ghép CỐ ĐỊNH (`chia sẻ`) với kết hợp TỰ DO (`cát trắng`) — và",
            "chính là chỗ mà tần suất bigram thô không đủ.",
            "",
            f"tổng token trên tập câu: {n_tok}",
            "",
            "âm_tiết\ttần_suất_dump\ttần_suất_tập_câu",
        ],
        attested,
    )
    print(f"  syllables.tsv : {len(attested):,} âm tiết đã chứng thực")

    # --- từ vay mượn được chấp nhận -------------------------------------------
    accepted = sorted(
        (
            (t, freq[t], spread[t])
            for t, sp in spread.items()
            if t not in valid and sp >= args.accept_min_sentences
        ),
        key=lambda r: (-r[2], r[0]),
    )
    write_tsv(
        args.out_dir / "accepted.tsv",
        [
            "Token KHÔNG phải âm tiết tiếng Việt nhưng được chấp nhận trong văn bản Việt.",
            "Gồm ba nhóm: từ vay mượn khoa học (vectơ, nitơ, electron, protein),",
            "tên riêng ngoại (méxico, napoléon), và viết tắt thường (sđd, đ).",
            "",
            f"Tiêu chí: xuất hiện trong >= {args.accept_min_sentences} câu khác nhau.",
            "Dùng SỐ CÂU CHỨA chứ không phải tần suất thô: tần suất thô bị một bài",
            "dài lặp lại một từ làm lệch, còn độ lan toả thì không.",
            "",
            f"Nguồn: {args.sentences.name} ({n_sent} câu)",
            "",
            "token\ttần_suất\tsố_câu_chứa",
        ],
        accepted,
    )
    print(f"  accepted.tsv  : {len(accepted):,} từ vay mượn / tên riêng / viết tắt")

    # --- từ ghép ---------------------------------------------------------------
    compounds = sorted(
        ((k, v) for k, v in bigrams.items() if v >= args.compound_min_count),
        key=lambda kv: (-kv[1], kv[0]),
    )
    write_tsv(
        args.out_dir / "compounds.tsv",
        [
            "Từ ghép 2 âm tiết (bigram) trong đó CẢ HAI thành phần là âm tiết hợp lệ.",
            f"Tiêu chí: xuất hiện >= {args.compound_min_count} lần.",
            "",
            "Phục vụ tách từ và xếp hạng candidate ở L3. Từ ghép 3-4 âm tiết sẽ đến",
            "cùng lượt dựng n-gram LM của L4 — cùng một lượt đọc corpus, không nên",
            "chạy hai lần.",
            "",
            f"Nguồn: {args.sentences.name} ({n_sent} câu)",
            "",
            "âm_tiết_1 âm_tiết_2\ttần_suất",
        ],
        compounds,
    )
    print(f"  compounds.tsv : {len(compounds):,} từ ghép")

    # --- trigram cho mô hình ngôn ngữ L4 ---------------------------------------
    keep_prefix = {
        (k.split(" ")[0], k.split(" ")[1]) for k, _ in compounds
    }
    print(f"Lượt hai: đếm trigram có tiền tố trong {len(keep_prefix):,} bigram đã giữ")
    trigrams = scan_trigrams(
        args.sentences, valid, keep_prefix, args.trigram_min_count
    )
    tri_rows = sorted(trigrams.items(), key=lambda kv: (-kv[1], kv[0]))
    write_tsv(
        args.out_dir / "trigrams.tsv",
        [
            "Trigram âm tiết cho mô hình ngôn ngữ L4 (Stupid Backoff).",
            f"Tiêu chí: tiền tố bigram nằm trong compounds.tsv, và xuất hiện >= "
            f"{args.trigram_min_count} lần.",
            "",
            "Trigram vắng mặt KHÔNG có nghĩa là 'không thể' — mô hình sẽ lùi về",
            "bigram rồi unigram. Chính chỗ này là thứ tần suất thô làm sai: nó coi",
            "đếm bằng 0 là bằng chứng, trong khi đó chỉ là thiếu dữ liệu.",
            "",
            f"Nguồn: {args.sentences.name} ({n_sent} câu, {n_tok} token)",
            "",
            "âm_tiết_1 âm_tiết_2 âm_tiết_3\ttần_suất",
        ],
        tri_rows,
    )
    print(f"  trigrams.tsv  : {len(tri_rows):,} trigram")

    for name in ("syllables.tsv", "accepted.tsv", "compounds.tsv", "trigrams.tsv"):
        size = (args.out_dir / name).stat().st_size
        print(f"    {name:<16} {size / 1024:>9,.0f} KB")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
