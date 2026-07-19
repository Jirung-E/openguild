# Reference library

The library is a notes/reference-docs store in the guild (own `BOOK-N` IDs),
organized into folders (design notes, research, links, planning documents,
etc. — anything worth keeping alongside the project but not tied to a single
quest). It also supports file attachments for large/binary material (PDFs,
zips, images) that don't belong as markdown body text — that was one of the
original reasons the library exists.

```bash
openguild library list [--table]
openguild library show <book-id>
openguild library new --title "..." [--file <PATH>] [--path "folder/sub"]
openguild library update <book-id> [--title ...] [--file <PATH>] [--path "..."]
openguild library delete <book-id> --yes

openguild library folder list
openguild library folder new <path>
openguild library folder delete <path> --yes
```

`--file` is for the markdown body (UTF-8 text). For large or binary files,
use attachments instead — a separate section from the body, same mechanism
as `quest attach`/`campaign attach`:

```bash
openguild library attach list <book-id>
openguild library attach add <book-id> <local-file-path> [--name "display name"]
openguild library attach remove <book-id> <path>          # path from `attach list`
```

Attachments are local-mode only (no remote-server support). Files are
copied into `.guild/attachments/`; removing the last reference to a file
also deletes the underlying blob (orphan cleanup).
