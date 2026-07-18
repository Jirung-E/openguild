---
name: openguild
description: Manage this project's tasks with openguild (guild/quest/campaign tracker) — create and track quests, comments, campaigns, tags, and docs from the CLI. Use whenever the user asks to track work, file a bug/task, check project status, or run any `openguild` command in a directory containing a `.guild` marker.
when_to_use: >
  Also use for: quest/campaign comments or discussions, private memos, tags,
  due dates/deadlines, quest or campaign history, backups/restore/snapshots,
  quest templates, project rules or conventions docs (`rule`/`rules`),
  reference docs or notes library (`library`), work log or activity history
  (`worklog`), or any request to check/change quest status (open, in
  progress, testing, done).
---

# openguild usage

openguild is a local-first CLI/GUI project tracker. A **guild** is one project
(a directory with a `.guild` marker file). A **quest** is a single task/issue
inside a guild, with an auto-incremented ID like `DEV-001` or `BUG-003`.

## Setup

```bash
cd /path/to/project
openguild init [--name "My Project"]   # creates the .guild marker
openguild quest list                   # auto-discovers .guild from cwd
```

Remote mode (HTTP to a hosted server) is also supported:
```bash
openguild --remote https://host/path quest list
```

Global flags: `--guild <PATH>` (explicit guild path), `--json` (machine
output), `--compact` (single-line JSON, requires `--json`).

## Quest lifecycle (follow this order)

1. **Create**: `openguild quest new --type <PREFIX> --title "..." [--description-file <PATH>] [--urgency 1-4] [--parent <slug>]`
   - Use `--description-file <UTF8-PATH>` for any non-ASCII body text — piping
     text through stdin/shell quoting can mangle encoding.
2. **Start work**: `openguild quest start <slug>` (→ In Progress).
3. **Finished**:
   - If fully covered by automated tests you already ran: `openguild quest done <slug>`.
   - If it needs a human to look at it (UI, UX, anything not automatable):
     add a "## Test plan" section to the description first, then
     `openguild quest move <slug> testing`. Never move to `done` yourself in
     that case — that's reserved for the human reviewing the result.
4. Status can also be set directly with `openguild quest move <slug> <STATUS>`
   (accepts `Open`/`open`/`In Progress`/`in_progress`/`in-progress`, etc. —
   case/spacing-insensitive). The old `quest status <slug> <STATUS>` form is
   deprecated; use `move`.

## Everyday commands

```bash
openguild quest list [--status open,in_progress] [--type DEV,BUG] [--urgency 1-2]
                      [--search "keyword"] [--sort urgency,id --reverse]
                      [--table]                 # aligned table for humans
                      [--json]                  # machine-readable
openguild quest show <slug> [--field title]
openguild quest update <slug> [--title ...] [--description-file <PATH>] [--urgency N]
openguild quest delete <slug> --yes             # soft delete, restorable
openguild quest restore <slug>

openguild quest comment add <slug> --author <name> --file <PATH>   # non-ASCII body → file
openguild quest comment list <slug>
openguild quest memo set <slug> --file <PATH>    # private note, not shown to others
openguild quest tag add <slug> <tag...>

openguild campaign new --title "..." [--start YYYY-MM-DD] [--end YYYY-MM-DD]
openguild campaign link <campaign-slug> <quest-slug>
openguild campaign checklist add <campaign-slug> "text"

openguild comments --unresolved                  # cross-project comment search
openguild backup new                             # manual snapshot
```

## Pitfalls

| Pitfall | Fix |
|---|---|
| Piping non-ASCII text as a shell argument or via stdin can get mangled by console encoding | Always write the text to a UTF-8 file first and pass `--description-file` / `--file` |
| A `--description`/body value that starts with `-` gets misread as a flag | Use `--description-file <PATH>` (or `--description=...` with an equals sign) |
| Editing `.guild/**` frontmatter (status/urgency/parent/etc.) by hand | Always use the CLI — the frontmatter is derived state, not a plain file |
| Forgetting `--yes` on delete | `quest delete`/`campaign delete` refuse to run without it (dry-run works without) |
| Moving straight to `done` for changes only a human can verify | Move to `testing` with a "## Test plan" section instead, and let the human promote it to `done` |

For the full command catalog (relations, due dates, history, campaigns,
backups, comment filters/reactions, templates, maintenance, rules, library,
worklog, safety guards, error handling, JSON usage), see `REFERENCE.md` in
this skill's directory. Any command also documents itself via `--help`.
