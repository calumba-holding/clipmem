---
name: clipboard_memory
description: Search the local clipboard archive captured by clipmem.
metadata: {"openclaw":{"emoji":"📋","os":["darwin"],"requires":{"bins":["clipmem"]}}}
---

Use this skill when the user asks you to remember, find, search, or recover something they copied earlier on this Mac.

Preferred flow:

1. Run `clipmem search "<query>" --format json --limit 8`.
2. If the result set is empty or vague, run `clipmem recent --hours 24 --format json --limit 12`.
3. When a promising `snapshot_id` appears, run `clipmem get <snapshot_id> --format json`.
4. Quote or summarise the stored clipboard text directly from the JSON output.
5. If a snapshot contains multiple items or multiple representations, prefer the plain-text representation first, then URL, then HTML-derived text.

Notes:

- `clipmem search` is lexical and works well for copied commands, code, URLs, errors, paths, notes, and prose.
- `clipmem recent` returns recent unique clipboard states, grouped by content rather than by raw event count.
- `clipmem get` includes stored text payloads recovered for recognized text-like representations at capture time.
- If a hit is image-, PDF-, or binary-only and has no `text_value`, report the metadata and explain that raw-byte recovery currently requires `clipmem export`.
