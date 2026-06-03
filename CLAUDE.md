# pst — plain-simple-tickets

A very lightweight, git-friendly ticketing system meant to be driven by LLMs. The whole ticket
DB is one plain UTF-8 text file, **one line = one ticket**.

Full design: **`docs/DESIGN.md`** (read it before touching anything).

## Components
- `pst` — tiny Rust CLI (`src/`, deps: `clap` + `memchr` + `serde_json`) for safe, validated writes
  + `pst init` per-repo activation.
- `skills/pst/SKILL.md` — agent contract: read/git recipes + non-negotiables for driving the DB.
  Embedded in the binary; written to `.pst/skill.md` by `pst init`.
- `pst-mandate.md` — the always-on "track work as tickets, not plan-mode/TODO" directive.
  Embedded in the binary; written to `.pst/mandate.md` by `pst init` and replicated into each
  detected agent's native always-on instruction file.
- Distribution: Homebrew via sibling tap `../homebrew-tap` (`Formula/pst.rb`) installs a prebuilt
  binary built by `.github/workflows/release.yml` on `v*` tags (template: `.github/homebrew/pst.rb.tmpl`).
  `cargo install --git` or `brew install --HEAD` build from source. No install script — `pst init`
  does all per-repo work.
- Releasing: bump `version` in `Cargo.toml`, commit, then `git tag -a vX.Y.Z -m "pst X.Y.Z" &&
  git push origin main vX.Y.Z`. The tag triggers CI to build binaries and auto-update the tap formula
  (needs the `HOMEBREW_TAP_TOKEN` secret). Full steps in README "Releasing".

## Status
CLI implemented and tested (`cargo test`). Opt-in per-repo packaging across Cursor / Antigravity /
Claude Code / Codex / Copilot — see "Packaging & distribution" in `docs/DESIGN.md`.
