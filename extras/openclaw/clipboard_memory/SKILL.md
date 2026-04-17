---
name: clipboard_memory
description: Recover copied text, commands, URLs, and clipboard items from this Mac with clipmem.
metadata: {"openclaw":{"emoji":"📋","os":["darwin"],"requires":{"bins":["clipmem"]},"install":[{"id":"brew","kind":"brew","label":"Install clipmem (brew)","bins":["clipmem"],"formula":"clipmem","tap":"tristanmanchester/tap"},{"id":"cargo","kind":"cargo","label":"Install clipmem (cargo)","bins":["clipmem"],"package":"clipmem"}]}}
---

Recover what the user copied on this Mac before reaching for generic search.

Use this skill when the user asks things like:

- "what was that command I copied?"
- "show me things I copied from Safari today"
- "find the URL I copied yesterday"
- "give me the exact text, not just a summary"
- "what did I copy earlier?"
- "find that snippet, link, note, or path I copied"

Do not use this skill for:

- general web search or current-events lookups
- searching the repository itself
- files or messages the user never copied into the clipboard

Use commands in this order:

1. Start with `clipmem recall ... --format json`.
2. Use `clipmem timeline` for true chronological history, or `clipmem search` for direct lexical lookup.
3. Use `clipmem get <snapshot_id> --format json` only when you need exact nested detail or provenance.

Near misses:

- `clipmem recent` shows recent unique clipboard states; `clipmem timeline` shows every capture event in order.
- `clipmem recall` is answer-first; `clipmem get` is forensic detail.

Quick examples:

- Command: `clipmem recall "that command I copied" --format json`
- Safari today: `clipmem recall --prefer-recent --app safari --hours 24 --format json`
- URL from yesterday: `clipmem recall "url" --hours 48 --format json`
- Exact text: `clipmem recall "<query>" --format json --quote --full`

Detailed command guidance: [references/commands.md](references/commands.md)
Troubleshooting: [references/troubleshooting.md](references/troubleshooting.md)
