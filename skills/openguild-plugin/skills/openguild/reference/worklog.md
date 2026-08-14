# Work log

The worklog is an activity/history feed for the guild, plus a free-form daily
note.

```bash
openguild worklog show [--date YYYY-MM-DD] [--from YYYY-MM-DD --to YYYY-MM-DD]

openguild worklog note show <DATE>
openguild worklog note set <DATE> [--file <PATH>]
openguild worklog note clear <DATE>
```

`<DATE>` is a required positional argument in `YYYY-MM-DD` format. `note set`
reads stdin when `--file` is omitted. The top-level `worklog show` command
still uses the optional `--date` flag.
