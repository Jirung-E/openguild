# Backup, restore, safety guards, errors, JSON

## Backup / restore

```bash
openguild backup new
openguild backup list
openguild backup remove <id>

openguild restore --to <id>
openguild restore --at YYYY-MM-DD
openguild restore --at latest
```

Backups are also taken automatically on risky operations (policy controlled
by env vars — see `--help` on `backup`/`restore` for the current defaults).

## Safety guards

| Guard | Behavior |
|---|---|
| Soft delete | `quest delete`/`campaign delete` mark deleted, don't erase — restorable |
| `--yes` required | Delete commands refuse to run without it (dry-run works without) |
| `--dry-run` | Preview a mutating command's effect without applying it (also supports `--json`) |
| Auto-backup | A snapshot is taken before destructive operations |
| Journal (AOF) | Every mutation is appended to a journal before the cache is updated, for crash recovery |

Recommended pattern: dry-run first, review, then re-run with `--yes`.

### Never edit `.guild/**` frontmatter by hand

`.guild/**` markdown files are the source of truth, but a SQL cache is
derived from them for fast queries. Hand-editing frontmatter (status,
urgency, parent, etc.) desyncs the cache from the files and silently breaks
listing/filtering until a `reindex`.

| Field | Command |
|---|---|
| status | `quest move` (or `start`/`done`/`reopen`) |
| title | `quest update --title` |
| description | `quest update --description` / `--description-file` |
| urgency | `quest update --urgency` |
| parent | `quest parent` / `quest parent --detach` |
| prerequisites | `quest prereq add` / `quest prereq remove` |
| delete / restore | `quest delete` / `quest restore` |
| type catalog | `type add` / `type update` / `type delete` |
| status catalog | `status add` / `status update` / `status delete` |

If you absolutely must edit a file directly (e.g. recovering from
corruption): 1) make the edit, 2) run `openguild reindex`, 3) run
`openguild check drift` to confirm the cache matches, 4) record why in a
journal entry/comment. **Never** hand-edit as a routine way to change quest
state — always prefer the CLI.

### Safe-delete example

```bash
openguild quest delete DEV-049 --dry-run --json    # preview
openguild quest delete DEV-049 --yes                # actually delete
openguild quest restore DEV-049                     # undo if needed
```

## Error handling

Exit code `0` = success, `1` = failure. Common failure cases:
- Local mode, no `.guild` found from cwd or `--guild` path.
- Remote mode, server unreachable (check with `openguild ping` first).

```bash
openguild ping || echo "server unreachable"
```

## Using JSON output

```bash
openguild quest list --json                       # pretty-printed
id=$(openguild quest new ... --json --compact | jq -r .quest_id)
openguild quest list --json --compact | jq -c '.[] | {id, status}'
openguild quest delete DEV-049 --dry-run --json --compact
```

`--compact` requires `--json` and is meant for piping (single line per
invocation, easy for `jq`/log collection).
