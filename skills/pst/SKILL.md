---
name: pst
description: Use whenever planning or tracking work in a repo that has a `.pst/tickets` database, or when reading, filtering, or writing its tickets — the plain-text one-line-per-ticket system. In a pst repo, track work as tickets instead of plan-mode scratch plans or TODO markdown files. Reads use coreutils; writes MUST go through the `pst` CLI.
---

# pst — driving the ticket DB

The DB is one UTF-8 file, `.pst/tickets`, **one line = one ticket, line number = ticket number**.
Each line is plain TSV — three fields separated by TAB (`\t`), tags within field 2 comma-separated:

```
status<TAB>tags<TAB>body
```

- `status` — exactly 6 bytes: `open  `, `wip   `, or `closed` (right-padded with spaces).
- `tags` — zero or more comma-separated bare tokens; may be empty.
- `body` — non-empty single-line UTF-8, no TAB.

## Hard invariants (never violate)
1. Exactly two TABs per line (three fields).
2. TAB is the field separator and `,` the tag separator — neither may appear inside field content.
   `\n` only terminates a line.
3. `status` is one of the three 6-byte literals above.
4. `body` is non-empty.
5. The file ends with `\n`.

## Work tracking — tickets, not plan mode
In a pst repo, the ticket DB **is** your work tracker. Track multi-step work as tickets, not
plan-mode scratch plans or a TODO markdown file. Read the board first (`grep -n '^open\|^wip'
.pst/tickets`), continue existing tickets before opening new ones, `pst wip <N>` on start and
`pst close <N>` when done. One epic ticket + `parent:#N` children for larger efforts; bulky
context only in `.pst/details/<N>-<slug>.md` — the one sanctioned markdown doc.

## Reading (`pst show` for a ticket; coreutils for filtering — there is no `pst ls`)
```sh
pst show 42                                         # read ticket 42: formatted + its detail file
pst show 42 7 '#13'                                 # several at once; a '#'-prefix is fine
sed -n '42p' .pst/tickets                           # raw ticket 42 (one line, no detail file)
wc -l < .pst/tickets                                # ticket count
grep -n '^open' .pst/tickets                        # open tickets (status is the line prefix)
grep -n '^wip' .pst/tickets                         # work-in-progress
grep -nc 'parent:#42' .pst/tickets                  # count children of epic 42
grep -n '@alice' .pst/tickets                       # everything mentioning alice
awk -F'\t' '$2 ~ /(^|,)login(,|$)/' .pst/tickets    # all tickets tagged 'login' (exact-tag match)
awk -F'\t' '{print NR": "$3}' .pst/tickets          # number + body only (drop status/tags)
```

A ticket's number is its **line position**, not text stored in the line. To read ticket N use
`pst show N` (or `sed -n 'Np'`) — `grep '#N'` finds tickets that *reference* N (the `#N` body
convention below), never ticket N itself.

## Git-history recipes (pst never touches git — you run these)
```sh
git blame -L 42,42 --porcelain -- .pst/tickets          # last change: author, author-time
git log -L 42,42:.pst/tickets --format='%an %at %s' -s   # full line history; oldest = creation
```

## Writing (always via the CLI — never raw `>>`)
```sh
pst add "Reject empty password" --tag bug --tag login   # append; prints the new number
pst set 42 --status wip                                  # or: pst wip 42
pst set 42 --tag +urgent --tag -login --body "New text"  # tag add/remove + body edit
pst close 42      # tombstone (status=closed); pst reopen 42 to revive
pst lint          # validate the whole file (used by the pre-commit hook)
```

## Soft body conventions (greppable, NOT enforced)
- `@name` — a person (`@[\w.-]+`).
- `#N` — a reference to ticket N (`#[0-9]+`).
- `key:#N` — a typed relation, e.g. `parent:#42`, `blocks:#7`.

## Detail files
Bulky context for an epic or complex ticket lives in `.pst/details/<N>-<slug>.md`.
The **number is the key**; the slug is cosmetic. Create/edit these by hand.

## Non-negotiable rules
- **Never delete or reorder lines** — it renumbers every later ticket and breaks `#N` refs and git history.
- "Delete" a ticket = `pst close N` (a tombstone). The line and number persist forever.
- Always write through `pst add` / `pst set` (a raw `>>` can fuse onto the last ticket if the trailing `\n` is missing).
- Bulky context goes in `.pst/details/<N>-<slug>.md`, never as extra lines.
