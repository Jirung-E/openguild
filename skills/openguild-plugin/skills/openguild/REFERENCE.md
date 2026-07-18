# openguild — full command reference

This is the detailed reference for the `openguild` skill. `SKILL.md` covers
the everyday lifecycle and pitfalls; this file has the full command catalog,
workflow patterns, safety guards, and error handling.

## 1. Setup

### 1.1 Initialize a guild (once)

```bash
cd /path/to/your-project
openguild init                      # directory name becomes the guild name
openguild init --name "My Project"  # explicit guild name
```

### 1.2 Modes

**Local (default, recommended)** — no server needed. Auto-discovers `.guild`
from cwd and calls core services directly.

```bash
cd /path/to/your-project
openguild quest list
openguild --guild ./other-project quest list   # explicit guild path
```

**Remote** — HTTP to a hosted server.

```bash
openguild --remote https://host/path quest list
# or
export OPENGUILD_REMOTE=https://host/path
openguild quest list
```

To run your own server:
```bash
GUILD_PATH=/path/to/your-project cargo run --bin openguild-server -- host
# in another terminal:
openguild --remote http://localhost:3000 ping
```

Maintenance/diagnostics (no server needed):
```bash
openguild backup new   # immediate snapshot
openguild info         # guild meta / DB size / backup status
```

### 1.3 Env vars / global flags

| Item | Default | Description |
|---|---|---|
| env `OPENGUILD_REMOTE` | (unset) | Remote server URL. Setting it enables remote mode |
| `--remote <URL>` | overrides env | Force remote mode |
| `--guild <PATH>` | (unset) | Local mode's guild path. Auto-discovers from cwd if unset |
| `--json` | off | Machine-readable output (2-space pretty) |
| `--compact` | off | Single-line JSON — pipes/jq/log collection. Requires `--json` |

## 2. Command catalog

### 2.1 Guild / meta

```bash
openguild init [--name <NAME>]
openguild ping                   # server health check
openguild type list              # quest types (DEV/BUG/REQ) — `types` alias also works
openguild status list            # statuses (Open/In Progress/Done/...) — `statuses` alias also works
openguild tag list [--used]      # tag definition catalog (--used: also surfaces tags used without a definition, local only)
openguild tag add <slug> [--color HEX] [--description TEXT]
openguild tag update <slug> [--color HEX] [--description TEXT]
openguild tag delete <slug>      # tag usages themselves are preserved
```

### 2.2 Quest CRUD

```bash
openguild quest list [--json]
openguild quest list --type DEV,BUG --status open,in_progress --urgency 1-2
                    --has-prereq --no-sub --child-of <slug> --no-parent
                    --created-after 2026-05-01 --updated-before 2026-06-01
                    --search "keyword" --title-only
                    --sort urgency,id --reverse --limit 20 --offset 0
                    --id-only | --count                # script-friendly output
                    --table                            # aligned table for humans (mutually exclusive with --json/--tree)
# --table is common to list-style commands: quest list/deleted, campaign list,
# type list, status list, library list.

openguild quest search "<keyword>" [--title-only] [--limit N] [--id-only | --count]

openguild quest show <slug> [--field <NAME>]   # NAME: id/title/status/description/urgency/type/parent/created_at/updated_at

openguild quest new --type <PREFIX> --title <T>      # status starts as Open
                  [--description <DESC> | --description-file <PATH>]
                  [--urgency 1-4]                    # 1=Critical 4=Low (default 3)
                  [--parent <slug>]                  # create as a sub-quest

openguild quest update <slug> [--title] [--description | --description-file <PATH>]
                              [--urgency] [--dry-run]

openguild quest delete <slug> [--cascade <slug>,...] --yes        # --yes required
openguild quest delete <slug> [--cascade <slug>,...] --dry-run    # preview impact

openguild quest restore <slug>
openguild quest deleted
```

### 2.3 Status transitions

```bash
openguild quest move   <slug> <STATUS>   # canonical — any status (name or slug/ID)
openguild quest start  <slug>            # -> In Progress (shortcut)
openguild quest done   <slug>            # -> Done       (shortcut)
openguild quest reopen <slug>            # -> Open       (shortcut)
openguild quest status <slug>            # read-only: current status
```

Status names accept any case/spacing: `In Progress`, `in progress`,
`in_progress`, `in-progress` are all the same status.

#### Recommended flow

```
open -> in_progress -> testing -> done
                ↓        ↑
            (repeatable)
            cancelled
            on_hold (branch off if needed)
```

