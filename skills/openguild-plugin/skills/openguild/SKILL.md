---
name: openguild
description: Manage project tasks with openguild (guild/quest/campaign tracker) — create and track quests, comments, campaigns, tags, and docs from the CLI. Use whenever the user asks to track work, file a bug/task, check project status, or run any `openguild` command — a `.guild` marker in the directory is a strong signal, but do NOT require one; also use when the user wants to set up / init a new guild in a project that has no `.guild` yet. Also use for quest/campaign comments or discussions, private memos, tags, due dates/deadlines, quest or campaign history, backups/restore/snapshots, quest templates, project rules or conventions docs (`rule`/`rules`), reference docs or notes library (`library`), work log or activity history (`worklog`), or any request to check/change quest status (open, in progress, testing, done).
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

**Locating a guild by name.** If the current directory has no `.guild` and
the user refers to a guild by name (e.g. "check my `foo` guild"), resolve its
path from `~/.openguild/recents.json` — an array of recently opened guilds,
`[{ "name": ..., "path": ..., "last_opened": ... }]`. Find the entry whose
`name` matches (confirm with the user on a partial/ambiguous match), then run
`openguild --guild <path> ...`. This is how the GUI/CLI remember guild
locations, so you don't need the user to give a full path.

## Before you start (do this first, every time)

1. **Check project rules before doing any real work**: run
   `openguild rule list` and read anything relevant (`openguild rule show
   <name>`) *before* writing code, filing a quest, or making a decision on
   this project. Rules encode this project's own conventions — they can
   override generic assumptions you'd otherwise make.
2. **Check the library BEFORE reading the code** (and before web search or
   asking): run `openguild library list` first, and `openguild library show
   <book-id>` on anything relevant, *before* diving into the source for a
   design/architecture/structure question. The library holds design
   invariants, decision records, and gotchas that the code alone does not
   show — skipping it and going straight to the code leads to violating
   decisions that only live in the library. Order: `library list` → read the
   relevant book → *then* the code. Only fall back to web search, general
   knowledge, or asking the user if the guild's rules/library don't answer it.

## What's worth recording (judgment calls)

