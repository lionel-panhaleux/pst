# Working on pst

This repo builds **pst** — a git-friendly, one-line-per-ticket work tracker for AI agents.

- Read `CLAUDE.md` for orientation and `docs/DESIGN.md` for the full design and invariants.
- The CLI lives in `src/` (Rust, deps `clap` + `memchr` + `serde_json`); tests in `tests/`.
  Run `cargo test`.
- The agent contract for *using* pst is `skills/pst/SKILL.md` — embedded in the binary; written to
  `.pst/skill.md` by `pst init`.
- Packaging is **opt-in per-repo** via `pst init [--cursor|--antigravity|--claude|--codex|--copilot]`.
  No global agent state. Embedded artifacts: `pst-mandate.md`, `skills/pst/SKILL.md`,
  `.pst-hooks/pre-commit`. Homebrew formula: sibling `../homebrew-tap`.

This is the pst source repo, so it has no `.pst/tickets` of its own unless you `pst init` it.