- **Changes you can fully verify with automated tests**: run the tests
  (`cargo test`, `npm test`, `npm run check`, etc.), and if they pass, go
  straight to `done`. Fix and re-run if something fails.
- **Changes that need a human to look at them** (UI/UX, external
  integrations): move to `testing` and let the human promote it to `done`.
  Attach a test plan to the body first (see below).
- If it's unclear which applies, `testing` is the safer choice.

#### Moving to `testing` — attach a test plan first

Right before or after `openguild quest move <slug> testing`, add a
**"## Test plan"** section to the quest body (description).

Example:
```bash
openguild quest update DEV-002 --description-file /tmp/body.md
openguild quest move DEV-002 testing
```
where `/tmp/body.md` contains something like:
```
Detects window.__TAURI__ -> invoke or fetch.

## Test plan
- `npm test -- --run` -> transport.test.ts passes
- `npm run check` -> 0 errors
- Manual: app loads correctly in a browser (fetch path)
```

What to include in a test plan:
- **Automated**: the command to run + the expected result
- **Manual verification**: what screen/behavior to check
- **Regression**: existing behavior this change could break
- **Expected output/files**: what should appear, and where

This is what tells the human what to verify, and keeps future-you (or
another agent) from losing context when revisiting the same quest.

### 2.4 Relations

```bash
openguild quest parent <slug> <parent-slug>
openguild quest parent <slug> --detach

openguild quest prereq add <slug> <prereq-slug>
openguild quest prereq rm  <slug> <prereq-slug>
```

### 2.5 Due dates

A quest can have a desired and a required due date (`YYYY-MM-DD`). The
**required due date** drives the Home dashboard's "upcoming"/"overdue"
sections. The desired due date is informational only.

```bash
openguild quest due <slug>                       # print both
openguild quest due <slug> --desired  2026-06-15
openguild quest due <slug> --required 2026-06-30
openguild quest due <slug> --clear-desired
openguild quest due <slug> --clear-required
```

Only `YYYY-MM-DD` is accepted. `--desired`/`--required` are mutually
exclusive with their matching `--clear-*`.

### 2.6 History

```bash
openguild quest history <slug>         # newest to oldest — status/type changes etc.
openguild quest history <slug> --json
```

### 2.7 Campaigns

A campaign is a "next milestone" plan — many-to-many linked to quests, plus
its own checklist and date range. Slugs look like `C-001`..`C-NNN`.

```bash
openguild campaign list [--status active|done|planned] [--json]
openguild campaign show <slug> [--json]

openguild campaign new --title <T> [--start <YYYY-MM-DD>] [--end <YYYY-MM-DD>]
openguild campaign delete <slug>

openguild campaign start <slug>              # status -> active
openguild campaign end   <slug>              # status -> done

openguild campaign link   <slug> <quest-slug>
openguild campaign unlink <slug> <quest-slug>

# checklist (1-based index, bidirectional with the body's `- [ ]`/`- [x]` lines)
openguild campaign checklist add     <slug> "<text>"
openguild campaign checklist check   <slug> <N>
openguild campaign checklist uncheck <slug> <N>
openguild campaign checklist rm      <slug> <N>
```

### 2.8 Backup / restore

```bash
openguild backup new
openguild backup list
openguild backup remove <TIMESTAMP>
openguild restore                      # restore latest snapshot (journal preserved)
openguild restore --to <TIMESTAMP>     # restore a specific snapshot
openguild restore --at <ISO8601-UTC>   # point-in-time restore — restore the latest
                                       # snapshot then replay the journal up to that
                                       # time. Mutually exclusive with --to. Rejected
                                       # if the range includes content ops (comment/
                                       # memo bodies), type changes, or attachments.
openguild restore --at latest          # replay the entire journal — the standard
                                       # entry point for recovering from corruption.
```

Auto-backup: a policy check runs after every mutation (snapshot after 50 ops
OR 24h elapsed by default). Tune with env `OPENGUILD_AUTO_BACKUP_OPS` /
`OPENGUILD_AUTO_BACKUP_HOURS`.

### 2.9 Comments / memos

