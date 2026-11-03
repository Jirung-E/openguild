# Quest CRUD, status, relations, due dates, history

## Quest CRUD

```bash
openguild quest list [--status open,in_progress] [--type DEV,BUG] [--urgency 1-2]
                      [--search "keyword"] [-q 'status:open -tag:wip due:overdue']
                      [--sort urgency,id --reverse]
                      [--table]     # aligned table for humans
                      [--json]      # machine-readable
openguild quest show <slug>                # summary — id/title/status/urgency + relation counts
openguild quest show <slug> --full         # everything (body, relations, tags, due dates)
openguild quest show <slug> --field <f>...  # only the given fields
openguild quest new --type <PREFIX> --title "..." [--description-file <PATH>]
                     [--urgency 1-4] [--parent <slug>]
openguild quest update <slug> [--title ...] [--description-file <PATH>] [--urgency N]
openguild quest delete <slug> --yes      # soft delete, restorable
openguild quest restore <slug>
openguild quest deleted                  # list soft-deleted quests
```

`--description-file`/`--file` REPLACES the whole description/body — it does
not append. To add to existing text, read the current value first and submit
the merged result.

## Status transitions

```
open ──▶ in_progress ──▶ testing ──▶ done
              ▲              │
              └── returned ◀─┘   (the human sent it back — read the comments, fix, retest)
      (cancelled / on_hold reachable from most states)
```

These are the default statuses; a guild can rename or add its own
(`openguild status list`).

## Changing the type

```bash
openguild quest retype DEV-001 BUG     # DEV-001 becomes the next BUG number, e.g. BUG-012
```

The quest **gets a new ID** — comments, memo, attachments, board position and
highlights follow it, but `[[DEV-001]]` written in other documents does not
change. Use the new ID from the output afterwards. Same type → no change.

```bash
openguild quest move <slug> <STATUS>   # case/spacing-insensitive: Open, in-progress, ...
openguild quest start <slug>           # shortcut for move → in_progress
openguild quest done <slug>            # shortcut for move → done
openguild quest reopen <slug>          # shortcut for move → open
```

`openguild quest status <slug>` is still a read-only lookup. Supplying the
optional `<STATUS>` argument still changes status, but that mutating form is
**deprecated**. Do not use it; use `move`/`start`/`done`/`reopen` instead.

**When to self-promote to `done` vs. stop at `testing`**: if the change is
fully covered by automated tests you already ran, go straight to `done`. If it
needs a human to look at it (UI/UX, anything not automatable), move to
`testing` instead and let the human promote it.

### Filtering by tag

`openguild quest list --tag backend` — with several (`--tag a,b`), only quests that have
**all** of them. Same rule as the tag row in the app, so the CLI and the screen agree.
`openguild library list --tag <tag>` does the same for library docs.

### Filtering with a query (`-q`)

One string instead of many flags — same syntax as the server's `?q=`:

```bash
openguild quest list -q 'status:[open, in_progress] -tag:wip urgency:1..=2 login'
openguild quest list -q '(tag:urgent | due:overdue) -status:done'
openguild quest list -q 'title:/^login/ due:2026-11-01..2026-12-01'
openguild quest list -q 'campaign:C-001 due:overdue'
openguild quest list -q 'parent:none type:BUG'
```

- Separate terms = AND (`&` / `AND` also work); `|` / `OR` = OR (uppercase only — `or` is a word);
  a leading `-` = NOT; group with `( )`. AND binds tighter: `a b | c` = `(a b) | c`.
- One of several values: `status:[open, in_progress]` (a bare comma is an error that suggests the brackets).
- Ranges: `a..b` (end excluded), `a..=b` (end included), `a..`, `..=b` — for `urgency` and every date key.
  Comparisons (`<=2`, `>=2026-11-01`) still work.
- Keyed text values match the whole value, ignoring case; `*` = any text (`tag:ui-*`); `/regex/` = a case-insensitive regex
  (bare words, `id`, `status`, `tag`, `type`, `folder`, `author`, `title`, `body`). `title:` / `body:` match a part by default;
  `=` makes them exact (`title:="Login screen"`).
- A bare word = the same text search as `--search` (title, body, id). Quote values with spaces: `title:"two words"` (tag names cannot contain spaces — `quest tag add X "a b"` adds two tags).
- Keys: `id` (`DEV-1`, `DEV-1..=DEV-9`, `DEV-*`, `/regex/`), `status` (slug, English or Korean name), `type`, `tag`,
  `campaign` (id or `none`), `urgency` (`2`, `1..=3`, `<=2`, `>=3`), `due` (`overdue`, `none`, `<=YYYY-MM-DD`, a range, …),
  `required_due`, `desired_due`, `campaign_end` (same values as `due`), `parent`, `child`, `prereq`, `successor` (id or `none`;
  `child:DEV-9` = quests with DEV-9 as a child), `discussion` (`unresolved`, `resolved`, `none`), `created`, `updated` (a date, or a time in a comparison or range: `>=2026-11-01T10:00`),
  `commented` (the **last comment**, same values as `created`, or `none`),
  `title`, `body`.
