# pst — plain-simple-tickets

A very lightweight, git-friendly ticketing system designed to be driven by LLMs and
manipulated with ordinary coreutils (`grep`, `sed`, `awk`). The entire ticket database
is a single plain-text file: **one line = one ticket, line number = ticket number.**

## Goals & non-goals

**Goals**
- Tickets stored as one plain UTF-8 text file, trivially versioned in git with clean diffs.
- Readable and writable by an LLM agent using either the `pst` CLI or raw shell tools.
- A fast, tiny Rust CLI whose real job is *safe writes* and *validation* — reads are
  delegated to coreutils.
- Format invariants guaranteed by a git pre-commit hook.

**Non-goals**
- No web/desktop UI, no server, no daemon. Visualization/summarization is the agent's job.
- No reinvention of "classic" ticketing (separate title/description, timestamps, workflows).
  Timestamps come from git history via a documented `git blame`/`git log` recipe in `SKILL.md` —
  **not** a `pst` feature. `pst` shells out to nothing and links no git library.
- No multi-line ticket bodies (bulky context lives in detail files — see below).

## Data format

### The DB file
- Everything lives under a single hidden, namespaced folder `.pst/` so it drops cleanly into any
  host repo: DB at `.pst/tickets`, detail files under `.pst/details/`, any sidecar (offset index,
  schema/version) also under `.pst/`. Override the DB path with `--file <path>` or `$PST_FILE`;
  `details/` is always derived as a sibling of the DB file.
- Pure UTF-8 text. **No header line, no metadata line** — keeping the file pure is what makes
  `line N = ticket N` hold. Any future schema/version metadata lives in a sidecar under `.pst/`.
- Each ticket is exactly one `\n`-terminated line. The file always ends with `\n`.

### Line layout
Three fields separated by ASCII **RS** (Record Separator, `0x1E`, shown here as `␞`):

```
status␞tags␞body\n
```

| Field    | Validated | Rules |
|----------|-----------|-------|
| `status` | yes       | Strict enum: `open` \| `wip` \| `closed`. Required. **Fixed-width: exactly 6 bytes, right-padded with spaces** (`open··`, `wip···`, `closed`; `·` = space, `0x20`). The fixed width lets a status change be an in-place 6-byte write that shifts no later byte — see *Write discipline*. |
| `tags`   | yes       | Zero or more bare tokens separated by ASCII **US** (`0x1F`, `␟`). May be empty. |
| `body`   | yes       | Non-empty UTF-8, **single line**. Long is fine. |

Example (`␞` = RS, `␟` = US; `status` shown with its two trailing pad spaces made visible as `··`):

```
open··␞bug␟login␞Reject empty password on the login form; @alice hit a 500 in prod. parent:#42
```

### Hard invariants (enforced by `pst` and by `pst lint`)
1. Every line contains exactly two RS characters (→ exactly three fields).
2. RS (`0x1E`) is the field separator and US (`0x1F`) the tag separator — these are structural, never field
   content. `\n` (`0x0A`) only ever terminates a line.
3. `status` is exactly 6 bytes whose `trim_end`-of-spaces value is one of `open` / `wip` / `closed`
   (i.e. `open··`, `wip···`, or `closed`). Trailing spaces are pad, never other content.
4. `body` is non-empty.
5. The file ends with a trailing `\n`.

### Soft body conventions (greppable, NOT enforced)
The body is free text. These conventions exist purely so agents and `grep` can find things;
the CLI does not validate or require them:

- `@name` — a person. Regex `@[\w.-]+`.
- `#N` — a reference to ticket number N. Regex `#[0-9]+`.
- `key:#N` — a typed relation to ticket N, e.g. `parent:#42`, `blocks:#7`. Grep `parent:#` etc.

> `#` is available as the ticket-ref sigil because tags have their own dedicated field and
> carry no sigil. `►` is the documented fallback if `#`+digit collisions ever become a problem.

## Identity & lifecycle

- A ticket's number is its **1-based line number**. Line 1 = ticket 1.
- The file is **append-only**. A new ticket is a new line at EOF and receives the next number.
- **Lines are never deleted or reordered.** Doing so would renumber every later ticket and
  break every `#N` reference and the git history. This is a hard rule, checked by intent (the
  CLI never reorders) and partially by the hook (line-count / structural integrity).
- "Deleting" a ticket = setting its status to `closed` (a tombstone). The line and its number
  persist forever. Reopening sets the status back to `open` or `wip`.

## Detail files

Bulky context — for complex tasks or high-level "epic" tickets that parent many smaller ones —
lives in a separate Markdown file:

