## Work tracking — use pst tickets

If this repository has a `.pst/tickets` file, it uses **pst** as the single source of truth for
work tracking. In that case:

- **Track all multi-step work as pst tickets** — not plan-mode scratch plans, not a TODO markdown
  file, not an in-chat checklist. When you would enter plan mode or open a `*.md` to list steps,
  create tickets instead.
- **Start of a task:** read the board first — `grep -n '^open\|^wip' .pst/tickets`. Continue
  existing tickets before creating new ones.
- **Read ticket N:** `pst show N` (several at once: `pst show N M`; a `#`-prefix is fine). The
  number is the **line position**, not text in the file — so grepping `#N` finds tickets that
  *reference* N, never ticket N itself.
- **Decompose:** one ticket per discrete unit of work; for a larger effort, one epic ticket with
  `parent:#N` children.
- **Lifecycle:** `pst wip <N>` when you start a ticket, `pst close <N>` when it's done.
- **Bulky context** (epic or genuinely complex ticket only) lives in `.pst/details/<N>-<slug>.md`
  — the only sanctioned markdown tracker. No separate plan/TODO doc.

See `.pst/skill.md` for the line format, shell read recipes, and the non-negotiable
write rules (never delete/reorder lines; always write via the `pst` CLI).