- `updated` moves when the document itself changes: title, body, urgency, status, parent, due dates — and on a campaign, its checklist
  (it is part of the body). Comments, tags, prerequisites and campaign links do not move it; use `commented` for comments.
- `due` is the effective due date: the earlier of the required due date and the end date of an
  active campaign the quest is in. `overdue` skips quests whose status counts as done.
  `required_due` and `campaign_end` look at one of those two only; `desired_due` is the only key for the desired date.
- Campaigns: `id`, `status`, `start`, `end`, `quest`, `checklist` (`open`, `done`, `none`), `created`, `updated`, `title`, `body`.
  Library: `id`, `tag`, `folder`, `created`, `updated`, `title`, `body`.
  Comments have their own keys (`openguild comments -q`, see `reference/comments.md`).
- An unknown key (typo) is an error that names the character position — quote text that
  contains a colon. `-q` is ANDed with the other flags.
- Over HTTP, a route that takes conditions answers **400** to a parameter it does not know
  (`/api/quests?nope=1`) instead of ignoring it and returning everything.

### Filtering by relation

`--parent <slug>` (same as `--child-of`) lists direct children of a quest; `--no-parent` the top-level
ones. `--has-child` / `--no-child` and `--has-prereq` / `--no-prereq` filter on having them. The query
keys `parent`, `child`, `prereq`, `successor` do the same and more.

### What changed since a date

A comment does **not** change the quest's `updated_at` — comments live in a separate file, so
`--updated-after` (and the app's "updated" range) miss quests that only got comments. "Changed
since D" means **either**, so run both and merge:

```bash
D=2026-10-20
{ openguild quest list --updated-after "$D" --id-only
  openguild quest list --commented-after "$D" --id-only; } | sort -u
```

`--commented-after` / `--commented-before` compare the **last** comment's time (same date/time
format as `--updated-after`), so "after D" = at least one comment since D and "before D" = quiet
since D (quests with no comments are left out). The query form is `commented:>=D` / `commented:<D`
/ `commented:none`. `quest list --json` rows carry `last_comment_at`.
Campaigns work the same way, with queries: `openguild campaign list -q 'updated:>=D'` and
`openguild comments -q 'campaign:* created:>=D'`.

### Attachments — read what is already there

`openguild quest show <slug>` only counts attachments (`attachments : 2`);
`quest show <slug> --full` and `openguild quest attach list <slug>` list them. They are files the
author added because the description does not carry them — specs, mockups, logs,
data samples. Read the relevant ones before starting the work; the path in the
listing is relative to `.guild/`.

```bash
openguild quest attach list DEV-001
cat .guild/attachments/spec-19401d58.pdf   # path comes from the listing
```

### Editing a body someone else may be editing — `--base-version`

A person may be editing the same quest in the app while you work. Read the version first and send it back
with your update — if the file changed in between, **nothing is saved**, the command exits non-zero and
prints the current content. Re-read, reapply your change, and try again. Without `--base-version` the update
overwrites whatever is there (old behavior).

```bash
V=$(openguild quest show DEV-002 --field version)
openguild quest show DEV-002 --field description > body.md   # edit body.md
openguild quest update DEV-002 --description-file body.md --base-version "$V"
```

`library update <BOOK-ID> --base-version N` and `rule set <slug> --base-version N` work the same way
(`library show` prints `version:`).

### Moving to testing — attach a test plan first

```bash
openguild quest update DEV-002 --description-file plan.md
openguild quest move DEV-002 testing
```

`plan.md` should contain the original description plus a `## Test plan`
section listing:
- exact steps to reproduce/verify
- expected vs. actual behavior
- any edge cases worth checking

## Relations

```bash
openguild quest parent <slug> <parent-slug>
openguild quest parent <slug> --detach
openguild quest prereq add <slug> <prereq-slug>
openguild quest prereq remove <slug> <prereq-slug>
```

## Due dates

```bash
openguild quest due <slug> --desired YYYY-MM-DD
openguild quest due <slug> --required YYYY-MM-DD
openguild quest due <slug> --clear-desired --clear-required
```

## History

```bash
openguild quest history <slug>
```

## Workflow patterns

```bash
# create, capture the new ID
id=$(openguild quest new --type DEV --title "Add search" --json --compact | jq -r .quest_id)

openguild quest start "$id"

# split into sub-quests
openguild quest new --type DEV --title "Search: indexing" --parent "$id"
openguild quest new --type DEV --title "Search: UI" --parent "$id"

openguild quest done "$id"

# file a bug found along the way
openguild quest new --type BUG --title "Crash on empty query"

# filter in-progress work
openguild quest list --status in_progress --json | jq -r '.[].quest_id'   # .id is the internal row number

# express a prerequisite
openguild quest prereq add DEV-049 DEV-048
```

## Fields (`--field`)

`--field` takes one or more names. A single field prints the bare value
(pipe friendly); several print `field: value` lines, and `--json` gives an
object.

```
id  title  status  status_ko  status_slug  urgency  description  type
parent  children  prerequisites  successors  created_at  updated_at  version
```

Relation fields print one slug per line — `--field children` is the way to
script over children/prereqs/successors.
