# Backup, restore, safety guards, errors, JSON

## Backup / restore

```bash
openguild backup new
openguild backup list
openguild backup remove <YYYYMMDD-HHMMSS>

openguild restore                              # latest snapshot only; journal preserved
openguild restore --to <YYYYMMDD-HHMMSS>      # exact snapshot; journal preserved
openguild restore --at <ISO8601-UTC>           # latest snapshot + journal replay through time
openguild restore --at latest                  # latest snapshot + complete journal replay
```

Use `backup list` to obtain snapshot timestamps. `--to` selects one exact
snapshot; it is not point-in-time journal replay. `--at` always starts from
the latest snapshot, replays the journal through the inclusive UTC timestamp
(for example `2026-06-27T00:15:00Z`), and then truncates the journal. Before a
destructive `--at` replay, the current state is automatically saved as a new
snapshot when the journal is non-empty.

## Safety guards

| Guard | Behavior |
|---|---|
| `quest delete` | Requires `--yes` for a real delete; uniquely supports `--dry-run`; soft-deleted quests can be restored |
| `campaign delete` | Requires `--yes`; no `--dry-run` option |
| `library delete` / `library folder delete` | Prompts interactively unless `--yes` is supplied |
| `rule delete` / `comment remove` | Prompts interactively unless `--force` is supplied |
| `tag delete` / `backup remove` | Executes immediately; no confirmation flag |
| Quest auto-snapshot | After quest mutations, the count/time policy may create a snapshot; this is not a per-delete confirmation or rollback guarantee |
| Journal (AOF) | Every mutation is appended to a journal before the cache is updated, for crash recovery |

Only use the dry-run/re-run pattern for commands that actually expose
`--dry-run` (currently `quest delete`). For other commands, check their
`--help` and use the confirmation mechanism shown above.

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
