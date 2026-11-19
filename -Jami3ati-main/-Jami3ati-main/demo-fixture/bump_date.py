"""Advance the demo date by five days, changing nothing else.

Usage:
    python bump_date.py <directory>

The fallback demo must show a one-line diff. Rewriting the page from a
PowerShell string is how the heading drifts (two-line diff) and how a missing
<meta charset> turns Arabic into mojibake. This script edits the date in place
and keeps every other byte identical.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PATTERN = re.compile(r"(أجل الترشيح:\s*)(\d{1,2})(\s+ديسمبر\s+2026)")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: python bump_date.py <directory>")
        return 2

    path = Path(sys.argv[1]) / "index.html"
    if not path.exists():
        print(f"error: {path} does not exist; copy demo-fixture/index.html first")
        return 1

    html = path.read_text(encoding="utf-8")
    match = PATTERN.search(html)
    if not match:
        print("error: date line not found; expected '<day> ديسمبر 2026'")
        return 1

    day = int(match.group(2))
    new_day = day + 5
    if new_day > 30:
        print(f"error: day {day} + 5 exceeds 30; restore demo-fixture/index.html")
        return 1

    updated = html[: match.start(2)] + str(new_day) + html[match.end(2) :]
    path.write_text(updated, encoding="utf-8", newline="\n")
    print(f"date: {day} -> {new_day} December 2026")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
