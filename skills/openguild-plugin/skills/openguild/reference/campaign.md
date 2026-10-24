# Campaigns

A campaign groups quests toward a milestone/release. Slugs look like
`C-001`, `C-002`, ...

```bash
openguild campaign list [--status active|done] [-q 'end:overdue quest:DEV-001']
openguild campaign show <slug>
openguild campaign new --title "..." [--start YYYY-MM-DD] [--end YYYY-MM-DD]
openguild campaign update <slug> [--title ...] [--description-file <PATH>] [--start ...] [--end ...]
openguild campaign history <slug>
openguild campaign delete <slug> --yes
openguild campaign start <slug>
openguild campaign end <slug>

openguild campaign link <campaign-slug> <quest-slug>
openguild campaign unlink <campaign-slug> <quest-slug>

openguild campaign checklist add <campaign-slug> "text"
openguild campaign checklist check <campaign-slug> <index>
openguild campaign checklist uncheck <campaign-slug> <index>
openguild campaign checklist remove <campaign-slug> <index>
```

### Filtering with a query

`campaign list -q` takes the same `key:value` syntax as `quest list -q`
(comma = OR within a key, separate terms = AND, leading `-` = NOT, bare words
search title, description and id) — only the keys differ:

| key | values |
|---|---|
| `status` | `active`, `done` |
| `start` / `end` | a date, `<=date`, `>=date`, `none`; `end:overdue` = past its end date and still active |
| `quest` | a quest id — campaigns that quest is linked to |

A key from another list (e.g. `urgency:`) is an error that names the keys you
can use. Over `--remote` the server filters (`GET /api/campaigns?q=`).


`<index>` is the item's current 1-based position, not a stable item ID.
Removing an item renumbers every item after it.

Campaigns also support comments/memos — see [comments.md](comments.md) — and
attachments (`openguild campaign attach list|add|remove`, same as `quest attach`).
