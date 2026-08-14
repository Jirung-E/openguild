# Quest CRUD, status, relations, due dates, history

## Quest CRUD

```bash
openguild quest list [--status open,in_progress] [--type DEV,BUG] [--urgency 1-2]
                      [--search "keyword"] [--sort urgency,id --reverse]
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
              │              ▲
              └──────────────┘
      (cancelled / on_hold reachable from most states)
```

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
openguild quest list --status in_progress --json | jq '.[] | .id'

# express a prerequisite
openguild quest prereq add DEV-049 DEV-048
```

## Fields (`--field`)

`--field` takes one or more names. A single field prints the bare value
(pipe friendly); several print `field: value` lines, and `--json` gives an
object.

```
id  title  status  status_ko  status_slug  urgency  description  type
parent  sub_quests  prerequisites  successors  created_at  updated_at
```

Relation fields print one slug per line — `--field sub_quests` is the way to
script over children/prereqs/successors.
