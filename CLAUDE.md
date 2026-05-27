# pst — plain-simple-tickets

A very lightweight, git-friendly ticketing system meant to be driven by LLMs. The whole ticket
DB is one plain UTF-8 text file, **one line = one ticket**.

Full design: **`docs/DESIGN.md`** (read it before touching anything).

## Components
- `pst` — tiny Rust CLI (deps: `clap` + `memchr`) for safe, validated writes to the tickets file.
- `SKILL.md` — agent contract: read/git recipes + non-negotiables for LLMs driving the DB directly.

## Status
Design complete & committed (`docs/DESIGN.md`). Nothing built yet — next step is the implementation
plan from the design.
