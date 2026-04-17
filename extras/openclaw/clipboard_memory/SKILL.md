---
name: clipboard_memory
description: Search the local clipboard archive captured by clipmem.
metadata: {"openclaw":{"emoji":"📋","os":["darwin"],"requires":{"bins":["clipmem"]}}}
---

Use this skill when the user asks you to remember, find, search, or recover something they copied earlier on this Mac.

Preferred flow:

1. Run `clipmem recall "<query>" --format json --limit 5`.
2. If there is no query, or the request is more about “what did I copy recently?”, run `clipmem recall --prefer-recent --hours 24 --format json --limit 5`.
3. Use `best_candidate.best_text`, `best_candidate.urls`, `best_candidate.file_paths`, and `why_selected` from the recall output first.
4. When a `snapshot_id` needs deeper nested detail, run `clipmem get <snapshot_id> --format json`.
5. Quote or summarise the surfaced recall text directly from the JSON output before falling back to nested representations.

Notes:

- `clipmem recall` is the primary retrieval command. It chooses one best candidate, ranks alternatives, and falls back to recent clipboard items when query matches are weak.
- `clipmem search` is still available for direct lexical lookup when you want raw ranked matches.
- `clipmem recent` is for recent unique clipboard states deduplicated by snapshot.
- `clipmem timeline` is for the true chronological capture-event history, including repeated copies of the same content.
- `clipmem get` includes stored text payloads recovered for recognized text-like representations at capture time.
- If a hit is image-, PDF-, or binary-only and has no `text_value`, report the metadata and explain that raw-byte recovery currently requires `clipmem export`.
