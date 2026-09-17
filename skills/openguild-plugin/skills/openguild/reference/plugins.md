# Plugins — event hooks

> A plugin lets the guild call **your** program when something happens: a quest
> is created, a comment is added. Definitions are shared through git; consent to
> run them is not.

## Where things live

```
.guild/plugins/{name}/plugin.json    definition — committed, shared via git
.guild/plugins/{name}/*.rhai         judgement/shaping script — committed
~/.openguild/plugin-consent.json     consent — this machine only, NOT git
```

A plugin arriving via `git pull` does **not** run. Someone on this machine has
to allow it first. That split is the whole point: `run` actions are arbitrary
code execution, and `post` actions send guild content to a URL somebody else
chose.

## Ready-made examples

`examples/plugins/` in the repo holds three working plugins to copy — one `post`
with a script, one `run` without, and one using the observational `pre` phase.
Read `examples/plugins/README.md` before writing one from scratch.

## Definition

```json
{
  "name": "ai-notify",
  "description": "Sends new quests and comments to my service.",
  "on": ["quest.created", "comment.added"],
  "scope": ["cli", "gui"],
  "action": {
    "post": {
      "url": "https://my-service.example/hook",
      "headers": { "Authorization": "Bearer ${MY_API_KEY}" },
      "timeout_ms": 10000
    }
  },
  "script": "transform.rhai"
}
```

| Field | Meaning |
|---|---|
| `description` | Optional. One sentence, in the author's own words, shown in the desktop Admin → Plugins screen and `openguild plugin list`. Every other field is machine-readable, so without this a person deciding whether to allow the plugin has nothing saying what it is *for*. Max 500 characters; scanned for secrets like every other field. |
| `on` | Event patterns. `quest.created`, `quest.*`, `*.created`, `*`. Prefix `pre:` to observe *before* the mutation (observation only — a plugin can never veto). |
| `scope` | **Required, no default.** Where it runs: `cli` / `gui` / `server`. Putting `server` in it applies the plugin to everyone using that server. |
| `action` | `post` (HTTP) or `run` (process, event JSON on **stdin**). These two are all the core owns. |
| `action.post.body_env` | `{ "chat_id": "TELEGRAM_CHAT_ID" }` — inject env values into named top-level body keys. Scripts have no I/O, so `payload()` cannot read the environment; APIs that want a private id *in the body* (Telegram's `chat_id`) need this. Only the named keys are touched, so user text is never expanded. |
| `script` | Optional `.rhai` file, relative to the plugin folder. |
| `inputs` | Values the user must supply: `{key, label, type, help, secret, default, options}`. `type` is `text`/`checkbox`/`select`/`number`. Rendered as widgets in the desktop Admin → Plugins screen and stored per guild in `~/.openguild/plugin-values.json` (0600, never committed). Read them as `${KEY}` (always a string), in the script as `config("KEY")` (keeps its type), or — for `run` — as an environment variable of the child. Resolution: stored value → process environment → `default`. |

A `run` hook's working directory is `~/.openguild/plugin-data/{guild}/{plugin}/`,
**not** the plugin folder — a hook that writes next to itself changes the consent
fingerprint and silently revokes its own consent (BUG-279). `OPENGUILD_PLUGIN_DIR`
points at the plugin folder (usable as `${OPENGUILD_PLUGIN_DIR}` inside `command`
and `args`); `OPENGUILD_PLUGIN_DATA_DIR` at the data folder. Configured `inputs` also arrive
as **environment variables** of the child (`$ARCHIVE_DIR`) — a `run` hook has no url or
header to put them in, and passing a token through `args` would show it in `ps`.

`openguild plugin events` lists all 55 event names — quests, comments, campaigns,
the library, rules, attachments, and the guild's own vocabulary (types, statuses, tag
definitions). Comments and attachments carry `target: {kind, id}` rather than having
separate names per document kind, so one subscription covers all of them. Only a couple of
events have an observational `pre` phase; subscribing `pre:` to one that has none is a load
error rather than a subscription that silently never fires.

### Secrets are rejected as literals

`plugin.json` is committed. An API key written there is in the history
forever. Only environment references (`${MY_API_KEY}`) are accepted; a
literal that looks like a key makes the plugin **fail to load**, not warn.
The variable is read on this machine at send time, and a missing variable is
an error rather than an empty header. `${VAR}` is expanded in the POST url and
headers and in a `run` action's command and args. The `.rhai` source is scanned
for key literals too — it is committed the same way.

## Script (optional)

Two functions, both optional. No I/O exists inside — no files, no network, no
processes. That is why a script can be shared safely; the sending is the
core's job.

```rhai
fn should_send(event) {
    event.event == "comment.added" && event.ok && event.comment.discussion
}
fn payload(event) {
    #{ text: `[${event.quest.id}] ${event.comment.author}: ${event.comment.body}` }
}
```

Without `should_send` everything subscribed is sent; without `payload` the
event JSON itself is the body. Runaway scripts are stopped by an operation
cap and a wall-clock limit.

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
openguild plugin events

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

`allow` without `--yes` only prints what you would be consenting to. Consent is
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
