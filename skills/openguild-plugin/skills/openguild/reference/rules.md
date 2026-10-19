# Project rules

Rules are short project-convention/policy docs stored in the guild (e.g.
"how we name branches", "definition of done").

```bash
openguild rule list
openguild rule show <name>
openguild rule new <name> --file <PATH>        # or --empty
openguild rule set <name> --file <PATH>        # create or replace
openguild rule delete <name> --force           # without --force it asks (and cancels with no terminal)
openguild rule rename <name> <new-name>
openguild rule history <name>
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
