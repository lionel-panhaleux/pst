# pst — plain-simple-tickets

A lean, git-friendly ticket DB: one UTF-8 text file, **one line = one ticket, line
number = ticket number**. Reads are plain coreutils; the `pst` CLI exists only for
*safe, validated writes*. Deps: `clap` + `memchr`, nothing else.

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

See `docs/DESIGN.md` for rationale and `SKILL.md` for the agent contract / shell
recipes. Install the format guard: `ln -sf ../../.pst-hooks/pre-commit .git/hooks/pre-commit`.
