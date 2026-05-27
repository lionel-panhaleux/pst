# pst — plain-simple-tickets

A lean, git-friendly ticket DB: one UTF-8 text file, **one line = one ticket, line
number = ticket number**. Reads are plain coreutils; the `pst` CLI exists only for
*safe, validated writes*. Deps: `clap` + `memchr`, nothing else.

## Install

Two steps: install the binary once, then activate pst per-repo. No global agent state.

### 1. The binary

```
brew install lionel-panhaleux/tap/pst
```
or, with Rust:
```
cargo install --git https://github.com/lionel-panhaleux/pst
```

### 2. Per repo

```
pst init
```
Scaffolds `.pst/` (tickets, details, mandate, skill), installs the git pre-commit lint hook,
and writes one tool-native always-on instruction per detected agent. Supported:

| Tool | Writes |
|---|---|
| Cursor | `.cursor/rules/pst-mandate.mdc` (`alwaysApply: true`) |
| Antigravity | `.agents/rules/pst-mandate.md` |
| Claude Code | `.claude/settings.json` `SessionStart` hook (JSON-merged) + `.claude/hooks/pst-mandate.sh` |
| Codex | `.codex/config.toml` `developer_instructions` block (marker-delimited) |
| Copilot | `.github/copilot-instructions.md` (marker-delimited block) |

Auto-detects from existing config dirs/files. Force a target with `--cursor`, `--antigravity`,
`--claude`, `--codex`, `--copilot` (repeatable) — that also creates the tool's dir if absent.
`pst init --show` prints what's installed. `pst init --uninstall` removes only pst-owned files
and marker blocks; your `.pst/tickets` data, foreign hooks, and unrelated settings stay put.

## Line format
`status␞tags␞body` — fields split by RS (`0x1e`), tags by US (`0x1f`):
- `status`: 6-byte padded `open  ` | `wip   ` | `closed`
- `tags`: zero or more US-separated tokens (may be empty)
- `body`: non-empty single-line UTF-8

## Commands
```
pst add <body> [--tag T]... [--status S]            # append, prints the number
pst set <N> [--status S] [--tag +T|-T]... [--body TEXT]
pst close|reopen|wip <N>                             # status shortcuts
pst show <N>                                         # decoded ticket + detail file
pst lint                                             # validate the whole file
```
DB path: `--file` > `$PST_FILE` > `.pst/tickets`.

## Performance notes
All parsing is byte-level with SIMD `memchr` — bytes are never decoded to `str`.
Writes hold an advisory `flock` and take the cheapest path:
- **add** appends under `O_APPEND`; the ticket number comes from a streaming
  newline count (no full-file buffer).
- **status-only edits** (`close`/`reopen`/`wip`, `set --status`) are an O(1)
  in-place 6-byte `pwrite`, located by a streaming scan.
- **tag/body edits** rewrite only from the edited line to EOF; the unchanged
  prefix is left untouched on disk and never copied.

Append-only: lines are never deleted or reordered ("delete" = `close`). Locating a
line is currently a newline scan; a `.pst/` offset index would make it O(1) later.

See `docs/DESIGN.md` for rationale and `skills/pst/SKILL.md` for the agent contract / shell recipes.
