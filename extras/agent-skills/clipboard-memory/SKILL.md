---
name: clipboard-memory
description: Recover copied text, commands, URLs, and clipboard items from a local clipmem archive.
compatibility: Works in agent runtimes with repository file access and a local `clipmem` binary on PATH.
metadata:
  version: "1.0.0"
---

Recover what the user copied on this machine before using generic search.

Use this skill when the user asks things like:

- "what was that command I copied?"
- "show me things I copied from Safari today"
- "find the URL I copied yesterday"
- "give me the exact text, not just a summary"
- "what did I copy earlier?"

Do not use this skill for:

- web research or current information lookups
- repository code search
- content the user never copied into the clipboard

Use commands in this order:

1. Start with `clipmem recall ... --format json`.
2. Use `clipmem timeline` or `clipmem search` when chronology or literal matching matters.
3. Use `clipmem get <snapshot_id> --format json` only for deeper nested detail.

This package is the portable variant for generic agent runtimes. For the OpenClaw-native package, use the sibling package under `extras/openclaw/clipboard_memory/`.

Detailed command guidance: [references/commands.md](references/commands.md)
Troubleshooting: [references/troubleshooting.md](references/troubleshooting.md)
