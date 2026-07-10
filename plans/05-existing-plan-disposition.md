# Existing plan disposition

The six files under `docs/plans` are useful historical design records, but current code indicates the planned work is substantially implemented. They should not remain indistinguishable from pending plans.

## Decisions

| Existing plan | Current evidence | Disposition |
|---|---|---|
| `2026-04-28-001-feat-markdown-rendering-plan.md` | `MarkdownTextRenderer.swift`, usage in result/detail views, and `MarkdownTextRendererTests.swift` implement the core renderer and fallback behavior. | Mark **implemented**; archive as historical. Keep any explicitly deferred Markdown syntax as a new scoped issue only if still desired. |
| `2026-04-28-002-feat-command-click-markdown-links-plan.md` | `CommandClickableMarkdownText.swift`, link target classification/opening, metadata badges, and `MarkdownLinkActionTests.swift` implement command-click behavior. | Mark **implemented**; archive. |
| `2026-04-28-003-feat-command-link-hover-cursor-plan.md` | The link monitor tracks hit targets and updates/restores cursor; tests cover hit testing. | Mark **implemented**; archive. |
| `2026-04-29-001-feat-agent-native-completeness-plan.md` | `agents context`, OpenClaw/Hermes management/validation, action parity docs, skills, schemas, and parity tests exist. | Mark **implemented with follow-up superseded**. Do not rerun its broad architecture project. |
| `2026-04-29-002-feat-close-agent-native-audit-gaps-plan.md` | Current agent package/doctor/context/capability surfaces and tests cover the named gap-closing direction. | Mark **implemented**; any residual parity failure should be a focused bug, not continuation of the old plan. |
| `2026-05-11-001-fix-history-image-quick-reuse-plan.md` | Detail selects previewable image reps and exports to a temp file for `NSImage`; image copy falls back to exact snapshot restore. | Mark **implemented but superseded for performance** by plans 3, 6, and 8. The user-facing outcome exists; the current BLOB/subprocess path is inefficient and cache invalidation is weak. |

## Required plan lifecycle convention

Add front matter or a fixed status block to every plan:

```yaml
status: proposed | accepted | in-progress | implemented | superseded | abandoned
created: YYYY-MM-DD
last-verified: YYYY-MM-DD
implemented-in: <release/commit/PR, when known>
superseded-by: <plan path, when applicable>
owners: []
```

A plan marked implemented should state:

- which acceptance criteria landed;
- which were intentionally changed;
- which deferred items became separate issues/plans;
- the code/tests that demonstrate completion.

## Modifications to assumptions in old plans

- Do not treat subprocess-per-operation as permanent merely because old UI plans used `ClipmemClient`; plan 7 makes transport a measured decision.
- Do not extend temp-file image preview as the archive read API; plans 3/6/8 replace it with targeted, version-keyed payload access.
- Keep Markdown/link components; they are localized and well tested. They are not an architectural problem worth rewriting.
- Preserve agent CLI/output contracts while underlying application/persistence services change.

## Documentation cleanup

Move completed files to `docs/plans/archive/` or keep them in place with unmistakable status. Update `docs/architecture.md` after the source-preserving image decision, because its current “no compression” statement conflicts with implemented behavior and the raw-byte promise.