```
.pst/details/<N>-<slug>.md
```

- Optional. Most tickets need only their one-line body.
- The **number is the key**. The CLI locates a ticket's detail file by globbing
  `.pst/details/<N>-*.md`, so the slug part is cosmetic and may be changed/renamed freely.
- `slug` is derived by slugifying the body (lowercase, non-alphanumerics → `-`, collapsed),
  truncated to roughly the first few words / ~50 chars. An explicit slug may be passed instead.
- Child tickets reference an epic with the `parent:#N` body convention.

## CLI: `pst`

**Design principle: the CLI owns writes, bash owns reads.** A command exists only when doing it
by hand is genuinely unsafe or error-prone. Reading, filtering, counting and searching are
deliberately *not* commands — they are documented bash recipes in `SKILL.md`, e.g.:

```sh
sed -n '42p' .pst/tickets            # raw ticket 42
wc -l .pst/tickets                   # ticket count
grep -n 'parent:#42' .pst/tickets    # all children of epic 42
grep -n '^open' .pst/tickets         # all open tickets (status is the line prefix; pad/RS follow)
grep -n '@alice' .pst/tickets        # everything mentioning alice
```

### Core commands (each unsafe to do by hand)
| Command | Purpose |
|---------|---------|
| `pst add <body> [--tag T]... [--status S]` | Append a new ticket (default status `open`). Guarantees valid status, no control-char/newline injection, the trailing-`\n` so the new line can't fuse onto the last ticket, and a `flock`-guarded append. Prints the new number. |
| `pst set <N> [--status S] [--tag +T\|-T]... [--body TEXT]` | Edit fields of ticket N. All field edits flow through here. A **status-only** change is a length-stable in-place 6-byte write (fast path, no full rewrite); any edit touching `tags`/`body` can change line length and is a locked file rewrite. |
| `pst lint [--file PATH]` | Validate the whole DB file against every invariant. Exit non-zero on any violation. Used by the pre-commit hook. |

### Conveniences
| Command | Purpose |
|---------|---------|
| `pst close <N>` / `pst reopen <N>` / `pst wip <N>` | Sugar over `set --status` for the most common write (status change). |
| `pst show <N>` | Read ticket N, decode RS/US to readable separators, and append `.pst/details/<N>-*.md` if it exists — the one read that assembles more than `sed` gives you. (Last-change author/date is a `SKILL.md` git recipe, not built in.) |

> `ls` is intentionally omitted: it would be `grep` with lipstick. Filtering recipes live in `SKILL.md`.
>
> Git history is omitted for the same reason: "who/when last changed ticket N" is a *read*, so it
> belongs to the calling agent as a `SKILL.md` recipe (`git blame -L N,N -- .pst/tickets`). `pst`
> never spawns git and links no git crate.

### Write discipline
Every mutating command first acquires an advisory lock (`flock`) on the DB file so parallel agents
cannot corrupt it, then takes one of three write paths:

1. **Status-only `set` (incl. `close`/`reopen`/`wip`)** — locate line N's byte offset and `pwrite`
   the 6-byte status field in place. The fixed width means no later byte shifts, so `line N =
   ticket N` and any offset index stay valid. O(1) write regardless of file size.
2. **`add`** — append the new line with `O_APPEND` + `fsync`. O(1), no full rewrite.
3. **Any `tags`/`body` edit** — read the file, rebuild only the target line in a buffer, then
   rewrite the DB in place (`seek(0)` → `write_all` → `set_len(new_len)` → `fsync`). O(file size).

No temp-file + `rename` dance: it's overkill here. `flock` prevents concurrent corruption; git is
the recovery net for an interrupted write. All paths keep git diffs minimal — unrelated lines are
byte-for-byte unchanged.

Locating line N for paths 1/2 is a `memchr` newline scan today; a `.pst/` sidecar of per-line byte
offsets (u64/line) can make it O(1) later — and it survives status edits precisely because the
status field is length-stable.

## SKILL.md — the agent contract

The whole point of the natural text format is that an agent can drive it *directly*, using the
CLI only for writes. `SKILL.md` is therefore a first-class deliverable, not documentation
afterthought — it is the contract that makes raw access safe and effective. It must cover:

- The line format and the three hard invariants, stated tersely.
- The control-char literals an agent needs in shell: `$'\x1e'` (RS), `$'\x1f'` (US).
- The canonical read/filter/count recipes (the bash snippets above and a few more).
- The git-history recipes (reads the agent runs itself, since `pst` never touches git):
  last change → `git blame -L N,N --porcelain -- .pst/tickets` (`author`, `author-time`);
  creation → oldest entry of `git log -L N,N:.pst/tickets --format='%an %at %s' -s`.
