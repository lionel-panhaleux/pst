# pst multi-tool packaging — design

**Date:** 2026-05-27
**Status:** approved (design); paths verified against official publisher docs only.
**Repo:** `https://github.com/lionel-panhaleux/pst` · **Homebrew tap:** `lionel-panhaleux/homebrew-tap`

## Goal

Make `pst` (the CLI) plus its agent contract installable with a smooth, low-maintenance
experience across the major agentic-AI tools: **Claude Code, Cursor, OpenAI Codex CLI, Gemini
CLI, Google Antigravity** — without a release pipeline, prebuilt binaries, or duplicated
instruction files.

## Guiding principles

- **Two problems, kept separate:** (1) get the `pst` binary on PATH, (2) get one `SKILL.md` into
  wherever each tool looks. Never tangle them.
- **One source of truth for the skill.** Exactly one `SKILL.md` exists in the repo. Everything
  else points at it (manifests reference it; `install.sh` symlinks it). No copies, no sync step.
- **Build from source — no CI, no release artifacts.** `pst` is a ~300-line, 2-dependency Rust
  tool that compiles in seconds, so prebuilt binaries buy almost nothing and cost the most.
- **YAGNI.** Explicitly out of scope: GitHub Releases CI, cross-compiled binaries, `curl|sh`
  installer, a plugin-bundled binary shim, committed symlinks, copy/sync scripts.

## Verified facts (official docs only — blogs/community treated as moot)

| Tool | Skill/instruction delivery (official) | Frontmatter | Source |
|---|---|---|---|
| Claude Code | Plugin reads `skills/<name>/SKILL.md` at plugin root; marketplace via `.claude-plugin/marketplace.json` | `name` + `description` | code.claude.com/docs |
| Cursor | Reads `~/.agents/skills/` and `~/.cursor/skills/` (+ project `.agents/skills`, `.cursor/skills`) | `name` (must match folder) + `description` | cursor.com/docs/skills |
| Codex CLI | User-level skills at `$HOME/.agents/skills`; repo-level `<repo>/.agents/skills` | `name` + `description` | developers.openai.com/codex/skills |
| Antigravity | Global `~/.gemini/antigravity/skills/`; workspace `<root>/.agent/skills/` (**singular** `.agent`) | `description` required, `name` optional (defaults to dir) | Google codelab (developers.google.com) |
| Gemini CLI | Extension manifest `gemini-extension.json` with `contextFileName` pointing at a bundled `GEMINI.md`; install via `gemini extensions install <git-url> [--ref ...]` | n/a (context file, not a skill) | geminicli.com/docs/extensions |

