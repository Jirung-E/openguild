# Guild metadata, maintenance, diagnostics

## Guild / meta catalogs

Types and statuses are the catalogs quests draw `--type`/status values from;
tags are free-form labels on quests.

```bash
openguild type list
openguild status list

openguild tag list
openguild tag add <slug> <tag...>
openguild tag update <old> <new>
openguild tag delete <tag>
```

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