```bash
openguild comments [--author A] [--since TS] [--until TS] [--grep T]
                   [--discussion | --unresolved] [--limit N] [--summary]
    # cross-guild (quest+campaign) comment search, latest 20 by default

openguild quest comment list <SLUG> [--author ... --since ... --top-only --grep ...]
openguild quest comment list <SLUG> --reply-to N          # replies to one entry only
openguild quest comment list <SLUG> --reverse --limit 5   # most recent
openguild quest comment list <SLUG> --tree                # indented reply tree
openguild quest comment show <SLUG> [--id N]
openguild quest comment show <SLUG> --id N --depth 2 --with-parents
openguild quest comment show <SLUG> --id N --depth all
openguild quest comment add <SLUG> --author <NAME> --file <PATH>
openguild quest comment add <SLUG> --author <NAME> --parent-id N --file <PATH>   # reply
openguild quest comment edit <SLUG> N --file <PATH>
openguild quest comment remove <SLUG> N [--force]
openguild quest comment react <SLUG> N <EMOJI> --author <NAME>   # toggle reaction
openguild quest comment discussion <SLUG> N               # toggle discussion flag (quest only)
openguild quest comment resolved <SLUG> N                 # toggle resolved (quest only)
openguild quest comment pinned <SLUG> N                   # toggle pin (quest/campaign both)
openguild quest memo set <SLUG> --file <PATH>             # private note (one per guild)
```

**Always pass `--author`** when adding a comment — a comment with no author
shows up as unattributed in the GUI and can't be traced back to who wrote it.
Use a stable identifier for yourself (e.g. `--author claude`).

Campaigns have the identical comment/memo structure:
```bash
openguild campaign comment list C-001 [--author ... --since ... --grep ...]
openguild campaign comment add C-001 --author <NAME> --file <PATH>
openguild campaign comment rm C-001 <ID> --force
openguild campaign memo set C-001 --file <PATH>
```

### 2.10 Quest templates

`.guild/templates/{name}.md` — same `+++` TOML frontmatter as a quest file
(`title`/`type`/`urgency`/`tags`, all optional) plus a default body.

```bash
openguild template list
openguild template show <NAME>
openguild quest new --template <NAME>
openguild quest new --template bug-report --title "Specific title"   # explicit options win
```

Merge priority: explicit options > template values > defaults (urgency
defaults to 3). Local mode only (not available over HTTP).

### 2.11 Maintenance / diagnostics / rules / library / worklog

```bash
openguild reindex                    # rebuild file -> index.db cache (= index rebuild)
openguild check drift                # check file <-> cache drift
openguild check counters             # check type counter consistency
openguild index rebuild | vacuum
openguild journal tail               # recent ops from the journal (AOF) — audit/debug
openguild info                       # guild meta / index.db, snapshot, journal summary

# Rules (.guild/rules/{slug}.md — project convention docs, git tracked)
# top-level command is singular `rule` only — no plural alias
openguild rule list
openguild rule show   <slug>
openguild rule new    <slug> --file <PATH>
openguild rule set    <slug> --file <PATH>   # replace body (idempotent)
openguild rule delete <slug> [--force]
openguild rule rename <old-slug> <new-slug>

# Library (.guild/library/ — reference docs/notes, own BOOK-N numbering, git tracked)
openguild library list
openguild library show   <ID>
openguild library new    --title <T> [--file <P>] [--path <F>]
openguild library update <ID> [--title <T>] [--file <P>] [--path <F>]
openguild library delete <ID> [--yes]

openguild library folder list
openguild library folder new    <PATH>
openguild library folder delete <PATH> [--yes]   # must be empty

# Worklog (activity = auto-aggregated quest history/comments/creation; notes = .guild/worklog/{date}.md)
openguild worklog show
openguild worklog show --date <YYYY-MM-DD>
openguild worklog show --from <D> --to <D>
openguild worklog note show  <YYYY-MM-DD>
openguild worklog note set   <YYYY-MM-DD> --file <P>
openguild worklog note clear <YYYY-MM-DD>
```

## 3. Workflow patterns

```bash
# Start new work
$ openguild quest new --type DEV --title "Implement OAuth" --json
{"id":47,"quest_id":"DEV-047","title":"Implement OAuth", ...}
$ openguild quest start DEV-047

# Split large work into sub-quests
$ openguild quest new --type DEV --title "Token issuance API" --parent DEV-047 --json
$ openguild quest new --type DEV --title "Token validation middleware" --parent DEV-047 --json

# Finish
$ openguild quest done DEV-047

# File a bug you found along the way
$ openguild quest new --type BUG --title "Login token doesn't expire" --urgency 2 --json

# Check what's in progress
$ openguild quest list --json | jq '.[] | select(.status_name_en == "In Progress")'

# Express a prerequisite ("token validation needs token issuance first")
$ openguild quest prereq add DEV-049 DEV-048
```

