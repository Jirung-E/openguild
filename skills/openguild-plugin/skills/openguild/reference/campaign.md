# Campaigns

A campaign groups quests toward a milestone/release. Slugs look like
`C-001`, `C-002`, ...

```bash
openguild campaign list
openguild campaign show <slug>
openguild campaign new --title "..." [--start YYYY-MM-DD] [--end YYYY-MM-DD]
openguild campaign delete <slug> --yes
openguild campaign start <slug>
openguild campaign end <slug>

openguild campaign link <campaign-slug> <quest-slug>
openguild campaign unlink <campaign-slug> <quest-slug>

openguild campaign checklist add <campaign-slug> "text"
openguild campaign checklist check <campaign-slug> <item-id>
openguild campaign checklist uncheck <campaign-slug> <item-id>
openguild campaign checklist remove <campaign-slug> <item-id>
```

Campaigns also support comments/memos — see [comments.md](comments.md).
