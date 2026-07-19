# Comments and memos

Comments are visible discussion attached to a quest or campaign. Memos are
private notes not shown to others.

**Always pass `--author <name>`** — comments/memos need attribution. For
non-ASCII bodies, write the text to a UTF-8 file and use `--file`/
`--description-file` rather than piping through stdin/shell args.

## Cross-guild search

Same filter/sort/tree option set as `quest comment list` below — `comments`
is the cross-guild version (spans every quest and campaign).

```bash
openguild comments [--author <name>] [--since <ts>] [--until <ts>] [--grep "text"]
                    [--discussion] [--unresolved]
                    [--top-only | --reply-to <entry_id>] [--reverse] [--tree]
                    [--limit N]      # default 20
                    [--summary]      # default: full body; --summary = 60-char first line
```

`--tree` groups results by slug first, then renders each slug's replies as an
indented tree.

## Quest comments

```bash
openguild quest comment list <slug> [--author <name>] [--since <ts>] [--until <ts>]
                                     [--grep "text"] [--discussion] [--unresolved]
                                     [--top-only | --reply-to <id>] [--reverse]
                                     [--limit N] [--tree] [--summary]
openguild quest comment show <slug> [--id <id> [--depth N] [--with-parents] | --all]
openguild quest comment add <slug> --author <name> --file <PATH> [--parent-id <id>]
openguild quest comment edit <slug> <id> --file <PATH>
openguild quest comment remove <slug> <id>
openguild quest comment react <slug> <id> --emoji <emoji> --author <name>
openguild quest comment discussion <slug> <id>   # toggle
openguild quest comment resolved <slug> <id>     # toggle
openguild quest comment pinned <slug> <id>       # toggle

openguild quest memo set <slug> --author <name> --file <PATH>
```

`quest comment show` without `--id` prints the most recent 20 entries by
default — pass `--all` to lift that limit. With `--id`, `--depth`/
`--with-parents` control how much of that entry's thread to include instead.

## Campaign comments

Same shape, on a campaign instead of a quest:

```bash
openguild campaign comment list <campaign-slug> [same filters as quest comment list]
openguild campaign comment show <campaign-slug> [--id <id> [--depth N] [--with-parents] | --all]
openguild campaign comment add <campaign-slug> --author <name> --file <PATH> [--parent-id <id>]
openguild campaign comment edit <campaign-slug> <id> --file <PATH>
openguild campaign comment remove <campaign-slug> <id>
openguild campaign comment react <campaign-slug> <id> --emoji <emoji> --author <name>
openguild campaign comment pinned <campaign-slug> <id>   # toggle

openguild campaign memo set <campaign-slug> --author <name> --file <PATH>
```

(Campaigns don't have a discussion/resolved flag — that's quest-only.)
