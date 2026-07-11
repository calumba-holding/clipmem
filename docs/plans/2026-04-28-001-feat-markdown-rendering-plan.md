---
title: "feat: Render basic Markdown in menu bar text"
type: feat
status: implemented
created: 2026-04-28
last-verified: 2026-07-11
implemented-in: current repository state
owners: []
---

# feat: Render basic Markdown in menu bar text

## Summary

Add a shared display-only Markdown renderer for the native menu bar app, then use it in compact result rows and History detail content. The implementation should render bold, italics, headings, and Markdown links visually while preserving the original stored, copied, restored, searched, and exported clipboard text.

---

## Problem Frame

Markdown snippets are common clipboard content, but the menu bar popover and History window currently show them as plain source text. Basic visual rendering would make notes, copied docs, and AI/tool output easier to scan without turning Clipmem into a full Markdown viewer.

---

## Requirements

- R1. Popover/recent result rows render basic Markdown styling for text-like clipboard entries.
- R2. History detail content renders basic Markdown styling for text-like clipboard entries.
- R3. Supported Markdown includes bold, italics, headings/titles, and `[label](url)` links.
- R4. Rendered links are colored and underlined but are not clickable.
- R5. Clipboard storage, restore, copy, search, export, and JSON decoding behavior remain unchanged.
- R6. Unsupported or malformed Markdown degrades to readable plain text rather than hiding content.
- R7. Compact row rendering preserves list density and truncation behavior.

---

## Scope Boundaries

- No clickable links.
- No full Markdown support for tables, images, task lists, fenced code blocks, blockquotes, footnotes, or embedded HTML.
- No Markdown/plain-text toggle.
- No Markdown editing.
- No Rust backend, database schema, CLI output, capture, restore, search, or export changes.
- No change to what is copied from action buttons or restored to the pasteboard.

### Deferred to Follow-Up Work

- Clickable link actions: future UX pass if rendered links prove useful enough to justify interaction design.
- Broader Markdown coverage: future work only if real usage shows basic rendering is insufficient.

---

## Context & Research

### Relevant Code and Patterns

- `macos/ClipmemMenuBar/ClipmemMenuBar/Views/ResultRowView.swift` renders row titles using `Text(item.displayText)` with two-line truncation, hover help, and existing typography tokens.
- `macos/ClipmemMenuBar/ClipmemMenuBar/Views/SnapshotDetailView.swift` renders full selected content using plain monospace `Text(text)` inside the Content section.
- `macos/ClipmemMenuBar/ClipmemMenuBar/Models/ClipmemModels.swift` owns text selection priority through `displayText`, `copyablePlainText`, and `SnapshotDetails` text fields; these should remain source-of-truth strings.
- `macos/ClipmemMenuBar/ClipmemMenuBar/Views/DesignSystem.swift` contains the app's typography, spacing, tint, and radius tokens; new rendering styles should align with these tokens.
- `macos/ClipmemMenuBar/ClipmemMenuBarTests/HistoryModelTests.swift` and `macos/ClipmemMenuBar/ClipmemMenuBarTests/CommandRunnerTests.swift` show the current Swift Testing style and helper pattern.
- `macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj/project.pbxproj` targets macOS 14 and Swift 6, making Foundation `AttributedString` and SwiftUI attributed text viable without adding a package dependency.

### Institutional Learnings

- `CHANGELOG.md` records several menu bar rendering and JSON-decoding fixes, including OCR/status decoding and row-rendering performance work. Keep this feature UI-local and avoid touching data contracts unless implementation proves a gap.
- `docs/solutions/performance-issues/improve-file-url-capture-storage-performance-2026-04-24.md` reinforces the project preference for scoped, measured changes when storage paths are involved. This plan avoids storage paths entirely.

### External References

- Apple Developer Documentation: `AttributedString(markdown:options:baseURL:)` can parse Markdown strings into attributed runs, including emphasis and links.
- Apple Developer Documentation: SwiftUI `Text` can display an `AttributedString`, and text attributes take priority over outer modifiers where applicable.

---

## Key Technical Decisions

- Use a shared SwiftUI/Foundation rendering helper rather than per-view parsing: keeps popover, History, and likely Quick Recall result rows visually consistent and makes malformed-input fallback testable in one place.
- Prefer platform-native `AttributedString(markdown:)` before considering a dependency: the app already targets macOS 14/Swift 6, and the requested feature is limited to basic Markdown presentation.
- Keep rendering display-only: all model fields remain plain strings, and copy/restore/search/export continue using existing source text.
- Treat row and detail typography differently: rows should preserve density and line limits, while History detail can show clearer heading hierarchy within the existing content panel.
- Style link runs after parsing so they are visibly link-like but non-interactive: colored and underlined text satisfies the current requirement without adding navigation behavior.
- Keep failure behavior conservative: parsing failures or unsupported Markdown should fall back to plain text display.

---

## Open Questions

### Resolved During Planning

- Should links be clickable? No. They should be visually styled only.
- Should Markdown affect stored or copied content? No. The renderer is display-only.
- Should this include full Markdown support? No. The scope is bold, italics, headings/titles, and links only.

