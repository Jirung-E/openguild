# Reference library

The library is a notes/reference-docs store in the guild (own `BOOK-N` IDs),
organized into folders (design notes, research, links, planning documents,
etc. — anything worth keeping alongside the project but not tied to a single
quest). It also supports file attachments for large/binary material (PDFs,
zips, images) that don't belong as markdown body text — that was one of the
original reasons the library exists.

```bash
openguild library list [--table] [--folder "아키텍처"]
openguild library show <book-id>
openguild library new --title "..." [--file <PATH>] [--folder "folder/sub"]
openguild library update <book-id> [--title ...] [--file <PATH>] [--folder "..."]
openguild library delete <book-id> --yes

openguild library folder list
openguild library folder new <path>
openguild library folder move <from> <to>
openguild library folder delete <path> --yes

# REQ-028: export into a real folder tree (title becomes the filename)
openguild library export <dest-dir> [--folder "아키텍처" | --id BOOK-003]
```

`export` exists because **library folders are not real directories** — every
document lives flat in `.guild/library/BOOK-N.md` and its folder is just a field
on it. Taking documents out therefore has to *build* that tree: `아키텍처/라우터
설계.md`, with each document's attachments beside it in `<title>.attachments/`.
Files are written with their frontmatter intact, nothing is overwritten (a
colliding name gets ` (2)`), and the guild itself is not touched. The desktop app
has the same thing on the document and the current folder — **Export** (pick a
folder) and **Copy** (stages the tree, then puts it on the OS clipboard so you can
paste it in Finder/Explorer).

The title is `--title`, not a positional argument: the document's ID is the
auto-assigned `BOOK-N`, so the title is just a field. (Commands whose name *is*
the ID — `library folder new <path>`, `rule new <slug>` — take it positionally.)

**Put a document in a folder when you create it** — you do not need to create
the folder first, and you do not need a second command to move it:

```bash
openguild library new --title "이벤트 설계" --folder "아키텍처/결정"
openguild library list --folder "아키텍처"      # includes subfolders
openguild library list --folder ""              # top-level only
```

`--folder` was called `--path` until BUG-281; the old name still works as an
alias. `list --folder` filters in SQL (and, over `--remote`, on the server) — it
does not download the whole library and discard most of it. The folder is a logical grouping recorded in the document's frontmatter —
files stay flat under `.guild/library/`.

**Moving or renaming a folder** is one command — every subfolder and every
document under it follows, in a single locked change:

```bash
openguild library folder move 아키텍처/결정 보관/결정   # move — `보관` must already exist
openguild library folder move 보관 아카이브            # rename
```

It refuses: moving a folder into itself, a destination that already exists,
and a destination whose parent folder does not exist (create it first with
`library folder new` — a typo should not silently make a new folder). Do not
emulate it with `library update --folder` per document — subfolders and the
old folder entry would stay behind.

## Tags

Library docs share the free-tag catalog with quests and rules. Frontmatter
is the source of truth; works in local and remote (`--remote`) mode.

```bash
openguild library tag list <book-id>
openguild library tag add <book-id> <tag...>       # merged with existing, deduped
openguild library tag remove <book-id> <tag...>    # missing tags are ignored
openguild library tag set <book-id> [tag...]       # replace all; 0 args = clear
```

## Attachments

`--file` is for the markdown body (UTF-8 text). For large or binary files,
use attachments instead — a separate section from the body, same mechanism
as `quest attach`/`campaign attach`:

```bash
openguild library attach list <book-id>
openguild library attach add <book-id> <local-file-path> [--name "display name"]
openguild library attach remove <book-id> <path>          # path from `attach list`
```

Attachments work in local and remote (`--remote`) mode. Remote uploads stream
the file body instead of buffering/base64-encoding the whole file. Files are
copied into `.guild/attachments/`; removing the last reference to a file also
deletes the underlying blob (orphan cleanup).

Attachment bytes are **not** included in backups/snapshots — the files in
`.guild/attachments/` are the only copy, so back them up with the rest of
your project (git or otherwise).
