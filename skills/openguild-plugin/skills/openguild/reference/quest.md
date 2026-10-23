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
openguild quest list -q 'status:open,in_progress -tag:wip urgency:<=2 login'
openguild quest list -q 'campaign:C-001 due:overdue'
openguild quest list -q 'parent:none type:BUG'
```

- Commas inside one key = OR; separate terms = AND; a leading `-` = NOT; a bare word = the
  same text search as `--search` (title, body, id). Quote values with spaces: `tag:"two words"`.
- Keys: `status` (slug, English or Korean name), `type`, `tag`, `campaign` (id or `none`),
  `urgency` (`2`, `1-3`, `<=2`, `>=3`), `due` (`overdue`, `none`, `<=YYYY-MM-DD`, …), `parent` (id or `none`),
  `commented` (date of the **last comment**: `>=YYYY-MM-DD`, `<YYYY-MM-DD`, a date, or `none`).
- `due` is the effective due date: the earlier of the required due date and the end date of an
  active campaign the quest is in. `overdue` skips quests whose status counts as done.
- An unknown key (typo) is an error that names the character position — quote text that
  contains a colon. `-q` is ANDed with the other flags.

### What changed since a date

A comment does **not** change the quest's `updated_at` — comments live in a separate file, so
`--updated-after` (and the app's "updated" range) miss quests that only got comments. "Changed
since D" means **either**, so run both and merge:

```bash
D=2026-10-20
{ openguild quest list --updated-after "$D" --id-only
  openguild quest list -q "commented:>=$D" --id-only; } | sort -u
```

`commented:>=D` compares the date of the **last** comment, so it is "at least one comment on or
after D". `commented:<D` finds quests that have gone quiet; `-commented:none` = has comments.
Campaigns work the same way but have no filter: compare `updated_at` from
`openguild campaign list --json` with the `ts` of `openguild campaign comment list <slug> --json`
entries.

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
parent  sub_quests  prerequisites  successors  created_at  updated_at  version
```

Relation fields print one slug per line — `--field sub_quests` is the way to
script over children/prereqs/successors.
