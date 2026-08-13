# Quest templates

Templates live at `.guild/templates/<name>.md` and pre-fill new quests.

```bash
openguild quest template list
openguild quest template show <name>
openguild quest new --type <PREFIX> --title "..." --template <name>
```

Merge priority when creating from a template: explicit flags (e.g.
`--description-file`) > template content > built-in default.

Template list/show/new and `quest new --template` work in local and remote
(`--remote`) mode.
