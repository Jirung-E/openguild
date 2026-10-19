# Quest templates

Templates live at `.guild/templates/<name>.md` and pre-fill new quests.

```bash
openguild template list
openguild template show <name>
openguild template new <name> [--type DEV] [--title "..."] [--urgency N] [--tags a,b] --file <PATH>
                              [--force]     # overwrite an existing template
openguild quest new --type <PREFIX> --title "..." --template <name>
```

Merge priority when creating from a template: explicit flags (e.g.
`--description-file`) > template content > built-in default.

Template list/show/new and `quest new --template` work in local and remote
(`--remote`) mode.
