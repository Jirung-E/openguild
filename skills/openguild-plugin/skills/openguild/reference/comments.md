# Comments and memos

Comments are visible discussion attached to a quest or campaign. Memos are
private notes not shown to others.

**Always pass `--author <name>`** — comments/memos need attribution. For
non-ASCII bodies, write the text to a UTF-8 file and use `--file`/
`--description-file` rather than piping through stdin/shell args.

## Cross-guild search

```bash
openguild comments --unresolved
openguild comments --author <name> --search "keyword"
```

## Quest comments

```bash
openguild quest comment list <slug> [--reply-to <id>] [--reverse] [--tree]
openguild quest comment show <slug> <id> [--depth N] [--with-parents]
openguild quest comment add <slug> --author <name> --file <PATH> [--reply-to <id>]
openguild quest comment edit <slug> <id> --file <PATH>
openguild quest comment remove <slug> <id>
openguild quest comment react <slug> <id> --emoji <emoji>
openguild quest comment discussion <slug> <id> [--resolved | --unresolved]
openguild quest comment pinned <slug> <id> [--pin | --unpin]

openguild quest memo set <slug> --author <name> --file <PATH>
```

## Campaign comments

Same shape, on a campaign instead of a quest:

```bash
openguild campaign comment list <campaign-slug>
openguild campaign comment add <campaign-slug> --author <name> --file <PATH>
openguild campaign comment edit <campaign-slug> <id> --file <PATH>
openguild campaign comment remove <campaign-slug> <id>
openguild campaign comment react <campaign-slug> <id> --emoji <emoji>
openguild campaign comment discussion <campaign-slug> <id> [--resolved | --unresolved]
openguild campaign comment pinned <campaign-slug> <id> [--pin | --unpin]

openguild campaign memo set <campaign-slug> --author <name> --file <PATH>
```