Consequence: a single skill folder named `pst-tickets` (matching the existing frontmatter
`name: pst-tickets`, which satisfies Cursor's folder-match rule) symlinked into **two** global
dirs covers Cursor + Codex + Antigravity. Claude uses the plugin; Gemini uses the extension's
context file.

## Final repo layout

```
pst/
├── Cargo.toml, src/, tests/, docs/DESIGN.md       # unchanged
├── skills/
│   └── pst-tickets/
│       └── SKILL.md                # THE single skill file (moved from repo root)
├── .claude-plugin/
│   ├── plugin.json                 # repo root IS the Claude plugin
│   └── marketplace.json            # repo root IS its own 1-plugin marketplace
├── gemini-extension.json           # repo root IS the Gemini CLI extension
├── GEMINI.md                       # symlink → skills/pst-tickets/SKILL.md (extension context file)
├── AGENTS.md                       # short pointer; for Codex/Antigravity users who commit it, + contributors
├── install.sh                      # symlinks the skill into non-plugin tools' global dirs
├── packaging/
│   └── pst.rb                      # Homebrew formula (build-from-source); copied into the tap repo
└── README.md                       # install matrix
```

The repo root simultaneously *is* the Claude plugin and the Gemini extension — these are just
small manifest files at root and do not conflict. Both reference the one `skills/pst-tickets/SKILL.md`.

## Components

### 1. `skills/pst-tickets/SKILL.md` (moved, content unchanged)
The current root `SKILL.md`, relocated. Frontmatter already has `name: pst-tickets` +
`description`, satisfying every tool. This is the only instruction file with real content.

### 2. Claude Code: `.claude-plugin/plugin.json` + `marketplace.json`
- `plugin.json`: `name` `pst`, `description`, `version`, `author`. No `bin/` — binary installed
  separately (documented in README).
- `marketplace.json`: one-plugin marketplace whose plugin `source` is `.` (the repo itself).
- User install: `/plugin marketplace add lionel-panhaleux/pst` → `/plugin install pst`.
- The plugin picks up `skills/pst-tickets/SKILL.md` automatically.

### 3. Gemini CLI: `gemini-extension.json` + `GEMINI.md`
- Manifest: `name`, `version`, `description`, `contextFileName: "GEMINI.md"`. No `mcpServers`.
- `GEMINI.md` is a **symlink to `skills/pst-tickets/SKILL.md`** so the contract stays single-source.
  The skill's 2-line YAML frontmatter is injected as harmless leading text — acceptable.
  (Alternative if a symlink is undesirable in the extension clone: a thin pointer `GEMINI.md`;
  rejected to avoid rule drift.)
- User install: `gemini extensions install https://github.com/lionel-panhaleux/pst`.

### 4. Cursor / Codex / Antigravity: `install.sh`
A short, idempotent POSIX shell script whose entire job is to symlink the one skill file into the
official global dirs:
- `~/.agents/skills/pst-tickets/SKILL.md` → covers **Cursor and Codex**.
- `~/.gemini/antigravity/skills/pst-tickets/SKILL.md` → **Antigravity**.
- `~/.claude/skills/pst-tickets/SKILL.md` → optional, for Claude users who skip the plugin.

Behaviour: resolve the repo's absolute `skills/pst-tickets/SKILL.md`, `mkdir -p` each target
parent, `ln -sfn` the skill dir (or file). Re-runnable. Prints what it linked. Does **not**
install the binary and does **not** require any tool to be present (it just creates the dirs the
tools will read). macOS/Linux only; Windows is out of scope (the bash recipes in SKILL.md are
Unix-oriented anyway).

### 5. `AGENTS.md` (short)
A few lines: "this repo/agent can manage a `.pst/tickets` DB; full contract in the pst skill;
always write via the `pst` CLI, never raw `>>`." Serves Codex/Antigravity/Cursor users who prefer
committing an `AGENTS.md` into a host repo, and orients contributors to pst itself. Intentionally
NOT a second copy of the rules.

### 6. Binary distribution (build-from-source, zero infra)
- `cargo install --git https://github.com/lionel-panhaleux/pst` — works with no crates.io
  publish and no CI. Requires a Rust toolchain.
- Homebrew: `packaging/pst.rb`, a build-from-source formula (`depends_on "rust" => :build`,
  `cargo install` into `bin`). Copied into the `lionel-panhaleux/homebrew-tap` repo (Homebrew
  requires the tap repo be named `homebrew-*`). User install:
  `brew install lionel-panhaleux/tap/pst`.
- No prebuilt binaries, no Releases, no installer script for the binary.

### 7. `README.md`
The install matrix: per-tool one-liners for the skill + the two binary options, plus the existing
format/usage summary.

## Data / control flow (install)

1. **Binary:** user runs `brew install lionel-panhaleux/tap/pst` *or* `cargo install --git …`.
   `pst` lands on PATH. Done once per machine.
2. **Skill, per tool:**
   - Claude → `/plugin install pst` (reads repo `skills/`).
   - Gemini → `gemini extensions install <git-url>` (reads bundled `GEMINI.md`).
   - Cursor/Codex/Antigravity → clone repo once, run `./install.sh`; symlinks make the skill
     globally available across all the user's projects.
3. **Usage:** agent reads the skill, performs reads with coreutils, writes via `pst`.

## Testing / verification

- `install.sh`: run it, assert the three symlinks exist and resolve to the canonical SKILL.md;
  re-run, assert idempotence (no error, no duplication).
- Manifests: validate `plugin.json`, `marketplace.json`, `gemini-extension.json` are well-formed
  JSON with the required fields per the official schemas above.
- `GEMINI.md` symlink resolves to `skills/pst-tickets/SKILL.md`.
- Homebrew formula: `brew install --build-from-source ./packaging/pst.rb` (or tap) produces a
  working `pst` (smoke: `pst --help`). Can defer actual tap-repo creation to push time.
- Existing Rust test suite is unaffected by the `SKILL.md` move (`tests/recipes.rs` uses an inline
  `tests/fixtures/sample.tickets` fixture, not the skill file) — confirm it still passes.
- Update prose references to the relocated skill (`README.md`, `CLAUDE.md`) to point at
  `skills/pst-tickets/SKILL.md`.

## Open / deferred (not blocking)

- Creating the `lionel-panhaleux/homebrew-tap` repo and pushing `pst.rb` happens at/after the
  first push (user will handle the tap repo).
- If non-compiling installs are ever wanted (Windows, no-toolchain users), add `cargo-dist` later
  — one tool generates Releases CI + `curl|sh` installer + brew formula from one config. Out of
  scope now.
</content>
</invoke>