## 4. Safety guards

| Guard | What it does |
|---|---|
| Soft delete | `delete` doesn't remove the row — it just sets `deleted_at`. Restore with `restore`, list with `deleted` |
| `--yes` required | Deletes are rejected without an explicit `--yes` |
| `--dry-run` | Preview the impact of `delete`/`update` without actually calling it |
| Auto-backup | A policy check runs after every mutation (50 ops OR 24h elapsed by default) and snapshots `.guild/` source files. Tune with `OPENGUILD_AUTO_BACKUP_OPS`/`_HOURS` |
| Journal (AOF) | Every mutation is recorded as an op in `.guild/backups/journal.db` — inspect with `journal tail`; source of point-in-time restore (`restore --at`) |

### Recommended pattern
- Always `--dry-run` before a delete, review the result, then run with `--yes`
- Don't delete many quests in a loop
- Capture `--json` output and use the slug in follow-up calls

### Never edit `.guild/**` frontmatter by hand

openguild's architecture is **files = source of truth, `.guild/index.db` =
SQL cache**. Every mutation goes through the `openguild` CLI /
`openguild-server` HTTP / a Tauri invoke, which atomically updates the
journal, the files, and the SQL cache together.

Hand-editing `.guild/quests/*.md`, `.guild/types/*.toml`, or
`.guild/statuses/*.toml` directly causes **drift**:
- the SQL cache holds a stale value, so the GUI/`list` show a different state
- the intent isn't recorded in the journal, so snapshot/restore can't undo it
- type/status counters can get out of sync

Use the CLI command for the field you want to change instead:

| Field | Command |
|---|---|
| status | `openguild quest move <slug> <STATUS>` (or `start`/`done`/`reopen`) |
| title | `openguild quest update <slug> --title <T>` |
| description | `openguild quest update <slug> --description <D>` (multi-line is fine) |
| urgency | `openguild quest update <slug> --urgency 1-4` |
| parent | `openguild quest parent <slug> <parent>` / `--detach` |
| prerequisites | `openguild quest prereq add/rm <slug> <other>` |
| delete/restore | `openguild quest delete/restore <slug>` |
| type metadata | `openguild type add/update/delete` (`update --prefix` to rename+cascade) |
| status metadata | `openguild status add/update/delete` (`update --slug` to rename+cascade) |

If you absolutely must edit a file directly (the body/`description` field is
usually fine to hand-edit since the CLI already supports multi-line values):
1. Run `openguild reindex` immediately afterward (rebuild the SQL cache).
2. Run `openguild check drift` to confirm it's back to zero.
3. Note the reason in your commit message or the quest body — the journal
   won't record intent automatically.

**Never** hand-edit `status`/`urgency`/`parent`/`prerequisites` in
frontmatter — always use the commands above. The body (`description`) is the
only field safe to edit directly.

### Safe-delete example

```bash
# 1. Preview the impact
$ openguild quest delete DEV-047 --cascade DEV-048,DEV-049 --dry-run --json
{
  "dry_run": true,
  "would_delete": "DEV-047",
  "cascade_delete": ["DEV-048", "DEV-049"],
  "detach_children": [],
  "unaffected_prerequisites": []
}

# 2. Confirm, then execute
$ openguild quest delete DEV-047 --cascade DEV-048,DEV-049 --yes
```

## 5. Error handling

- **Exit code 0**: success
- **Exit code 1**: failure, with `error: ...` on stderr
- Local mode, no `.guild` found: stderr message + exit 1. Run `openguild init`.
- Remote mode, server down: HTTP error + exit 1. Check with `openguild ping` first.

```bash
if ! openguild ping >/dev/null 2>&1; then
    echo "openguild isn't reachable (local: no .guild found; remote: server down)" >&2
    exit 1
fi
```

## 6. Using JSON output

With `--json`, every output is pretty-printed (2-space) JSON, parseable with
`jq`/`serde_json`/etc.:

```bash
SLUG=$(openguild quest new --type DEV --title "X" --json | jq -r '.quest_id')
openguild quest start "$SLUG"
```

Add `--compact` for single-line JSON — useful for pipe chains, JSONL log
collection, or saving tokens. Pretty output stays the default for
backward-compat with existing scripts:

```bash
openguild quest list --json --compact | jq -c '.[] | {quest_id, status_slug}'
```

`--dry-run` also supports JSON mode, so impact analysis can be handled
programmatically.
