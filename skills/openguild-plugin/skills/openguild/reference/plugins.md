# Plugins — event hooks

> A plugin lets the guild call **your** program when something happens: a quest
> is created, a comment is added. Definitions are shared through git; consent to
> run them is not.

## Where things live

```
.guild/plugins/{name}/plugin.toml    definition — committed, shared via git
.guild/plugins/{name}/*.rhai         judgement/shaping script — committed
.guild/plugins/{name}/*.test.rhai    tests for that script — committed, never run as code
~/.openguild/plugin-consent.json     consent — this machine only, NOT git
```

A plugin arriving via `git pull` does **not** run. Someone on this machine has
to allow it first. That split is the whole point: `run` actions are arbitrary
code execution, and `post` actions send guild content to a URL somebody else
chose.

## Ready-made examples

`examples/plugins/` in the repo holds five working plugins to copy — a multi-line one
(`telegram-quest-status`, with tests), a `run` one without a script (`desktop-notify`), one that
blocks in the `pre` phase (`deleted-audit`), and one that archives backups (`backup-archive`).
Read `examples/plugins/README.md` before writing one from scratch.

## Definition

A definition is a list of **lines**: when to look, and what to do. One plugin can have many.

```toml
#:schema ../plugin.schema.json

name        = "ai-notify"
description = "Sends new quests and discussion comments to my service."
scope       = ["cli", "gui"]
scripts     = ["main.rhai"]
permissions = ["notify"]          # only for notify()/backup()

# Where things go lives **here only** — the script knows names, not addresses.
[actions.out.post]
    url        = "https://my-service.example/hook"
    headers    = { Authorization = "Bearer ${MY_API_KEY}" }
    timeout_ms = 10000

# Line 1 — no script: a condition and a destination.
[[handlers]]
    id     = "urgent-quests"
    post   = ["quest.created"]
    action = "out"
    [handlers.when]
        "quest.urgency" = [1, 2]

# Line 2 — a script function decides.
[[handlers]]
    post = ["comment.added"]
    with = ["subject"]            # fn on_comment(e, subject)
    call = "on_comment"
```

| Field | Meaning |
|---|---|
| `name` | Unique in the guild — consent and every screen key off it. |
| `description` | Optional. One sentence, in the author's own words, shown in Admin → Plugins and `openguild plugin list`. Every other field is machine-readable, so without this a person deciding whether to allow the plugin has nothing saying what it is *for*. Max 500 characters; scanned for secrets like every other field. |
| `scope` | **Required, no default.** Where it runs: `cli` / `gui` / `server`. Putting `server` in it applies the plugin to everyone using that server. |
| `scripts` | `.rhai` files. Their functions share one namespace; a duplicate name (same arity) is a load error. |
| `permissions` | `notify` / `backup` — what the script may ask the *guild* to do. Calling one that is not declared fails that line and is reported. `send`/`run` need no permission: `[actions]` already shows where they go. |
| `actions` | Named destinations: `post` (HTTP) or `run` (process, event JSON on **stdin**). |
| `handlers` | The lines, in order. |
| `inputs` | Values the user must supply: `{key, label, type, help, secret, default, options}`. `type` is `text`/`checkbox`/`select`/`number`. Rendered as widgets in Admin → Plugins and stored per guild in `~/.openguild/plugin-values.json` (0600, never committed). Read them as `${KEY}` (always a string), in the script as `config("KEY")` (keeps its type), or — for `run` — as an environment variable of the child. Resolution: stored value → process environment → `default`. |

### A line

| Field | Meaning |
|---|---|
| `post` / `pre` | Event patterns — `quest.created`, `quest.*`, `*.created`, `*`. Exactly one of the two. `pre` runs **before** the mutation: it can block it or change field values, and always waits. Only a few events have a `pre` phase; asking for one that has none is a load error, not a subscription that silently never fires. |
| `call` / `action` | Exactly one. `call` names a script function; `action` is an `[actions]` name or an action written inline. |
| `when` | Conditions, all of which must hold: `"change.to" = ["done", "closed"]`. A listed value matches any of them; a field that is itself a list matches if it contains the value. Paths that cannot exist for this line's events are rejected at load. |
| `with` | Related data the function receives after the event: `subject`, `parent`, `children`, `prereqs`, `campaigns`, `quests`. What is available is decided by the **subject kind** of the event (`openguild plugin events` prints it per event), read straight from `.guild/` files. |
| `wait` | Wait for this line to finish before the command returns. Off by default — one slow hook must not stall the guild. `pre` lines always wait. |
| `id` | Line name, used by screens and error messages. |
| `timeout_ms` / `on_timeout` / `on_error` | Time budget, and what to do when it is exceeded or the script throws: `continue` (default) or `block` (`pre` lines only). |

