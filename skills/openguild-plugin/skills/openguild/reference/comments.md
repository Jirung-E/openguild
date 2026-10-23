# Comments and memos

Comments are visible discussion attached to a quest or campaign. Memos are
private notes not shown to others.

For agent-authored comments, **always pass `--author <name>`** even though
`comment add` permits an empty author at the CLI syntax level. Reactions
require `--author` because toggling is per author. Memos have no author field
and therefore do not accept `--author`. For non-ASCII bodies, write the text
to a UTF-8 file and use `--file`/`--description-file` rather than piping
through stdin/shell args.

To read the comments on **one** quest or campaign, use `quest comment list <slug>`
/ `campaign comment list <slug>`. The guild-wide `comments` command is a search
tool for when you don't know which quest — see the last section.

## Quest comments

```bash
openguild quest comment list <slug> [--author <name>] [--since <ts>] [--until <ts>]
                                     [--grep "text"] [--discussion] [--unresolved]
                                     [--top-only | --reply-to <id>] [--reverse]
                                     [--limit N] [--tree] [--summary]
openguild quest comment show <slug> [--id <id> [--depth N] [--with-parents] | --all]
openguild quest comment add <slug> --author <name> --file <PATH> [--parent-id <id>] [--discussion]
openguild quest comment edit <slug> <id> --file <PATH>
openguild quest comment remove <slug> <id>
openguild quest comment react <slug> <id> <emoji> --author <name>
openguild quest comment discussion <slug> <id>   # toggle
openguild quest comment resolved <slug> <id>     # toggle
openguild quest comment pinned <slug> <id>       # toggle

openguild quest memo set <slug> --file <PATH>
```

Discussion is a **flag, not a text prefix.** Writing `[discussion]` at the
start of the body does nothing — the done-transition block, the
`comments --unresolved` filter, and the GUI's discussion badge all read the
flag. Pass `--discussion` on `add` to create one in a single step, or use
`comment discussion <slug> <id>` to toggle an existing entry.

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
openguild campaign comment react <campaign-slug> <id> <emoji> --author <name>
openguild campaign comment pinned <campaign-slug> <id>   # toggle

openguild campaign memo set <campaign-slug> --file <PATH>
```

(Campaigns don't have a discussion/resolved flag — that's quest-only.)

## Highlights

The user can highlight text in the app/web reading view — body, comments and
memo of a quest; a campaign; a rule; a library doc. A highlight is the user
marking what matters, so read them with the text. They are private like memos
(`.guild/highlights/`, kept out of git).

```bash
openguild quest highlight list <slug>
openguild campaign highlight list <campaign-slug>
openguild rule highlight list <name>
openguild library highlight list <book-id>
```

Each entry prints which part it is on (`body`, `memo`, `comment #N`), when it
was made, its id, then the highlighted text. `--json` adds the text just before
and after (`prefix` / `suffix`) — the app uses those to find the spot again
after the body changes. An unknown slug is an error, not an empty list.

**Add or remove a highlight only when the user asks.** Never highlight on your
own initiative — it clutters the user's reading view.

```bash
openguild quest highlight add <slug> "<text>" [--part body|memo|comment:N] [--nth N]
openguild quest highlight remove <slug> <highlight-id>
# campaign: body / memo / comment:N — rule and library: body only
```

- `<text>` is what the **reading view shows**, not the markdown source: drop
  `**`, backticks and `[..](..)`; a `[[dev-1]]` link shows as `DEV-1`. Spaces
  and line breaks are matched loosely; what is stored is the screen text.
- It must be inside one paragraph (or one code block). Across paragraphs →
  error; highlight each paragraph separately.
- Appears more than once → error with the count; choose with `--nth N` (from 1).
- Highlighting the same spot again returns the existing one instead of adding.
- `remove` with an id that isn't on that item is an error (exit 1).

## Guild-wide search

**Only for searching when you don't know which quest.** To read one quest's or
campaign's comments, use `quest comment list` / `campaign comment list` above —
`comments` mixes in every other quest and stops at 20 results by default.

Same filter/sort/tree option set as `quest comment list` — `comments` spans
every quest and campaign in the selected guild (not other guilds). It works in
local and remote (`--remote`) mode.

```bash
openguild comments [--author <name>] [--since <ts>] [--until <ts>] [--grep "text"]
                    [--discussion] [--unresolved]
                    [--top-only | --reply-to <entry_id>] [--reverse] [--tree]
                    [--limit N]      # default 20
                    [--summary]      # default: full body; --summary = 60-char first line
```

`--tree` groups results by slug first, then renders each slug's replies as an
indented tree.
