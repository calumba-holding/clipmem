---
status: implemented
created: 2026-07-10
last-verified: 2026-07-11
implemented-in: schema v23, builder v3 merged implementation
owners: []
---

# Plan 9 — Text projection and content contracts

**Priority:** P2  
**Primary owners:** model projection builder, canonical document, search/detail output  
**Depends on:** plan 4 canonical builder/document

## Problem and evidence

HTML/RTF extraction is lightweight and may lose or add text. Binary bytes can retain a decoded `text_value`, while search and detail use different eligibility rules. Preview, best text, summary, fragments, and factual text flags are conflated in places.

## Required outcome

Every representation has explicit decoded/display/search roles. HTML/RTF projection is deterministic, bounded, versioned, and tested against real clipboard fixtures. Search, detail, filters, and copy-text consume the same canonical contract while raw bytes remain authoritative.

## Representation content model

For each representation derive:

```text
DecodedRepresentation {
  uti
  classified_kind
  decoded_text?          // mechanical decode result
  display_role           // primary | supplemental | metadata | hidden
  search_role            // searchable | exact-only | excluded
  copy_text_role         // preferred | supplemental | excluded
  structured_values      // URLs/file paths/etc.
  diagnostics            // decode/parser warnings, not normally user-visible
}
```

Do not use `decoded_text.is_some()` as proof it should be searchable or displayed. Chromium source metadata, opaque binary, and producer metadata receive explicit roles.

Canonical document builder selects:

- best text according to stable UTI/role priority;
- fragments only from allowed display roles;
- search text only from searchable roles;
- copy-text output according to copy roles;
- preview as a display-only summary;
- factual flags from roles/structured content/OCR.

## UTI classification

Replace broad substring heuristics where they produce ambiguity with a table/predicate registry:

- exact known public UTIs and conformance relationships where available;
- explicit producer metadata exclusions;
- fallback opaque binary;
- documented priority ordering.

Keep platform-independent behavior deterministic for DB migration/tests. AppKit `UTType` conformance can be an adapter, not the sole source needed on Linux tests.

Unknown valid UTF-8 binary remains decoded for diagnostics only unless a role rule marks it searchable/displayable.

## HTML projection

Requirements:

- decode charset from representation context/HTML metadata where practical, with bounded fallback;
- parse text nodes, decode entities;
- exclude script/style/noscript/template and nonvisible metadata;
- preserve useful block/line boundaries without excessive whitespace;
- extract href/src URLs separately according to current product contract;
- cap input bytes, output chars, nesting/work to avoid pathological resource use (resource correctness, not a security audit);
- malformed HTML yields best-effort deterministic text and warnings, never panics.

Choose a maintained parser crate only after dependency/license/size evaluation; otherwise implement a deliberately small tokenizer with a strict fixture contract. Do not use regex stripping as the final parser.

## RTF projection

Requirements:

- handle escaped braces/backslashes, hex escapes, Unicode `\uN` and `\ucN` fallback counts;
- skip nontext destinations (font/color tables, pictures, objects, annotations as chosen);
- honor ignorable destinations;
- normalize paragraph/line/tab controls;
- cap input/output/work;
- malformed groups produce deterministic best effort.

Prefer a focused parser crate if quality and maintenance are acceptable; otherwise write a state machine with comprehensive fixtures. Do not expand ad-hoc replacement chains.

## Projection versioning

Increment `builder_version` when role/parser output changes. Rebuild canonical documents in durable bounded jobs or maintenance batches. Source rows do not change. Search readiness supports mixed versions only under explicit policy; ideally queue rebuild and query latest/compatible version.

Output may include projection version in diagnostics/get advanced metadata, not necessarily every list row.

## Implementation sequence

1. Collect anonymized/repository-safe fixtures from major producers represented by current code/tests: plain text, Safari/Chrome/ChatGPT/Codex HTML, rich editors, Finder file URLs, RTF/RTFD metadata, image metadata, unknown binary.
2. Define role registry and golden expected decoded/search/display/copy/structured outputs.
3. Refactor existing builder to emit `DecodedRepresentation` without changing output; shadow compare.
4. Implement HTML parser adapter and fixtures.
5. Implement RTF parser adapter and fixtures.
6. Integrate roles into canonical `SnapshotDocumentBuilder`; remove detail-time alternate flattening.
7. Rebuild documents with new builder version; compare search quality benchmark.
8. Update get/output/docs to explain extracted vs original text and copy behavior.
9. Delete duplicated migration/detail extraction logic.

## Edge cases

- UTF-16 with BOM/embedded NUL; preserve repaired behavior from migration tests.
- Multiple text representations with same semantic content; deduplicate fragments deterministically without losing UTI provenance.
- HTML containing only image/alt text: decide whether alt text is searchable and test.
- RTF attachments/pictures: excluded from text but source remains exportable/restorable.
- Very large rich text: output truncation has explicit limits and indicates truncation in diagnostics/document.
- URLs in visible text vs link target: store both with provenance; avoid duplicate display noise.
- OCR plus native rich text: native remains best according to rule, OCR supplemental unless it is the only text.

## Tests

- Golden parser/role fixtures for all cases above.
- Fuzz/property tests for no panic and bounded output/work on malformed HTML/RTF (not framed as a security pass).
- Live builder vs migration rebuild identical.
- Search/detail/copy-text all derive from same document fields.
- Historical migration repair fixtures remain correct.
- Benchmark top-k and snippets before/after; intentional changes approved.

## Acceptance criteria

- One representation-role registry and one canonical builder own search/display/copy text.
- No detail-time alternate projection over raw reps.
- HTML/RTF parsing passes golden malformed/unicode/destination fixtures and is bounded.
- Binary placeholder/decoded text cannot accidentally become factual `has_text` or search content.
- Builder version and rebuild path are operational and resumable.
- Raw export/restore bytes are unchanged.

## Rollback

Old canonical document versions remain until new backfill passes shadow comparison. Read/search feature flag can select previous builder version. Parser dependency/code can be rolled back without source migration because only derived documents change.