`action.post.body_env` — `{ chat_id = "TELEGRAM_CHAT_ID" }` — injects env values into named
top-level body keys. Scripts have no I/O, so they cannot read the environment; APIs that want a
private id *in the body* (Telegram's `chat_id`) need this. Only the named keys are touched, so
user text is never expanded.

A `run` hook's working directory is `~/.openguild/plugin-data/{guild}/{plugin}/`,
**not** the plugin folder — a hook that writes next to itself changes the consent
fingerprint and silently revokes its own consent (BUG-279). `OPENGUILD_PLUGIN_DIR`
points at the plugin folder (usable as `${OPENGUILD_PLUGIN_DIR}` inside `command`
and `args`); `OPENGUILD_PLUGIN_DATA_DIR` at the data folder; `OPENGUILD_GUILD_DIR`
at the guild — a hook that calls `openguild` back needs it, because the working
directory is deliberately outside the guild
(`openguild --guild "$OPENGUILD_GUILD_DIR" …`). Configured `inputs` also arrive
as **environment variables** of the child (`$ARCHIVE_DIR`) — a `run` hook has no url or
header to put them in, and passing a token through `args` would show it in `ps`.

`openguild plugin events` lists all 55 event names — quests, comments, campaigns,
the library, rules, attachments, and the guild's own vocabulary (types, statuses, tag
definitions) — with each event's subject kind and the `with` names it allows. Comments and
attachments carry a subject of several possible kinds rather than having separate names per
document kind, so one subscription covers all of them.

### Secrets are rejected as literals

`plugin.toml` is committed. An API key written there is in the history
forever. Only environment references (`${MY_API_KEY}`) are accepted; a
literal that looks like a key makes the plugin **fail to load**, not warn.
The variable is read on this machine at send time, and a missing variable is
an error rather than an empty header. `${VAR}` is expanded in the POST url and
headers and in a `run` action's command and args. The `.rhai` source is scanned
for key literals too — it is committed the same way.

## Script (optional)

Function names are free; a line's `call` picks one. The first argument is the event, then one
argument per `with` name. **There is no I/O inside** — no files, no network, no processes. A
script *records* work and the core performs it after the function returns, outside the guild lock.

```rhai
fn on_comment(e, subject) {
    if !e.comment.discussion { return; }
    send("out", #{ text: `[${subject.id}] ${e.comment.author}: ${plain_text(e.comment.body)}` });
    notify(`${subject.id}: a discussion comment`);        // needs permissions = ["notify"]
}
```

| Call | What it does |
|---|---|
| `send(name, body)` / `run(name, input)` | Queue an `[actions]` action. The script never sees the url. |
| `notify(text)` / `backup()` | Ask the guild itself. Must be declared in `permissions`. |
| `config("KEY")` | A value the user configured. |
| `guild_name()`, `status_name(slug[, "ko"\|"en"])`, `type_name(prefix)`, `link(kind, id)` | Guild facts read by the core — events carry slugs, people read names. Unknown slugs come back unchanged. `link` uses `OPENGUILD_WEB_BASE` when set, otherwise an in-app path. |
| `status_info(slug)`, `type_info(prefix)` | The same as maps: `#{ slug, ko, en, color, done, order }` and `#{ prefix, description, color }`. Take this when the language of the place you are sending to is not this machine's setting, or to ask `done` ("does this status count as finished?") instead of hard-coding slugs. Type files hold one description, so types have no language choice. |
| `now()`, `ago(ts)`, `truncate(s, n)`, `plain_text(md)` | Time and text helpers. `truncate` counts characters, not bytes. |
| `import "path" as name` | Share helper files between plugins — outside the folder is allowed. The path must be a literal; consent is asked once and not re-asked when those files change (the consent screen says so). |

A `pre` function speaks through its **return value**: a string is the reason to block, a map is
the fields to change, `false` blocks without a reason. Returning nothing lets it through.

At most 32 queued commands per call; runaway scripts are stopped by an operation cap and a
wall-clock limit. One line throwing stops that line only.

## Tests

`{anything}.test.rhai` next to the plugin, with `test_`-prefixed zero-argument functions. Test
files are **not** listed in `scripts` — listing them would make them real code. They share the
namespace with the plugin's own scripts, so helpers can be called directly.

```rhai
fn test_done_goes_out() {
    set_config("ON_STATUS", true);                   // this test only
    let r = fire("quest.status_changed",
                 #{ ok: true, quest: #{ id: "DEV-1" }, change: #{ to: "done" } });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "out");
    assert(r.lines.contains("urgent-quests") == false, "wrong line ran");
}
```

`fire(event, payload[, with])` runs the **real** line selection, `when`, `with` and ordering —
the only differences are that commands are recorded instead of sent, and `with` comes from the
map you pass instead of guild files (so tests need no guild). Prefix the name with `pre:` for the
before phase. It returns `#{ lines, commands, blocked, changes, problems }`; `problems` holds the
failures that are otherwise only reported (undeclared permission, unknown action name, a throwing
script).

## CLI

