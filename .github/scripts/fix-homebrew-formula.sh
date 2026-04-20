#!/usr/bin/env python3

import pathlib
import re
import sys


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <formula.rb>", file=sys.stderr)
        return 2

    path = pathlib.Path(sys.argv[1])
    text = path.read_text()

    text = re.sub(r'^\s*version "[^"]+"\n', "", text, count=1, flags=re.MULTILINE)
    text = re.sub(
        r'\n\s*if OS\.mac\? && Hardware::CPU\.arm\?\n'
        r'\s*(url "[^"]+")\n'
        r'\s*(sha256 "[^"]+")\n'
        r'\s*end\n',
        r"\n  \1\n  \2\n",
        text,
        count=1,
    )
    text = re.sub(
        r'\n\s*if OS\.mac\?\n'
        r'\s*if Hardware::CPU\.arm\?\n'
        r'\s*(url "[^"]+")\n'
        r'\s*(sha256 "[^"]+")\n'
        r'\s*end\n'
        r'\s*end\n',
        r"\n  \1\n  \2\n",
        text,
        count=1,
    )
    text = text.replace(
        '    bin.install "clipmem" if OS.mac? && Hardware::CPU.arm?',
        '    bin.install "clipmem"',
    )
    text = re.sub(
        r'\n\s*if OS\.mac\? && Hardware::CPU\.arm\?\n'
        r'\s*bin\.install "clipmem"\n'
        r'\s*end\n',
        '\n    bin.install "clipmem"\n',
        text,
        count=1,
    )
    text = re.sub(
        r'\n\s*if OS\.mac\?\n'
        r'\s*if Hardware::CPU\.arm\?\n'
        r'\s*bin\.install "clipmem"\n'
        r'\s*end\n'
        r'\s*end\n',
        '\n    bin.install "clipmem"\n',
        text,
        count=1,
    )

    if "  url " not in text:
        print(f"{path}: formula repair failed: missing top-level url", file=sys.stderr)
        return 1
    if 'bin.install "clipmem" if OS.mac?' in text or "if OS.mac?" in text:
        print(f"{path}: formula repair failed: leftover OS guard", file=sys.stderr)
        return 1

    path.write_text(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
