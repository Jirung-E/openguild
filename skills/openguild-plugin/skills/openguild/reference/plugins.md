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
| `on` | Event patterns. `quest.created`, `quest.*`, `*.created`, `*`. Prefix `pre:` to observe *before* the mutation (observation only — a plugin can never veto). |
| `scope` | **Required, no default.** Where it runs: `cli` / `gui` / `server`. Putting `server` in it applies the plugin to everyone using that server. |
| `action` | `post` (HTTP) or `run` (process, event JSON on **stdin**). These two are all the core owns. |
| `action.post.body_env` | `{ "chat_id": "TELEGRAM_CHAT_ID" }` — inject env values into named top-level body keys. Scripts have no I/O, so `payload()` cannot read the environment; APIs that want a private id *in the body* (Telegram's `chat_id`) need this. Only the named keys are touched, so user text is never expanded. |
| `script` | Optional `.rhai` file, relative to the plugin folder. |

`openguild plugin events` lists all 54 event names — quests, comments, campaigns,
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
openguild plugin trust --yes          # solo guild: allow everything, now and later
openguild plugin untrust              # undo that — per-plugin consent remains
openguild plugin events
```

While a guild is trusted, per-plugin `revoke` has no effect (`trusted` short-circuits
the check) — the CLI says so instead of reporting a silent success. Run `untrust` first.

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
| Desktop | Settings → Plugins (local guild only) |
| Server | Deliberately **not** over HTTP. Run the CLI on the server's machine. |

`GET /api/plugins` is **read-only** — it lists what is configured and what is running, and
answers `manageable: false`. Anyone who can read the guild can already read
`.guild/plugins/`, so the listing leaks nothing new; consent is what stays local.

The desktop app connected to a *remote* guild is read-only too: `invoke` still reaches the
local Store, so allowing there would edit consent for a guild you are not looking at.

Non-interactive CLI runs (scripts, CI) never prompt and never run unconsented
plugins — they print why instead.