```bash
openguild plugin list                 # running / awaiting consent / broken
openguild plugin allow ai-notify      # shows the definition + script, grants nothing
openguild plugin allow ai-notify --yes
openguild plugin revoke ai-notify
openguild plugin allow --all --yes    # every plugin present now (later ones still ask)
openguild plugin revoke --all         # every plugin present now
openguild plugin trust --yes          # auto-allow on: new or changed plugins run without asking
openguild plugin untrust              # auto-allow off: what runs keeps running, later changes ask
openguild plugin events               # event names, subject kinds, allowed `with`
openguild plugin check <folder>       # validate before running (exit 1 on errors)
openguild plugin test <folder>        # run *.test.rhai (exit 1 on failures)
openguild plugin schema --out plugin.schema.json   # editor autocomplete for plugin.toml

# plugins outside the guild — a "source" is a folder holding plugins (like a marketplace)
openguild plugin source add ~/dev/my-plugins   # register (this machine)
openguild plugin available                     # what the sources offer
openguild plugin add backup-archive            # use it in this guild (still needs allow)
openguild plugin add ~/dev/one-plugin          # a single plugin folder: register + use
openguild plugin remove backup-archive         # stop using it — files are not deleted
openguild plugin source list / remove <name>

# a running server keeps what it loaded at startup — reload it (from the server's own machine only)
openguild plugin reload --remote http://127.0.0.1:3000
```

**A plugin never receives a change it caused.** Every event carries
`origin: {"by": "user" | "plugin", "chain": [...]}` — the plugins that led to it. The runtime skips
any plugin already in the chain, so a hook that runs `openguild quest tag add` on
`quest.tags_changed` fires once, not forever; other plugins still get the event. A `run` child gets
the chain (plus itself) in `OPENGUILD_PLUGIN_CHAIN`, which the `openguild` CLI picks up; `--remote`
forwards it as the `X-OpenGuild-Plugin-Chain` header, and a `post` hook sends that header too.

A `run` may carry per-OS variants — `windows = { command = "powershell", args = [...] }`
(also `macos`, `linux`); on that OS it replaces `command`/`args`. Shell scripts need one for
Windows, which has no `sh`. Keep the `.ps1` ASCII-only and read stdin as UTF-8 explicitly:
Windows PowerShell 5.1 reads a BOM-less script in the local code page, so non-ASCII text there
breaks parsing (`pwsh` — PowerShell 7 — does not, but it is not installed by default).

`.guild/plugins/` is always loaded (it belongs to the guild and is shared through git).
Sources and "used in this guild" live only on this machine (`~/.openguild/plugin-sources.json`).
Plugins load from the source folder **in place** — no copy — so editing the original takes effect
(and re-asks consent, since the fingerprint changes) — immediately for the CLI, which reads plugins
on every run, and after a **reload** for the desktop app (Admin → Plugins → Reload) or a server
(`plugin reload --remote`). Nothing reloads automatically, on purpose: a definition being edited
must not start running the moment it is saved. The server accepts reload only from its own machine
(loopback, or the very address it is bound to). The desktop Admin → Plugins screen can also add a
folder (+ Add plugin), show each plugin's origin, stop using a source plugin, and list/remove sources. A name that exists both in the guild and
in a source is refused at load; a source folder that disappeared is reported, not silently dropped.

`allow --all` / `revoke --all` are bulk edits, not modes — per-plugin `allow`/`revoke`
still work afterwards. Auto-allow (`trust`) covers plugins added or changed later, but
a plugin you revoked stays off even while it is on.

`allow` without `--yes` prints what you would be consenting to, spelled out line by line: when
each line runs, whether it can block, what it reads, what it may ask the guild to do, where it
sends, which environment variables it uses (names only), which files it imports — followed by the
definition and the full script. Consent is
recorded against the definition **and every file in the plugin folder**, so
editing the script — or the `hook.py` a `run` action executes — asks again.

Management works on plugins of any `scope` — you can allow a `gui`-only plugin
from the CLI. Running is still limited to the matching component.

## Timing and failure

Delivery does not block the mutation. `openguild comment add` returns
immediately and the hook goes out on a separate thread; the CLI gives it a
short grace period before exiting (`OPENGUILD_PLUGIN_DRAIN_MS`, default 2000).
A hook that is slower than that is cut short, and you are told.

One failing plugin never affects the guild or the other plugins. Failures are
reported (stderr on CLI, startup lines on the server, the Plugins tab in the
desktop app) rather than swallowed.

## Where consent is given

| Component | How |
|---|---|
| CLI | `openguild plugin allow <name> --yes` |
| Desktop | Admin → Plugins (local guild only) |
| Server | Deliberately **not** over HTTP. Run the CLI on the server's machine. |

`GET /api/plugins` is **read-only** — it lists what is configured and what is running, and
answers `manageable: false`. Anyone who can read the guild can already read
`.guild/plugins/`, so the listing leaks nothing new; consent is what stays local.

The desktop app connected to a *remote* guild is read-only too: `invoke` still reaches the
local Store, so allowing there would edit consent for a guild you are not looking at.

Non-interactive CLI runs (scripts, CI) never prompt and never run unconsented
plugins — they print why instead.
