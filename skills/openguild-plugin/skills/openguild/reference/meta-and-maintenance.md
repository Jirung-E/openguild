# Guild metadata, maintenance, diagnostics

## Guild / meta catalogs

Types and statuses are the catalogs quests draw `--type`/status values from.
The top-level `tag` command manages tag definitions (display color and
description); attaching free-form labels to a document uses that document's
own `tag` subgroup.

```bash
openguild type list
openguild status list

openguild tag list
openguild tag add <slug> [--color <HEX>] [--description <TEXT>]
openguild tag update <slug> [--color <HEX>] [--description <TEXT>]
openguild tag delete <slug>

openguild quest tag add <quest-slug> <tag...>
openguild library tag add <book-id> <tag...>
openguild rule tag add <rule-slug> <tag...>
```

`tag update` requires at least one of `--color` or `--description`. Deleting a
definition does not remove that tag from documents; it only removes its
catalog color/description.

## Maintenance / diagnostics

```bash
openguild reindex                 # rebuild the SQL cache from .guild/**
openguild check drift             # compare files vs. cache, report mismatches
openguild check counters          # verify ID counters are consistent
openguild index rebuild
openguild index vacuum
openguild journal tail            # tail the append-only journal
openguild info                    # guild summary (path, counts, version)
```

Catalog mutations and all diagnostics above work in local and remote
(`--remote`) mode. `migrate-to-files` is intentionally local-only because it
is a one-time offline migration of a guild directory.