- The soft body conventions: `@name`, `#N`, `key:#N`.
- The non-negotiable rules: **never delete or reorder lines**; "delete" = `pst close`; bulky
  context goes in `.pst/details/<N>-<slug>.md`; always write through `pst add`/`pst set` (never a
  raw `>>` that could fuse onto the last ticket).
- When to reach for the CLI vs. when bash is fine.

## Git pre-commit hook

A `pre-commit` hook runs `pst lint` against the staged DB file. It rejects:
- malformed lines (wrong field count),
- invalid `status`,
- stray embedded newlines in fields,
- a missing trailing newline.

This guarantees that every committed version of the file is structurally valid.

## Implementation & testing

- **Language:** Rust. Minimal dependencies — `clap` (args), `memchr` (SIMD separator/newline
  scanning). No `serde`, no `tempfile`, no mmap, no locking crate:
  - **Format handling is raw `&[u8]`**, never decoded to `str` to parse. Split on RS/US with
    `memchr`; match the 6-byte status by bytes.
  - **Locking** uses std `File::lock`/`try_lock` (stable since Rust 1.89 — `flock LOCK_EX` on
    Unix), so no external crate.
  - Single static binary, fast startup.
- **Tests:**
  - Format round-trip: encode → decode → identity.
  - Invariant enforcement: every malformed shape (wrong field count, bad status, wrong-width or
    non-space-padded status, empty body, missing trailing newline) is
    rejected.
  - Per-command effects on the file (add appends + numbers correctly; set edits only the
    target line; status shortcuts; tag add/remove).
  - Status fast path: a status-only `set`/`close`/`reopen`/`wip` changes exactly the 6 status
    bytes and leaves every other byte (incl. later lines' offsets) identical; a `tags`/`body`
    edit rewrites the file in place. Assert via byte-diff against the pre-edit file.
  - `lint` exit codes for valid and each invalid case.
  - Concurrent-write locking (two writers do not corrupt or interleave).
  - The `SKILL.md` bash recipes actually work: run each documented `grep`/`sed`/`wc` snippet
    against a sample DB and assert the expected result, so the agent contract can't silently rot.

## Open defaults (locked unless changed)

- Binary name: `pst`
- DB file: `.pst/tickets`
- Detail folder: `.pst/details/`

## Rust implementation notes (agent self-brief)

Tight rules for when we code this. Follow them; don't re-litigate.

**Deps:** `clap`, `memchr`. Nothing else unless a profile demands it.

**Do**
- Work in `&[u8]` end to end. Split fields with `memchr::memchr`/`memchr3`, count lines with
  `memchr::memchr_iter(b'\n', ..)`. Match the 6-byte status by byte compare against the 3 literals.
- Load whole file with `std::fs::read` for `lint` (one alloc, simple, fast).
- Lock with std `File::lock()` / `try_lock()` (Rust ≥1.89). No locking crate.
- Rewrite (tags/body edit) in place: `seek(0)` → `write_all(buf)` → `set_len(buf.len())` →
  `sync_all`. No temp file, no `rename`. `add` = `OpenOptions::append` + write + `sync_all`.
- Status fast path: scan to line N's offset, `pwrite`/`write_at` exactly 6 bytes. Touch nothing else.
- UTF-8 check (lint only): one `std::str::from_utf8` per body. Not a char loop.

**Don't**
- ❌ `mmap` — slower than `fs::read` for single-file scans (ripgrep's own benches) and adds
  unsafe + SIGBUS risk. Skip it.
- ❌ `regex`/`str::split`/`.chars()` on the hot path — `memchr` wins; no need to find char boundaries.
- ❌ `serde`, or decoding to `String` to parse.
- ❌ Full-file rewrite for a status-only change — that's the whole point of fixed-width status.
- ❌ Per-byte loops to find separators — let `memchr`'s SIMD do it (16+ bytes/step).

**Validation shortcut:** the forbidden field bytes (RS `0x1E`, US `0x1F`, GS `0x1D`, `\n`) are all
single bytes → one `memchr3`/`memchr` pass per field, no decode needed. UTF-8 validity is the only
thing that needs `from_utf8`.

**If `lint`/scan ever measures too slow** (unlikely <100ms @100MB single-threaded): split the buffer
at newline boundaries and `rayon`-parallelize; add `simdutf8` only if UTF-8 validation dominates a
profile. Both are last resorts, not defaults.

**If `show N`/`set N` line-location measures too slow:** add the `.pst/` u64-per-line offset index
(valid across status edits; patch/rebuild on tags/body edits). Not before.
