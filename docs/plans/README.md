# Plan lifecycle

Plans preserve intent and evidence; they are not perpetual task lists. Every
new plan uses YAML front matter with these fields:

```yaml
status: proposed | accepted | in-progress | implemented | superseded | abandoned
created: YYYY-MM-DD
last-verified: YYYY-MM-DD
implemented-in: <release, commit, or PR when known>
superseded-by: <plan path when applicable>
owners: []
```

An implementation PR changes the status and records which acceptance criteria
landed, intentional changes, deferred follow-ups, and the code/tests that prove
completion. `implemented` means the outcome exists. `superseded` means a newer
plan owns any remaining work. Historical files stay in this directory so links
remain stable, but their status must make them unmistakable from pending work.

## Historical disposition

| Plan | Disposition and completion evidence |
|---|---|
| Markdown rendering | Implemented by `MarkdownTextRenderer` and its Swift tests; unsupported syntax remains independently scoped. |
| Command-click links | Implemented by `CommandClickableMarkdownText`, target classification, and `MarkdownLinkActionTests`. |
| Link hover cursor | Implemented by the link monitor and hit-testing tests. |
| Agent-native completeness | Core outcome implemented; the umbrella is superseded by the focused gap-closing plan. |
| Agent-native gap closing | Implemented by context/capability commands, package doctors, schemas, parity tests, and current docs. Residual parity failures are bugs. |
| History image quick reuse | User outcome implemented, but its temp-file transport/performance approach is superseded by Plans 3, 6, and 8. |