Not every observation needs a quest/comment/rule/library entry — use these
per-type triggers, and skip everything else (don't spam the guild):

- **Quest**: file one when you find an actual bug/task that isn't already
  tracked and is a distinct scope from what you're doing right now. Don't
  file a quest for every minor observation along the way.
- **Comment**: add one on an existing quest when there's a "why"/blocker/
  decision the description doesn't capture and a future reader would need.
  Don't restate what's already in the description.
- **Rule**: propose one when the user corrects your approach, or states a
  policy/convention meant to apply going forward — not a one-off decision.
  Don't turn a single call into a permanent rule.
- **Library**: save research/design notes that can't be re-derived from the
  code. But even when something *could* technically be re-derived by reading
  the code, write it down anyway if the codebase is large/complex enough that
  re-deriving it would cost real time later (a module map, a non-obvious
  data-flow, a gotcha that took real digging to find) — it pays for itself by
  making the next problem faster to spot and saving the re-discovery work.
  For small/simple code, skip it — reading the code is faster than reading a
  library entry. This is also where large/binary material belongs — planning
  docs, spec PDFs, design mockups, zipped assets — via `library attach`
  rather than pasting into a quest description or the markdown body.

## Quest lifecycle (follow this order)

> **Keep the board honest — move the quest before and after you work.**
> The status is how the human tracks progress, so it must reflect reality:
> - **Before touching any code/files**, move the quest to **In Progress**.
>   Not after finishing, not "later" — first, then work. This is the step
>   agents skip most often, especially when doing several quests in a row:
>   do it again for *each* quest.
> - **When the implementation is done**, move it to **Testing**. Leave `done`
>   to the human unless automated tests fully cover it (see step 3).
>
> **Exception — the guild renamed or removed these statuses.** Statuses are
> per-guild configurable. Run `openguild status list` if unsure: if there is
> no In Progress / Testing equivalent, this rule does not apply — use the
> guild's own nearest stage, or leave the status alone rather than inventing
> one. Never create or rename statuses just to satisfy this rule.

1. **Create**: `openguild quest new --type <PREFIX> --title "..." [--description-file <PATH>] [--urgency 1-4] [--parent <slug>]`
   - Use `--description-file <UTF8-PATH>` for any non-ASCII body text — piping
     text through stdin/shell quoting can mangle encoding.
2. **Start work**: `openguild quest start <slug>` (→ In Progress) — *before*
   the first edit, not after.
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

## Cross-links `[[…]]` — follow them, don't skip them

Guild documents reference each other with double-bracket tokens. When you read
a quest body, comment, rule, or library doc and hit one of these, it is a
**pointer to another document in this same guild** — the author put it there
because that document matters here. **Fetch it** before acting, unless you
already know its content or it is clearly irrelevant to the current task.
Do not treat it as plain text, and do not guess what it says from the ID alone.

Resolve by prefix (`kind:` is optional; without it, infer from the ID shape):

| Token | Meaning | Read it with |
|---|---|---|
| `[[DEV-001]]`, `[[BUG-003]]`, `[[quest:DEV-001]]` | quest | `openguild quest show DEV-001` |
| `[[C-001]]`, `[[campaign:C-001]]` | campaign | `openguild campaign show C-001` |
| `[[BOOK-012]]`, `[[library:BOOK-012]]` | library doc | `openguild library show BOOK-012` |
| `[[some-rule]]`, `[[rules:some-rule]]` | project rule | `openguild rule show some-rule` |

Accepted prefixes: `quest`/`q`, `campaign`/`c`, `rule`/`rules`/`r`,
`book`/`library`/`lib`. Rule slugs may contain spaces (`[[coding style]]`).
A token pointing at something that doesn't exist is shown in red in the GUI —
if you meant to reference a real document, check the ID; if you are recording
a to-be-created item, that's fine.

**When writing**, link instead of duplicating: reference `[[BOOK-012]]` rather
than pasting its content into a quest description, and cross-link related
quests so the next reader (human or agent) can follow the trail.

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

# `--author` is REQUIRED on every comment-family command (see Pitfalls).
openguild quest comment add <slug> --author <name> --file <PATH>   # non-ASCII body → file
openguild quest comment list <slug>
openguild quest comment react <slug> <id> --emoji <emoji> --author <name>
openguild quest memo set <slug> --author <name> --file <PATH>   # private note, not shown to others
openguild quest tag add <slug> <tag...>
# Tags work the same on library docs and rules (list/add/remove/set).
openguild library tag add <book-id> <tag...>
openguild rule tag add <name> <tag...>

openguild campaign new --title "..." [--start YYYY-MM-DD] [--end YYYY-MM-DD]
openguild campaign link <campaign-slug> <quest-slug>
openguild campaign checklist add <campaign-slug> "text"

openguild comments --unresolved                  # cross-project comment search
openguild backup new                             # manual snapshot
```

## Pitfalls

| Pitfall | Fix |
|---|---|
| Writing a comment/memo/reaction without `--author` | Always pass `--author <your name>` — every comment-family command takes it. Without it the entry has no attribution, so nobody can tell later who said what (and you cannot filter with `comments --author`). Use the same name consistently for yourself |
| Piping non-ASCII text as a shell argument or via stdin can get mangled by console encoding | Always write the text to a UTF-8 file first and pass `--description-file` / `--file` |
| A `--description`/body value that starts with `-` gets misread as a flag | Use `--description-file <PATH>` (or `--description=...` with an equals sign) |
| Editing `.guild/**` frontmatter (status/urgency/parent/etc.) by hand | Always use the CLI — the frontmatter is derived state, not a plain file |
| Reading/grepping `.guild/**` files directly to inspect data (library docs, comments, quests) | Use the CLI for reads too — `library list`/`library show <book-id>`, `quest comment list <slug>`, `comments`, `quest show <slug>`. Raw files bypass sidecars/cache, so you miss reactions, discussion state, and ordering the user actually sees |
| Forgetting `--yes` on delete | `quest delete`/`campaign delete` refuse to run without it (dry-run works without) |
| Moving straight to `done` for changes only a human can verify | Move to `testing` with a "## Test plan" section instead, and let the human promote it to `done` |

## Full reference

Each topic below is its own file under `reference/` in this skill's
directory — load only the one you need:

- [reference/setup.md](reference/setup.md) — init, local vs. remote mode, env vars/flags
- [reference/quest.md](reference/quest.md) — quest CRUD, status transitions, relations, due dates, history, workflow examples
- [reference/campaign.md](reference/campaign.md) — campaigns, checklists, linking quests
- [reference/comments.md](reference/comments.md) — quest/campaign comments, memos, reactions, discussion threads
- [reference/templates.md](reference/templates.md) — quest templates
- [reference/rules.md](reference/rules.md) — project rules/convention docs
- [reference/library.md](reference/library.md) — reference notes library, folders, and file attachments (large/binary material)
- [reference/worklog.md](reference/worklog.md) — activity history and daily notes
- [reference/meta-and-maintenance.md](reference/meta-and-maintenance.md) — type/status/tag catalogs, reindex, drift checks, journal
- [reference/backup-and-safety.md](reference/backup-and-safety.md) — backup/restore, delete safety, never-hand-edit rule, error handling, JSON output

Any command also documents itself via `--help`.
