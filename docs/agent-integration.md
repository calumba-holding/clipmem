# Agent integration

clipmem is designed as a JSON-first CLI that agents — OpenClaw and
others — can call directly. You can also install a packaged skill that
gives agents structured knowledge of clipmem's commands, filters, and
output schemas.

## Install the OpenClaw skill

Install the packaged skill using the CLI rather than copying files
manually:

```bash
clipmem agents openclaw install-skill
```

By default this installs into the active OpenClaw workspace:

```
~/.openclaw/workspace/skills/clipboard-memory
```

The workspace root is resolved from
`openclaw config get agents.defaults.workspace`, falling back to
`~/.openclaw/workspace`.

To install into the shared OpenClaw skills directory instead:

```bash
clipmem agents openclaw install-skill --shared
# writes to ~/.openclaw/skills/clipboard-memory
```

Or to a specific destination:

```bash
clipmem agents openclaw install-skill --dest /path/to/skill --force
```

## What the skill ships

```
SKILL.md
references/commands.md
references/json-schema.md
references/examples.md
references/setup-check.md
references/troubleshooting.md
scripts/check-setup.sh
```

The packaged skill points agents at the JSON-first retrieval commands:

- `clipmem recall "<query>" --format json` — start here
- `clipmem recall --prefer-recent --hours 24 --format json` — for
  recent items without a specific query
- `clipmem timeline ... --format json` — when chronology matters
- `clipmem search ... --format json` — when direct lexical matching
  matters
- `clipmem get <snapshot-id> --format json` — when deeper nested
  detail is needed

## Manage the skill

Check integration health:

```bash
clipmem agents openclaw doctor
```

Print the packaged SKILL.md for inspection:

```bash
clipmem agents openclaw print-skill
```

Uninstall the skill:

```bash
clipmem agents openclaw uninstall-skill
```

## Troubleshooting

OpenClaw must be able to run `clipmem` from its own environment, not
just from your interactive shell.

If you installed `clipmem` into `~/.local/bin`, make sure that
directory is on the PATH seen by OpenClaw. If sandboxing is active,
the binary may also need to be available inside the sandbox image or
container.

Start with:

```bash
clipmem agents openclaw doctor
openclaw sandbox explain   # when available
```

For empty results, sandbox visibility, or binary-only snapshots where
exact text isn't available, see
[troubleshooting](troubleshooting.md).

## Example agent prompts

Once the skill is installed, agents can handle prompts like:

- "Find that ffmpeg command I copied yesterday."
- "Search my clipboard history for the SQL migration with WAL mode."
- "What was the URL I copied from Safari about objc2 `NSPasteboard`?"
- "Show me the full clipboard entry for snapshot 128."

## Skill packaging

The skill exists in three locations within the repository:

- `skills/clipboard-memory/` — the canonical cross-agent skill source
  that repo-based installers discover from the root.
- `extras/openclaw/clipboard-memory/` — the OpenClaw-native package
  installed by `clipmem agents openclaw install-skill`.
- `extras/agent-skills/clipboard-memory/` — a portable packaging
  mirror for generic agent-skill runtimes beyond OpenClaw.

For compatibility, `./scripts/install_openclaw_skill.sh` still exists
as a thin wrapper around
`clipmem agents openclaw install-skill --shared --force`.

## Next steps

- [Output formats](output-formats.md) — understand the JSON envelope,
  flattened text fields, and TOON skim output
- [Command reference](command-reference.md) — exhaustive flag-level
  reference for every command
- [Troubleshooting](troubleshooting.md) — diagnose empty results,
  PATH issues, and database problems
