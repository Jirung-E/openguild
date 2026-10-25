# Guild metadata, maintenance, diagnostics

## Guild / meta catalogs

Types and statuses are the catalogs quests draw `--type`/status values from.
The top-level `tag` command manages tag definitions (display color and
description); attaching free-form labels to a document uses that document's
own `tag` subgroup.

```bash
openguild type list
openguild type add <PREFIX> --color "#4A90D9" [--description ...]
openguild type update <PREFIX> [--prefix NEW] [--color ...] [--description ... | --clear-description]
openguild type delete <PREFIX>                 # only when no quest uses it

openguild status list
openguild status add <name_en> --color "#..." [--name-ko ...] [--sort-order N]
openguild status update <slug> [--slug NEW] [--name-en ...] [--name-ko ...] [--color ...] [--sort-order N]
openguild status delete <slug>                 # only when no quest uses it

openguild tag list
openguild tag add <slug> [--color <HEX>] [--description <TEXT>]
openguild tag update <slug> [--color <HEX>] [--description <TEXT>]
openguild tag delete <slug>

openguild quest tag add <quest-slug> <tag...>
openguild library tag add <book-id> <tag...>
openguild rule tag add <rule-slug> <tag...>
```

The definition name is the tag itself, exactly as attached — Korean and
other non-ASCII names work (`openguild tag add 리팩터링 --color "#e94f4f"`).
Refused: whitespace, `/ \ : * ? " < > |`, a leading or trailing dot, Windows
reserved names (`CON`, `NUL`, `COM1`…), more than 64 characters, and a name
that differs from an existing definition only by case (the files would
collide on macOS/Windows).

**Renaming is a cascade.** `type update --prefix ISSUE` renames every quest of
that type (`BUG-001` → `ISSUE-001`: file names, frontmatter, the index), and
`status update --slug` rewrites every quest's status and its history. Ask the
user before doing either on a real guild.

`tag update` requires at least one of `--color` or `--description`. Deleting a
definition does not remove that tag from documents; it only removes its
catalog color/description.

## Maintenance / diagnostics

```bash
openguild reindex                 # rebuild the SQL cache from .guild/**
openguild check drift             # compare files vs. cache — and resync the cache if they differ
openguild check counters          # verify ID counters — and correct them if behind
openguild check sidecars          # comment/memo/attachment files whose document is missing (report only)
openguild check update            # is a newer release out? (no guild needed; tells only, never installs)
openguild index rebuild
openguild index vacuum
openguild journal tail            # tail the append-only journal
openguild info                    # guild summary (path, counts, version)
```

Catalog mutations and all diagnostics above work in local and remote
(`--remote`) mode. `migrate-to-files` is intentionally local-only because it
is a one-time offline migration of a guild directory.