### Deferred to Implementation

- Exact parser options: choose the strictest option set that supports the requested constructs while preserving readable fallback behavior after confirming compiler support in this project.
- Exact visual mapping for heading levels in compact rows: tune during UI implementation so headings are noticeable without increasing row height unpredictably.
- Exact helper naming and file placement: choose the smallest local shape that fits existing SwiftUI view organization.

---

## Implementation Units

- U1. **Add shared Markdown text rendering helper**

**Goal:** Provide one app-local rendering surface that converts clipboard source text into display-ready attributed text with graceful fallback.

**Requirements:** R3, R4, R5, R6

**Dependencies:** None

**Files:**
- Create: `macos/ClipmemMenuBar/ClipmemMenuBar/Views/MarkdownTextRenderer.swift`
- Modify: `macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj/project.pbxproj`
- Test: `macos/ClipmemMenuBar/ClipmemMenuBarTests/MarkdownTextRendererTests.swift`

**Approach:**
- Add a small renderer/helper in the menu bar app layer, not the Rust CLI or model layer.
- Parse source strings into an attributed representation for display.
- Apply app-appropriate attributes for supported Markdown constructs, especially link foreground color and underline.
- Preserve the source string as the fallback output when parsing fails, the input is empty, or Markdown is malformed.
- Keep helper behavior deterministic and independent of view state so it can be unit tested directly.

**Execution note:** Implement the helper test-first because parsing and fallback behavior are easy to regress silently.

**Patterns to follow:**
- Swift Testing style from `macos/ClipmemMenuBar/ClipmemMenuBarTests/HistoryModelTests.swift`.
- Design tokens from `macos/ClipmemMenuBar/ClipmemMenuBar/Views/DesignSystem.swift`.

**Test scenarios:**
- Happy path: source containing `**bold**` renders visible text without literal Markdown delimiters and carries a bold/strong presentation attribute.
- Happy path: source containing `*italic*` renders visible text without literal Markdown delimiters and carries an italic/emphasis presentation attribute.
- Happy path: source containing a heading such as `# Title` renders visible text as `Title` with heading/title presentation information or equivalent app styling metadata.
- Happy path: source containing `[Clipmem](https://example.com)` renders visible text as `Clipmem`, carries link metadata, and applies link-like styling.
- Edge case: plain text with no Markdown returns display text equivalent to the source.
- Edge case: empty or whitespace-only text stays readable and does not crash.
- Error path: malformed Markdown falls back to a readable plain-text representation rather than returning an empty attributed string.

**Verification:**
- Unit tests prove supported constructs, fallback behavior, and non-mutating source semantics.
- The new helper is included in the Xcode project for both app and test targets as needed.

---

- U2. **Render Markdown in result rows**

**Goal:** Use the shared renderer for the compact row title shown in the popover, History result list, and Quick Recall result list while preserving row density and interactions.

**Requirements:** R1, R3, R4, R5, R6, R7

**Dependencies:** U1

**Files:**
- Modify: `macos/ClipmemMenuBar/ClipmemMenuBar/Views/ResultRowView.swift`
- Test: `macos/ClipmemMenuBar/ClipmemMenuBarTests/MarkdownTextRendererTests.swift`

**Approach:**
- Replace the plain row-title `Text` construction with rendered display text from the shared helper.
- Preserve existing two-line limit, tail truncation, font scale, selected-row styling, hover help, score rendering, metadata, and row actions.
- Ensure headings do not create oversized rows; compact row typography should remain close to the current title weight and size.
- Keep context menu copy/restore behavior tied to existing plain model fields.

**Patterns to follow:**
- Existing row composition and row highlight style in `macos/ClipmemMenuBar/ClipmemMenuBar/Views/ResultRowView.swift`.
- Shared `ResultRowView` use from `macos/ClipmemMenuBar/ClipmemMenuBar/Views/MenuBarPanelView.swift`, `macos/ClipmemMenuBar/ClipmemMenuBar/Views/HistoryWindowView.swift`, and `macos/ClipmemMenuBar/ClipmemMenuBar/Views/QuickRecallWindowView.swift`.

**Test scenarios:**
- Happy path: a row title source with bold/italic/link Markdown renders through the same helper used by unit tests.
- Edge case: a heading in a row stays within the existing two-line row title constraints.
- Integration: popover, History results, and Quick Recall continue to share `ResultRowView`, so one row rendering change applies consistently across all result-list surfaces.
- Error path: malformed Markdown row text still displays source text and preserves `.help(...)` source text.

**Verification:**
- Manual UI inspection confirms recent rows remain compact, selectable, hoverable, and context-menu-capable.
- Existing row consumers require no model or action changes.

---

- U3. **Render Markdown in History detail content**

**Goal:** Render selected clipboard content with basic Markdown styling in the History detail pane while retaining text selection and copy behavior.

**Requirements:** R2, R3, R4, R5, R6

**Dependencies:** U1

