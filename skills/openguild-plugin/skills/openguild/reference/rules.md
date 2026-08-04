# Project rules

Rules are short project-convention/policy docs stored in the guild (e.g.
"how we name branches", "definition of done").

```bash
openguild rule list
openguild rule show <name>
openguild rule new <name> --file <PATH>
openguild rule set <name> --file <PATH>
openguild rule delete <name>
openguild rule rename <name> <new-name>
```

## Tags

Rules share the same free-tag catalog as quests and library docs
(`.guild/tags/{slug}.toml`). The frontmatter of the rule file is the source
of truth. Works in both local and remote (`--remote`) mode.

```bash
openguild rule tag list <name>
openguild rule tag add <name> <tag...>       # merged with existing, deduped
openguild rule tag remove <name> <tag...>    # missing tags are ignored
openguild rule tag set <name> [tag...]       # replace all; 0 args = clear
```

Tags may be passed as separate arguments or space-separated in one
argument — `add <name> "docs review"` and `add <name> docs review` are the
same.