**Files:**
- Modify: `macos/ClipmemMenuBar/ClipmemMenuBar/Views/SnapshotDetailView.swift`
- Test: `macos/ClipmemMenuBar/ClipmemMenuBarTests/MarkdownTextRendererTests.swift`

**Approach:**
- Use the shared renderer for the Content section's displayed body text.
- Preserve the existing copy button behavior so it copies the original source text selected by `bestText(from:)`.
- Keep text selection enabled for the rendered content if the platform path supports it cleanly; if rendering prevents reliable selection, preserve copy-button behavior and document the limitation in implementation notes.
- Use History's larger content area to show clearer heading hierarchy than compact rows, while keeping the existing content panel, padding, and background treatment.

**Patterns to follow:**
- Content section structure in `macos/ClipmemMenuBar/ClipmemMenuBar/Views/SnapshotDetailView.swift`.
- Existing `bestText(from:)` source priority and copy button behavior.

**Test scenarios:**
- Happy path: detail source with heading, bold, italics, and link renders without Markdown delimiters and keeps the expected visible text.
- Happy path: pressing/calling the copy behavior still uses the original source text rather than rendered text.
- Edge case: binary/image/PDF items with no extracted text still show the existing no-content empty state.
- Error path: malformed Markdown detail text remains visible as fallback source text.

**Verification:**
- History detail content shows Markdown styling, keeps existing metadata/data-format/event sections intact, and does not alter loaded `SnapshotDetails`.

---

- U4. **Document and release-note the UI behavior**

**Goal:** Record the user-facing behavior change in project documentation and release notes.

**Requirements:** R1, R2, R3, R4, R5

**Dependencies:** U2, U3

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `docs/menu-bar-app.md`
- Test: none

**Approach:**
- Add an `Unreleased` changelog entry under the appropriate category describing basic Markdown rendering in menu bar/History text displays.
- Update menu bar documentation only if the current docs describe displayed clipboard text in a way that should mention Markdown rendering; keep it brief and user-facing.
- Avoid documenting implementation details or unsupported Markdown internals.

**Patterns to follow:**
- Existing `CHANGELOG.md` categories and concise release-note style.
- Existing `docs/menu-bar-app.md` tone and limitations structure.

**Test scenarios:**
- Test expectation: none -- documentation and release-note updates do not introduce executable behavior.

**Verification:**
- Changelog entry is under `## Unreleased`.
- Documentation accurately states that rendering is visual-only and links are not clickable, if mentioned.

---

## System-Wide Impact

- **Interaction graph:** `ResultRowView` is shared by popover recents, History result lists, and Quick Recall result lists, so row rendering changes affect all three surfaces.
- **Error propagation:** Markdown parsing errors should be contained in the renderer and surface as plain text fallback, not user-visible errors.
- **State lifecycle risks:** No new persistent state, cache, database writes, or model mutation should be introduced.
- **API surface parity:** CLI output, JSON models, database APIs, and clipboard actions remain unchanged.
- **Integration coverage:** Unit tests cover renderer behavior; manual UI verification covers compact row density and History detail readability.
- **Unchanged invariants:** Source text remains the source of truth for copy, restore, search, export, hover help, and stored history.

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Native Markdown parsing supports more constructs than requested, creating surprising display differences. | Scope the helper to basic display behavior and keep unsupported constructs visually harmless; document only the supported subset. |
| Heading styling makes popover rows too tall. | Treat row headings as compact emphasis rather than full document headings. Verify row line limits and density manually. |
| Link styling implies clickability. | Keep links underlined/colored as requested but avoid pointer, navigation, or tap handling; document non-clickable behavior if docs mention links. |
| Attributed text selection behaves differently than plain `Text`. | Preserve copy-button/source behavior, and validate whether text selection still works acceptably during implementation. |
| Malformed Markdown hides or drops clipboard text. | Centralize fallback behavior and cover malformed input in renderer tests. |

---

## Documentation / Operational Notes

- Update `CHANGELOG.md` because this is a user-facing menu bar UI behavior change.
- Consider updating `docs/menu-bar-app.md` only with a short note; avoid expanding docs into a Markdown feature matrix.
- No migration, rollout flag, permissions, or operational change is required.

---

## Sources & References

- Brainstorm input: user-confirmed scope in this session.
- Related code: `macos/ClipmemMenuBar/ClipmemMenuBar/Views/ResultRowView.swift`
- Related code: `macos/ClipmemMenuBar/ClipmemMenuBar/Views/SnapshotDetailView.swift`
- Related code: `macos/ClipmemMenuBar/ClipmemMenuBar/Models/ClipmemModels.swift`
- Related code: `macos/ClipmemMenuBar/ClipmemMenuBar/Views/DesignSystem.swift`
- Related tests: `macos/ClipmemMenuBar/ClipmemMenuBarTests/HistoryModelTests.swift`
- External docs: [Apple Developer Documentation - Instantiating Attributed Strings with Markdown Syntax](https://developer.apple.com/documentation/foundation/attributedstring/instantiating_attributed_strings_with_markdown_syntax)
- External docs: [Apple Developer Documentation - SwiftUI Text](https://developer.apple.com/documentation/swiftui/text)
